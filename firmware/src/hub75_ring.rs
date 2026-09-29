//! The HUB75 RING driver (Gitea #857 / #892; docs/hub75-ring-design.md;
//! feature `hub75-ring`): a fixed ring of row-pair slots in internal SRAM,
//! refilled just ahead of the beam by COPYING from a packed frame in PSRAM
//! that was packed ONCE per rendered frame.
//!
//! What replaces what, against `hub75.rs`'s two-buffer driver:
//!
//! - **No packed frame in internal RAM.** `N` slots of `planes + 1` rows of
//!   `cols` words each (`luxel_hub75::ring`), sized from the `panel` line's
//!   `ring_ms` of slack — 10 slots and 10 KB for a 64x64 at stock, instead
//!   of two 29 KB framebuffers. The DMA chain is `esp_hub75::fill_ring_chain`'s
//!   row-major circular chain over them (`Hub75::new_ring`), `suc_eof` on
//!   the ring's last slot only, so the driver's `frame_count()` IS the ring
//!   wrap count and `dma_position()` the slot the beam is in.
//! - **Pack once per FRAME, in PSRAM** (Jeremy, 2026-09-29, #892): `write_frame`
//!   packs the wire frame with `pie::pack_row_pair` into one of [`FRAMES`]
//!   packed frames in the arena — every row pair already in its slot format
//!   (ENTRY row, plane rows, control bits, the row's own address) — and
//!   points `newest` at it. The first ring build re-packed every PASS from
//!   RGB and its cost scaled with the pass rate (65–240 µs a row pair,
//!   mostly fixed per row pair; 2x1 saturated at `lsb 3`), while the content
//!   only changes at the frame rate. Now the per-pass work is a copy.
//! - **The copy is the refill.** A slot the beam has left is claimed out of
//!   one queue word (`ring::claim_next`: the counter and, in its top bit,
//!   which packed frame this pass reads) and its row pair is `memcpy`'d
//!   from that frame — 1–4 KB, cache-friendly and sequential, the one PSRAM
//!   access shape that costs little. Claims are taken only while
//!   [`Ring::fillable`] and while the slots left before the beam cover the
//!   copier's own worst copy; a claim that cannot make it is SKIPPED and
//!   counted `late` — the slot then shows the row it already held, at that
//!   row's own address (stale, never a mixed-address row, design §7).
//! - **Core 0 copies, from the output task; core 1 steals** (design §6):
//!   [`Hub75Ring::flush`] drains the queue on every turn of the output
//!   task's loop, and a periodic timer interrupt on the AppCpu
//!   ([`arm_steal`], TIMG1, the same interval) drains it too — an INTERRUPT,
//!   not the render task's vsync wait, which is empty whenever the pattern
//!   is slower than the panel. Both go through one [`drain`] over a
//!   [`Shared`] view of the ring (published once, from `flush`, when the
//!   driver's address is final); the queue word is the only thing they
//!   contend on, and a copier needs no pads, tables or template, so
//!   nothing is per-core but its copy-time estimate.
//!   `POST /api/ring {"steal":false}` is the lever back to core 0 alone.
//! - **Frame-atomic for free**: the claim of a pass's row 0 latches `newest`
//!   for the whole pass; four packed frames because two passes can be in
//!   flight on two frames while `newest` waits for the next pass and a
//!   fourth is being packed — one is always free. `ready_for_frame` is true
//!   once a new pass has been claimed since the last accepted frame, which
//!   paces the render task to the pass rate. No swap, no `Hub75Swap`, no
//!   pass audit; the `pass` block's swap counters read 0 on this driver.
//!
//! Counters (`/api/status` `pass.ring`): `rows` (slots), `slack_us`,
//! `late` (claims skipped), `blanked` and `backoffs` (#852, 0 here),
//! `packed_core0`, `packed_core1` (row pairs COPIED by each core),
//! `pack_us` / `pack1_us` (the typical copy per core), `pack_us_max`,
//! `frame_pack_us` (the once-per-frame pack), `steal`.

use core::cell::{RefCell, UnsafeCell};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use alloc::vec::Vec;
use esp_hal::dma::DmaDescriptor;
use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::peripherals::{DMA_CH0, LCD_CAM, TIMG1};
use esp_hal::timer::timg::TimerGroup;
use esp_hal::timer::PeriodicTimer;
use esp_hal::system::Cpu;
use esp_hal::Blocking;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_hub75::{Hub75, Hub75Pins16};
use esp_println::println;
use luxel_core::arena::ArrVec;
use luxel_core::layout::{LiveDriver, Matrix, PanelDriver};
use luxel_hub75::chip::ChipInit;
use luxel_hub75::pie::{self, PairPads};
use luxel_hub75::ring::{self, Claim, Ring};
use luxel_hub75::{arrange, Control, Geometry, Schedule, Tables};

use crate::hub75::{
    alloc_tables, board_default_matrix, build_remap, chip_init, clock_rate, control_of,
    schedule_of, Block, DynFb, BOOT_HEAP_FLOOR, LIVE, LIVE_COLS, LIVE_SCAN, MAX_SCAN, WANT_BLANK,
};
use crate::leds::Protocol;
use crate::output::OutputDriver;

/// The packed frames the copiers read from: two passes in flight, the
/// newest published, and one free to pack into.
const FRAMES: usize = 4;

/// Slack the packer keeps beyond its own worst pack when deciding whether
/// a claim can still make the beam, as a fraction of the pack time (1/2).
const PACK_MARGIN_SHIFT: u32 = 1;

// ---- counters for /api/status -------------------------------------------
pub static RING_ROWS: AtomicU32 = AtomicU32::new(0);
pub static RING_SLACK_US: AtomicU32 = AtomicU32::new(0);
pub static RING_LATE: AtomicU32 = AtomicU32::new(0);
pub static RING_BLANKED: AtomicU32 = AtomicU32::new(0);
pub static RING_BACKOFFS: AtomicU32 = AtomicU32::new(0);
pub static RING_PACKED_CORE0: AtomicU32 = AtomicU32::new(0);
pub static RING_PACKED_CORE1: AtomicU32 = AtomicU32::new(0);
/// Worst and typical (EWMA) row-pair pack, microseconds — what the skip
/// rule measures against.
pub static RING_PACK_US: AtomicU32 = AtomicU32::new(0);
pub static RING_PACK_US_MAX: AtomicU32 = AtomicU32::new(0);
/// Claims that were held because nothing was fillable when the packer
/// looked — the queue was full (a healthy sign, the ring is ahead).
pub static RING_IDLE: AtomicU32 = AtomicU32::new(0);
/// Core 1's typical row-pair pack, microseconds (core 0's is `RING_PACK_US`).
pub static RING_PACK1_US: AtomicU32 = AtomicU32::new(0);
/// The lever (design §6): may the render task steal claims? Runtime,
/// not persisted — `POST /api/ring {"steal":false}` for an A/B.
pub static STEAL: AtomicBool = AtomicBool::new(true);
/// The once-per-frame pack (`write_frame`), microseconds, EWMA.
pub static RING_FRAME_PACK_US: AtomicU32 = AtomicU32::new(0);
/// The steal's timer, kept so its handler can clear the interrupt.
static STEAL_TIMER: BlockingMutex<CriticalSectionRawMutex, RefCell<Option<PeriodicTimer<'static, Blocking>>>> =
    BlockingMutex::new(RefCell::new(None));

/// What both copiers read: raw views of the ring the output task owns.
/// Published once by [`Hub75Ring::publish`]; the driver never moves after
/// that (it lives inside `output_task`'s future, which never returns) and
/// nothing here is freed.
struct Shared {
    hub75: *const Hub75<Blocking, DynFb>,
    slots: *mut u16,
    slot_words: usize,
    /// Descriptors (emissions) per slot — `dma_position`'s index unit.
    emissions: usize,
    /// The packed frames (PSRAM, 16-aligned): row pair `r` of frame `f`
    /// is `slot_words` words at `packed[f] + r * slot_words`, slot-formatted.
    packed: [*const u16; FRAMES],
    ring: Ring,
    slot_clocks: u32,
    clock_hz: u32,
    slack_cycles: u32,
}
struct SharedCell(UnsafeCell<Option<Shared>>);
// SAFETY: written once before `SHARED_READY` is released, read-only after;
// the raw pointers inside are used under the queue's own claim discipline.
unsafe impl Sync for SharedCell {}
static SHARED: SharedCell = SharedCell(UnsafeCell::new(None));
static SHARED_READY: AtomicBool = AtomicBool::new(false);

fn shared() -> Option<&'static Shared> {
    if !SHARED_READY.load(Ordering::Acquire) {
        return None;
    }
    // SAFETY: read-only after the release above.
    unsafe { (*SHARED.0.get()).as_ref() }
}

/// One copier's own state — one per core: `pack_cycles` is the clipped
/// EWMA of a row-pair COPY the skip rule reads, measured on the core that
/// copies (a preemption on one core says nothing about the other).
struct Packer {
    /// Typical copy of one row pair in cycles (EWMA) — the skip rule.
    pack_cycles: u32,
    /// The queue has been brought level with the beam once (the first
    /// drain after boot); later catch-ups are stalls and count `late`.
    synced: bool,
}

impl Packer {
    const fn new() -> Self {
        Self { pack_cycles: 0, synced: false }
    }
}

/// The steal's copier (design §6): touched only by the steal interrupt on
/// the AppCpu, which cannot re-enter itself (one priority level).
static mut CORE1: Packer = Packer::new();

/// The queue word (`ring::Claim::encode`): the next claim's counter, and
/// the frame index of the pass being claimed in its top bit. CAS-claimed.
static NEXT_FILL: AtomicU32 = AtomicU32::new(0);
/// The packed frame most recently published by `write_frame`.
static NEWEST: AtomicU8 = AtomicU8::new(0);
/// The frame index each in-flight pass reads, by pass parity — written by
/// the claimer of that pass's row 0, read by `write_frame` to pick a free
/// frame. Two entries: the pass being emitted and the pass being packed.
static PASS_FRAME: [AtomicU8; 2] = [const { AtomicU8::new(0) }; 2];

pub struct Hub75Ring {
    hub75: Option<Hub75<Blocking, DynFb>>,
    g: Geometry,
    control: Control,
    sched: Schedule,
    ring: Ring,
    /// Slot `k` starts at word `k * slot_words`; 16-byte aligned. A raw
    /// pointer, not a slice: two cores write slots (each its own claims),
    /// so no `&mut` may cover the block.
    slots: *mut u16,
    slot_words: usize,
    /// Slot rows the DMA reads per slot, in order (`ring::slot_emissions`).
    order: Vec<u8>,
    /// The packed frames' storage (PSRAM arena) and their 16-aligned starts.
    packed: [ArrVec<u16>; FRAMES],
    packed_ptr: [*mut u16; FRAMES],
    /// Words per packed frame: `rows * slot_words`.
    frame_words: usize,
    tables: Option<&'static mut Tables>,
    tables_b5: u8,
    pads: PairPads,
    remap: Option<&'static [u16]>,
    /// The once-per-frame pack in cycles (EWMA), for `frame_pack_us`.
    frame_pack_cycles: u32,
    /// Pass of the last frame `write_frame` accepted; `ready_for_frame`
    /// waits for the queue to move past it.
    last_frame_pass: u32,
    /// Clocks per slot at the panel's pixel clock, for the skip rule.
    slot_clocks: u32,
    clock_hz: u32,
    /// The ring's usable slack in CPU cycles (`ring::slack_us`), the most
    /// time any claim can ever have: the ceiling on a believable pack.
    slack_cycles: u32,
    /// Core 0's packer — the output task's.
    packer: Packer,
    /// `SHARED` has been published (once, from the first `flush`).
    published: bool,
}

impl Hub75Ring {
    /// Build the panel from the STORED layout, falling back once to the board
    /// default if the configured shape cannot be built — `Hub75Output::new`'s
    /// contract.
    pub fn new(lcd_cam: LCD_CAM<'static>, pins: Hub75Pins16<'static>, channel: DMA_CH0<'static>) -> Self {
        let m = crate::layout::matrix();
        let d = crate::layout::driver();
        match Self::try_boot(lcd_cam, pins, channel, m, d, false) {
            Ok(me) => me,
            Err(why) => {
                let (dm, dd) = (board_default_matrix(), PanelDriver::default());
                if (m, d) == (dm, dd) {
                    println!("hub75-ring: {why} at the board default — panel output disabled");
                    return Self::dead();
                }
                println!("hub75-ring: {why} — falling back to the board default panel");
                // SAFETY: as in `Hub75Output::new` — the failed attempt
                // consumed the peripherals and pins; these steals are the
                // only live owners.
                let (lcd_cam, channel, pins) =
                    unsafe { (LCD_CAM::steal(), DMA_CH0::steal(), crate::board::hub75_pins_stolen()) };
                match Self::try_boot(lcd_cam, pins, channel, dm, dd, true) {
                    Ok(me) => me,
                    Err(why) => {
                        println!("hub75-ring: {why} at the board default — panel output disabled");
                        Self::dead()
                    }
                }
            }
        }
    }

    fn dead() -> Self {
        Self {
            hub75: None,
            g: Geometry::new(0, 0, 0),
            control: Control::new(0, 0),
            sched: Schedule::plan(Geometry::new(0, 0, 0), Control::new(0, 0), 0),
            ring: Ring::new(ring::GUARD_SLOTS + 1, 1),
            slots: core::ptr::null_mut(),
            slot_words: 0,
            order: Vec::new(),
            packed: [
                luxel_core::arena::empty(),
                luxel_core::arena::empty(),
                luxel_core::arena::empty(),
                luxel_core::arena::empty(),
            ],
            packed_ptr: [core::ptr::null_mut(); FRAMES],
            frame_words: 0,
            tables: None,
            tables_b5: u8::MAX,
            pads: PairPads::new(0),
            remap: None,
            frame_pack_cycles: 0,
            last_frame_pass: u32::MAX,
            slot_clocks: 1,
            clock_hz: 1,
            slack_cycles: 1,
            packer: Packer::new(),
            published: true, // nothing to publish: `steal` finds no `SHARED`
        }
    }

    fn try_boot(
        lcd_cam: LCD_CAM<'static>,
        mut pins: Hub75Pins16<'static>,
        channel: DMA_CH0<'static>,
        m: Matrix,
        d: PanelDriver,
        fallback: bool,
    ) -> Result<Self, &'static str> {
        crate::hub75::note_heap_before_panel();
        let Some(g) = arrange::fb_geometry(&m, usize::from(d.planes)) else {
            return Err("the arrangement has no framebuffer (odd ph, or scan does not divide ph/2)");
        };
        if g.planes == 0 || g.planes > luxel_hub75::MAX_PLANES {
            return Err("planes out of range");
        }
        if g.rows > MAX_SCAN {
            return Err("scan deeper than the five HUB75 address lines can reach");
        }
        if g.pixels() as u32 > crate::board::MAX_PIXELS {
            return Err("the panel is larger than this board's pixel cap");
        }
        if !g.cols.is_multiple_of(pie::LANES) {
            return Err("the ring driver needs a column count in whole vector lanes (a multiple of 8)");
        }
        let c = control_of(&d);
        let s = schedule_of(g, &d);
        // No trailing block: the ring's ENTRY row per row pair does that job
        // (luxel_hub75::ring module docs).
        let stripes = arrange::stripes(&m);
        let (w, h) = (g.cols / stripes, 2 * g.rows * stripes);
        let clock_hz = d.clock_hz();
        let n = ring::slots_for_slack(u32::from(d.ring_ms) * 1000, &s, g.cols, clock_hz, g.rows);
        let ring_ = Ring::new(n, g.rows as u32);
        let slot_words = ring::slot_words(g.planes, g.cols);
        let order: Vec<u8> = ring::slot_emissions(&s).map(|r| r as u8).collect();
        let row_bytes = g.cols * 2;

        // ---- memory: slots (internal, 16-aligned), descriptors, tables, pads
        let slots_layout = core::alloc::Layout::from_size_align(n as usize * slot_words * 2, 16)
            .map_err(|_| "bad slot layout")?;
        let slots_block = Block::zeroed(slots_layout).ok_or("ring slot alloc failed")?;
        let descs = esp_hub75::ring_descriptor_count(n as usize, order.len(), row_bytes);
        let desc_layout = core::alloc::Layout::array::<DmaDescriptor>(descs).map_err(|_| "bad descriptor layout")?;
        let desc_block = Block::zeroed(desc_layout).ok_or("DMA descriptor alloc failed")?;
        let (tables_block, tables) = alloc_tables().ok_or("packer table alloc failed")?;
        let slots: *mut u16 = slots_block.ptr.cast::<u16>();
        {
            // SAFETY: the block is `n * slot_words` zeroed, 16-byte-aligned
            // u16s we own for the life of the driver (leaked below); this
            // is the only whole-block borrow, before the DMA or any packer
            // exists. Pre-fill: slot k carries row pair k (dark) with its
            // template.
            let all = unsafe { core::slice::from_raw_parts_mut(slots, n as usize * slot_words) };
            for (k, slot) in all.chunks_exact_mut(slot_words).enumerate() {
                ring::format_slot_for(slot, g, c, &s, k % g.rows);
            }
        }
        let slot_bases: Vec<*const u8> =
            // SAFETY: in-bounds offsets of the block above.
            (0..n as usize).map(|k| unsafe { slots.add(k * slot_words) }.cast::<u8>().cast_const()).collect();
        // SAFETY: `descs` EMPTY-initialised descriptors in a block we own.
        let descriptors: &'static mut [DmaDescriptor] = unsafe {
            let p = desc_block.ptr.cast::<DmaDescriptor>();
            for i in 0..descs {
                p.add(i).write(DmaDescriptor::EMPTY);
            }
            core::slice::from_raw_parts_mut(p, descs)
        };

        // ---- the packed frames, in the arena (PSRAM): every row pair in
        // its slot format, 16-aligned for the vector packer's stores, and
        // pre-formatted dark so the first passes (before any frame) emit
        // valid control bits at the right addresses.
        let frame_words = g.rows * slot_words;
        let mut packed: [ArrVec<u16>; FRAMES] = [
            luxel_core::arena::empty(),
            luxel_core::arena::empty(),
            luxel_core::arena::empty(),
            luxel_core::arena::empty(),
        ];
        let mut packed_ptr: [*mut u16; FRAMES] = [core::ptr::null_mut(); FRAMES];
        for (f, v) in packed.iter_mut().enumerate() {
            if v.try_reserve_exact(frame_words + 8).is_err() {
                return Err("packed frame alloc failed");
            }
            v.resize(frame_words + 8, 0);
            let off = v.as_mut_ptr().align_offset(16);
            // SAFETY: `off` < 8 elements, and the Vec holds `frame_words + 8`.
            let base = unsafe { v.as_mut_ptr().add(off) };
            let all = unsafe { core::slice::from_raw_parts_mut(base, frame_words) };
            for (r, block) in all.chunks_exact_mut(slot_words).enumerate() {
                ring::format_slot_for(block, g, c, &s, r);
            }
            packed_ptr[f] = base;
        }
        let pads = PairPads::for_geometry(g);

        let took = crate::hub75::heap_before_panel().saturating_sub(esp_alloc::HEAP.free());
        let left = esp_alloc::HEAP.free();
        if left < BOOT_HEAP_FLOOR {
            println!(
                "hub75-ring: this panel leaves {} B of heap for WiFi, the web server and the engine (floor {} B)",
                left, BOOT_HEAP_FLOOR
            );
            return Err("the ring leaves too little heap for the rest of the boot");
        }
        let slack_us = ring::slack_us(n, &s, g.cols, clock_hz);
        println!(
            "hub75-ring: {} slots x {} B = {} B internal for {} ms asked ({} us slack), {} descriptors x 12 B, \
             {} emissions/pair, panel took {} B of heap, {} B left (floor {} B)",
            n,
            slot_words * 2,
            n as usize * slot_words * 2,
            d.ring_ms,
            slack_us,
            descs,
            order.len(),
            took,
            left,
            BOOT_HEAP_FLOOR,
        );
        let remap = build_remap(&m, g);
        println!(
            "hub75-ring: {}x{} tiles of {}x{} from {} {}{}{}, framebuffer {}x{} scan 1/{}, remap {}, est {} Hz",
            m.cols,
            m.rows,
            m.pw,
            m.ph,
            m.start.as_str(),
            m.dir.as_str(),
            if m.snake { " snake" } else { "" },
            if m.rot180 { " rot180" } else { "" },
            w,
            h,
            g.rows,
            if remap.is_some() { "on" } else { "off (row-major)" },
            clock_hz / (ring::slot_clocks(&s, g.cols) as u32 * g.rows as u32),
        );
        crate::hub75::print_schedule(&s, g, &d);
        if d.chip.needs_init() {
            let k = chip_init(&mut pins, d.chip, g.cols);
            println!("hub75-ring: {} init sequence sent ({} clocks)", d.chip.as_str(), k);
        }

        match Hub75::<Blocking, DynFb>::new_ring(
            lcd_cam,
            pins,
            channel,
            descriptors,
            clock_rate(&d),
            &slot_bases,
            row_bytes,
            &order,
            n as usize,
        ) {
            Ok(hub75) => {
                println!(
                    "hub75-ring: {}x{} panel, scan 1/{}, {} bitplanes, LCD_CAM @ {} MHz, chip {}, blank {}, \
                     circular DMA over the ring{}",
                    w,
                    h,
                    g.rows,
                    g.planes,
                    d.clock_mhz,
                    d.chip.as_str(),
                    d.blank,
                    if fallback { " — FALLBACK, the configured panel would not build" } else { "" },
                );
                let live = LiveDriver {
                    planes: d.planes,
                    clock_mhz: d.clock_mhz,
                    chip: d.chip,
                    blank: d.blank,
                    lsb: s.lsb,
                    w: w as u16,
                    h: h as u16,
                    scan: g.rows as u16,
                    fb_bytes: (n as usize * slot_words * 2) as u32,
                    fallback,
                    ring_ms: d.ring_ms,
                    ring_rows: n as u16,
                    ring_slack_us: slack_us,
                };
                LIVE.lock(|c| c.set(Some(live)));
                LIVE_SCAN.store(g.rows as u16, Ordering::Relaxed);
                LIVE_COLS.store(g.cols as u16, Ordering::Relaxed);
                WANT_BLANK.store(d.blank, Ordering::Relaxed);
                RING_ROWS.store(n, Ordering::Relaxed);
                RING_SLACK_US.store(slack_us, Ordering::Relaxed);
                NEXT_FILL.store(Claim { buf: 0, abs: ring_.first_claim() }.encode(), Ordering::Relaxed);
                let _ = slots_block.leak();
                let _ = desc_block.leak();
                let _ = tables_block.leak();
                Ok(Self {
                    hub75: Some(hub75),
                    g,
                    control: c,
                    sched: s,
                    ring: ring_,
                    slots,
                    slot_words,
                    order,
                    packed,
                    packed_ptr,
                    frame_words,
                    tables: Some(tables),
                    tables_b5: u8::MAX,
                    pads,
                    remap,
                    frame_pack_cycles: 0,
                    last_frame_pass: u32::MAX,
                    slot_clocks: ring::slot_clocks(&s, g.cols) as u32,
                    clock_hz,
                    slack_cycles: slack_us.saturating_mul(240).max(1),
                    packer: Packer::new(),
                    published: false,
                })
            }
            Err(e) => {
                println!("hub75-ring: LCD_CAM init failed at {} MHz: {:?}", d.clock_mhz, e);
                let _ = slots_block.leak();
                let _ = desc_block.leak();
                let _ = tables_block.leak();
                Err("LCD_CAM init failed")
            }
        }
    }

    /// Publish the cross-core view of the ring (once). Called from `flush`,
    /// i.e. from inside `output_task`'s future, whose address is final.
    fn publish(&mut self) {
        self.published = true;
        let Some(h) = self.hub75.as_ref() else { return };
        if self.packed_ptr.iter().any(|p| p.is_null()) {
            return;
        }
        let sh = Shared {
            hub75: h as *const _,
            slots: self.slots,
            slot_words: self.slot_words,
            emissions: self.order.len().max(1),
            packed: [
                self.packed_ptr[0].cast_const(),
                self.packed_ptr[1].cast_const(),
                self.packed_ptr[2].cast_const(),
                self.packed_ptr[3].cast_const(),
            ],
            ring: self.ring,
            slot_clocks: self.slot_clocks,
            clock_hz: self.clock_hz,
            slack_cycles: self.slack_cycles,
        };
        // SAFETY: the single write, before the release below.
        unsafe { *SHARED.0.get() = Some(sh) };
        SHARED_READY.store(true, Ordering::Release);
    }

    /// Pack the wire frame into packed frame `i`: every row pair formatted
    /// for its slot (control bits, its own address) and its colour bits
    /// packed by the vector kernel. Only the output task packs, so the
    /// brightness tables are its own. Between chunks of row pairs the
    /// queue is drained, so a long pack (8 ms at 4x1) does not hold core
    /// 0's refill off for a whole ring. Returns false if the frame is
    /// shorter than the panel (nothing written).
    fn pack_frame(&mut self, i: usize, rgb: &[[u8; 3]]) -> bool {
        if rgb.len() < self.g.pixels() {
            return false;
        }
        let Some(t) = self.tables.as_deref_mut() else { return false };
        let b5 = crate::out_brightness();
        if self.tables_b5 != b5 {
            t.build_scale5(b5);
            self.tables_b5 = b5;
        }
        let t0 = esp_hal::xtensa_lx::timer::get_cycle_count();
        let dst = self.packed_ptr[i];
        let (g, control, sched, slot_words, rows) = (self.g, self.control, self.sched, self.slot_words, self.g.rows);
        for r in 0..rows {
            // SAFETY: frame `i` is free (`free_frame`: no pass reads it and
            // it is not `newest`) and the output task is the only packer.
            let words = unsafe { core::slice::from_raw_parts_mut(dst.add(r * slot_words), slot_words) };
            ring::format_slot_for(words, g, control, &sched, r);
            let planes = &mut words[ring::plane_row(0) * g.cols..];
            let _ = pie::pack_row_pair(planes, g, r, rgb, self.remap, t, &mut self.pads);
            if r % 8 == 7 {
                if let Some(s) = shared() {
                    drain(&mut self.packer, s, 0);
                }
            }
        }
        let dt = esp_hal::xtensa_lx::timer::get_cycle_count().wrapping_sub(t0);
        self.frame_pack_cycles =
            if self.frame_pack_cycles == 0 { dt } else { (self.frame_pack_cycles * 7 + dt.min(self.frame_pack_cycles * 2)) / 8 };
        RING_FRAME_PACK_US.store(self.frame_pack_cycles / 240, Ordering::Relaxed);
        true
    }

    /// A frame index no in-flight pass reads and that is not `newest`.
    fn free_frame(&self) -> Option<u8> {
        let newest = NEWEST.load(Ordering::Acquire);
        let a = PASS_FRAME[0].load(Ordering::Acquire);
        let b = PASS_FRAME[1].load(Ordering::Acquire);
        (0..FRAMES as u8).find(|&i| i != newest && i != a && i != b)
    }

    /// How often the output task should turn the loop: a quarter of the
    /// ring's slack, floored at 500 µs.
    pub fn poll_interval(&self) -> embassy_time::Duration {
        let us = RING_SLACK_US.load(Ordering::Relaxed) / 4;
        embassy_time::Duration::from_micros(u64::from(us.max(500)))
    }
}

/// Where the beam is, as the ring's absolute counter: ring wraps (the
/// EOF count — one EOF per ring, on its last slot) times `n` plus the
/// slot of the descriptor being read. A raised-but-unserviced EOF is a
/// wrap the count has not seen yet.
fn abs_dma(s: &Shared) -> Option<u32> {
    // SAFETY: the driver outlives every caller (see `Shared`); both probes
    // read statics behind their own multicore-safe locks.
    let hub75 = unsafe { &*s.hub75 };
    let (_ring, idx, eof_pending) = hub75.dma_position()?;
    let wraps = hub75.frame_count().wrapping_add(u32::from(eof_pending));
    let slot = (idx / s.emissions) as u32;
    Some(s.ring.abs(wraps, slot))
}

/// Copy one claimed row pair from its pass's packed frame into its slot.
fn copy_claim(s: &Shared, claim: Claim) {
    let slot = s.ring.slot(claim.abs) as usize;
    let row = s.ring.row(claim.abs) as usize;
    // SAFETY: this slot is ours by the CAS on the queue word — no other
    // copier holds the claim — and the DMA has left it (`fillable`). The
    // packed frame a pass reads is kept out of `write_frame`'s choice
    // (`NEWEST` / `PASS_FRAME`) for as long as the pass is in flight, and
    // row `row` of it is `slot_words` words at `row * slot_words`.
    let src = unsafe { core::slice::from_raw_parts(s.packed[usize::from(claim.buf) % FRAMES].add(row * s.slot_words), s.slot_words) };
    let dst = unsafe { core::slice::from_raw_parts_mut(s.slots.add(slot * s.slot_words), s.slot_words) };
    dst.copy_from_slice(src);
}

/// Drain the queue with copier `p` on `core`: claim and copy every slot
/// the beam has left that can still be filled in time. Returns how many
/// were copied. Reentrant across cores: the queue word is the only shared
/// write, and a CAS that loses simply looks again.
fn drain(p: &mut Packer, s: &Shared, core: usize) -> u32 {
    let Some(abs_dma) = abs_dma(s) else { return 0 };
    let (packed_ctr, pack_us) = if core == 0 { (&RING_PACKED_CORE0, &RING_PACK_US) } else { (&RING_PACKED_CORE1, &RING_PACK1_US) };
    let mut packed = 0u32;
    loop {
        let word = NEXT_FILL.load(Ordering::Acquire);
        let (claim, next) = ring::claim_next(word, NEWEST.load(Ordering::Acquire), &s.ring);
        if s.ring.late(claim.abs, abs_dma) {
            // The beam has overtaken the queue head — the first drain
            // after boot (the DMA has run since `new_ring`, seconds
            // before the output task's first turn) or a stall long
            // enough to lap the ring. Nothing behind the beam can be
            // packed and a late head never becomes fillable by itself,
            // so re-join right after the beam and count what was
            // skipped (the boot catch-up is not a late row: the ring
            // was dark and expected to be). The pass being re-joined
            // reads `newest`, and `PASS_FRAME` says so, so `write_frame`
            // keeps its hands off that frame.
            let buf = NEWEST.load(Ordering::Acquire);
            let target = s.ring.catch_up(abs_dma);
            let word2 = Claim { buf, abs: target }.encode();
            if NEXT_FILL.compare_exchange(word, word2, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                PASS_FRAME[(s.ring.pass(target) % 2) as usize].store(buf, Ordering::Release);
                if p.synced {
                    RING_LATE.fetch_add(s.ring.dist(claim.abs, target), Ordering::Relaxed);
                }
            }
            p.synced = true;
            continue;
        }
        if !s.ring.fillable(claim.abs, abs_dma) {
            if packed == 0 {
                RING_IDLE.fetch_add(1, Ordering::Relaxed);
            }
            break;
        }
        // The skip rule (design §7): slots left before the beam, less
        // the one it may be in, must cover the worst pack plus margin.
        let left_clocks = s.ring.until_late(claim.abs, abs_dma).saturating_sub(1) * s.slot_clocks;
        let left_cycles = (u64::from(left_clocks) * 240_000_000 / u64::from(s.clock_hz.max(1))) as u32;
        let need = p.pack_cycles + (p.pack_cycles >> PACK_MARGIN_SHIFT);
        // An estimate the whole ring cannot cover is not a pack time,
        // it is a preemption (esp-wifi's scheduler owns core 0 too:
        // one 590 ms "pack" during WiFi bring-up on 2026-09-28 left the
        // ×4 ring skipping every claim as late for good — the spare-plane
        // freeze of #620 in a new coat). Forget it and measure again.
        if need > s.slack_cycles {
            p.pack_cycles = 0;
        }
        let need = p.pack_cycles + (p.pack_cycles >> PACK_MARGIN_SHIFT);
        if p.pack_cycles != 0 && left_cycles < need {
            if NEXT_FILL.compare_exchange(word, next, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                RING_LATE.fetch_add(1, Ordering::Relaxed);
                if s.ring.row(claim.abs) == 0 {
                    PASS_FRAME[(s.ring.pass(claim.abs) % 2) as usize].store(claim.buf, Ordering::Release);
                }
            }
            continue;
        }
        if NEXT_FILL.compare_exchange(word, next, Ordering::AcqRel, Ordering::Relaxed).is_err() {
            continue; // the other core took it
        }
        if s.ring.row(claim.abs) == 0 {
            PASS_FRAME[(s.ring.pass(claim.abs) % 2) as usize].store(claim.buf, Ordering::Release);
        }
        let t0 = esp_hal::xtensa_lx::timer::get_cycle_count();
        copy_claim(s, claim);
        let dt = esp_hal::xtensa_lx::timer::get_cycle_count().wrapping_sub(t0);
        // The typical pack, not the worst: a sample can at most double
        // the estimate (a preempted pack decays out over the next few
        // clean ones), and the first sample is bounded by the slack.
        // `pack_us_max` keeps the raw worst.
        let clipped = if p.pack_cycles == 0 { dt.min(s.slack_cycles) } else { dt.min(p.pack_cycles * 2) };
        p.pack_cycles = if p.pack_cycles == 0 { clipped } else { (p.pack_cycles * 7 + clipped) / 8 };
        pack_us.store(p.pack_cycles / 240, Ordering::Relaxed);
        RING_PACK_US_MAX.fetch_max(dt / 240, Ordering::Relaxed);
        packed_ctr.fetch_add(1, Ordering::Relaxed);
        packed += 1;
    }
    packed
}

/// Arm the core-1 steal on THIS core (design §6): call from the AppCpu's
/// init closure, before the render task runs. A periodic timer interrupt
/// on TIMG1 at a quarter of the ring's slack (≥ 500 µs, the output task's
/// own poll) drains the queue in interrupt context — the one home on that
/// core that a 30 ms render frame cannot hold off. A copier needs no
/// scratch, so nothing is allocated. A no-op when there is no panel
/// (`RING_SLACK_US` 0).
pub fn arm_steal(timg1: TIMG1<'static>) {
    let slack = RING_SLACK_US.load(Ordering::Relaxed);
    if slack == 0 {
        return;
    }
    let us = u64::from((slack / 4).max(500));
    let mut t = PeriodicTimer::new(TimerGroup::new(timg1).timer0);
    t.set_interrupt_handler(InterruptHandler::new(steal_isr, Priority::Priority1));
    if t.start(esp_hal::time::Duration::from_micros(us)).is_err() {
        println!("hub75-ring: steal timer would not start — core 0 packs alone");
        return;
    }
    t.listen();
    STEAL_TIMER.lock(|c| *c.borrow_mut() = Some(t));
    println!("hub75-ring: core-1 steal armed on {:?}, every {} us", Cpu::current(), us);
}

extern "C" fn steal_isr() {
    STEAL_TIMER.lock(|c| {
        if let Some(t) = c.borrow_mut().as_mut() {
            t.clear_interrupt();
        }
    });
    steal();
}

/// One turn of the steal (design §6): copy whatever the queue has that
/// core 0 has not reached. Cheap when there is nothing: one atomic load
/// and a position probe. Counters credit the core it actually ran on (the
/// AppCpu init closure runs on core 0 when the second core failed to
/// start, and the timer then lands there too).
pub fn steal() -> u32 {
    if !STEAL.load(Ordering::Relaxed) {
        return 0;
    }
    let Some(s) = shared() else { return 0 };
    // SAFETY: `CORE1` is the steal interrupt's alone — one priority level,
    // so it never re-enters.
    let p = unsafe { &mut *core::ptr::addr_of_mut!(CORE1) };
    let core = if Cpu::current() == Cpu::AppCpu { 1 } else { 0 };
    drain(p, s, core)
}

impl OutputDriver for Hub75Ring {
    type Error = &'static str;

    fn set_protocol(&mut self, _p: Protocol) -> Result<(), Self::Error> {
        Err("hub75 panel: wire format is fixed")
    }

    fn resize(&mut self, _pixels: usize) -> bool {
        self.hub75.is_some()
    }

    /// A new pass has been claimed since the last accepted frame — the
    /// pacing the swap's landing used to give the render task.
    fn ready_for_frame(&self) -> bool {
        let next = Claim::decode(NEXT_FILL.load(Ordering::Acquire)).abs;
        self.ring.pass(next) != self.last_frame_pass && self.free_frame().is_some()
    }

    fn paces_frames(&self) -> bool {
        self.hub75.is_some()
    }

    /// Publish: pack the wire frame into a free packed frame (once per
    /// frame, #892) and point `newest` at it. The next pass claimed copies
    /// from it.
    fn write_frame(&mut self, rgb: &[[u8; 3]], _brightness5: u8) -> bool {
        let Some(hub75) = self.hub75.as_ref() else { return false };
        // `frame_count` is RING WRAPS here (one EOF per ring), and a ring
        // is `n` of the frame's `rows` row pairs — so `rescan_hz` would read
        // 243 for a 76 Hz panel on a 10-slot ring (2026-09-28). Scale it to
        // passes so the field keeps meaning what docs/api.md says.
        let wraps = u64::from(hub75.frame_count());
        let passes = (wraps * u64::from(self.ring.n) / u64::from(self.ring.rows.max(1))) as u32;
        crate::shared::RESCANS.store(passes, Ordering::Relaxed);
        let Some(i) = self.free_frame() else { return false };
        if !self.pack_frame(usize::from(i), rgb) {
            return false;
        }
        NEWEST.store(i, Ordering::Release);
        self.last_frame_pass = self.ring.pass(Claim::decode(NEXT_FILL.load(Ordering::Acquire)).abs);
        true
    }

    /// The refill: every turn of the output task's loop.
    fn flush(&mut self) -> bool {
        self.adopt_blank();
        if !self.published {
            self.publish();
        }
        if let Some(s) = shared() {
            drain(&mut self.packer, s, 0);
        }
        true
    }
}

impl Hub75Ring {
    /// A latch-blanking change (Gitea #778): the template is stamped into
    /// every packed frame at pack time, so adopting it is one field and one
    /// schedule refit — the next frame carries it.
    fn adopt_blank(&mut self) {
        let want = WANT_BLANK.load(Ordering::Relaxed);
        if want == crate::hub75::BLANK_NONE || want == self.control.blank {
            return;
        }
        if 2 * u32::from(want) + u32::from(self.control.latch_clocks) >= self.g.cols as u32 {
            WANT_BLANK.store(self.control.blank, Ordering::Relaxed);
            return;
        }
        self.control.blank = want;
        self.sched = self.sched.refit(self.g, self.control);
        let lsb = self.sched.lsb;
        LIVE.lock(|c| {
            if let Some(mut l) = c.get() {
                l.blank = want;
                l.lsb = lsb;
                c.set(Some(l));
            }
        });
    }
}
