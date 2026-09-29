//! The HUB75 RING driver (Gitea #857; docs/hub75-ring-design.md; feature
//! `hub75-ring`): a fixed ring of row-pair slots in internal SRAM, packed
//! just ahead of the beam straight from the engine's RGB frame in PSRAM.
//!
//! What replaces what, against `hub75.rs`'s two-buffer driver:
//!
//! - **No packed frame anywhere.** `N` slots of `planes + 1` rows of `cols`
//!   words each (`luxel_hub75::ring`), sized from the `panel` line's
//!   `ring_ms` of slack — 9 slots and 9 KB for a 64x64 at stock, instead of
//!   two 29 KB framebuffers. The DMA chain is `esp_hub75::fill_ring_chain`'s
//!   row-major circular chain over them (`Hub75::new_ring`), `suc_eof` on
//!   the ring's last slot only, so the driver's `frame_count()` IS the ring
//!   wrap count and `dma_position()` the slot the beam is in.
//! - **The packer is the refill.** A slot the beam has left is claimed out
//!   of one queue word (`ring::claim_next`: the counter and, in its top bit,
//!   which RGB frame this pass reads), re-templated for its new row pair
//!   (`ring::format_slot_for`), and packed with `pie::pack_row_pair` — the
//!   vector packer, 22 cycles/px (#855). Claims are taken only while
//!   [`Ring::fillable`] and while the slots left before the beam cover the
//!   packer's own worst pack; a claim that cannot make it is SKIPPED and
//!   counted `late` — the slot then shows the row it already held, at that
//!   row's own address (stale, never a mixed-address row, design §7).
//! - **Core 0 packs, from the output task.** [`Hub75Ring::flush`] drains the
//!   queue on every turn of the output task's loop; the task polls at
//!   [`Hub75Ring::poll_interval`] (a fraction of the ring's slack) instead
//!   of waiting a frame. The core-1 steal (design §6) is the next PR: the
//!   queue is already atomics, and nothing here assumes one packer.
//! - **`write_frame` publishes.** The wire frame is copied into one of four
//!   RGB frames in the PSRAM arena (48 KB at 4096 px; the arena has
//!   megabytes) and its index becomes `newest`. The claim of a pass's row 0
//!   latches `newest` for the whole pass, so a pass never mixes frames; four
//!   buffers because two passes can be in flight (one being emitted, one
//!   being packed) on two different frames while `newest` waits for the next
//!   pass and a fourth is being written — one is always free.
//!   `ready_for_frame` is true once a new pass has been claimed since the
//!   last accepted frame, which paces the render task to the pass rate the
//!   way the two-buffer driver's swap did.
//! - **Frame-atomic for free**, no swap, no `Hub75Swap`, no pass audit; the
//!   `pass` block's swap counters read 0 on this driver.
//!
//! Counters (`/api/status` `pass.ring`): `rows` (slots), `slack_us`,
//! `late` (claims skipped), `blanked` and `backoffs` (#852, 0 here),
//! `packed_core0`, `packed_core1` (0 until the steal).

use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use alloc::vec::Vec;
use esp_hal::dma::DmaDescriptor;
use esp_hal::peripherals::{DMA_CH0, LCD_CAM};
use esp_hal::Blocking;
use esp_hub75::{Hub75, Hub75Pins16};
use esp_println::println;
use luxel_core::arena::FrameVec;
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

/// The RGB frames the packer reads from: two passes in flight, the newest
/// published, and one free to write.
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

/// The queue word (`ring::Claim::encode`): the next claim's counter, and
/// the frame index of the pass being claimed in its top bit. CAS-claimed.
static NEXT_FILL: AtomicU32 = AtomicU32::new(0);
/// The RGB frame most recently published by `write_frame`.
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
    /// Slot `k` starts at `slots[k * slot_words]`; 16-byte aligned.
    slots: &'static mut [u16],
    slot_words: usize,
    /// Slot rows the DMA reads per slot, in order (`ring::slot_emissions`).
    order: Vec<u8>,
    frames: [FrameVec; FRAMES],
    tables: Option<&'static mut Tables>,
    tables_b5: u8,
    pads: PairPads,
    remap: Option<&'static [u16]>,
    /// Pass of the last frame `write_frame` accepted; `ready_for_frame`
    /// waits for the queue to move past it.
    last_frame_pass: u32,
    /// Typical pack of one row pair in cycles (EWMA) — the skip rule.
    pack_cycles: u32,
    /// Clocks per slot at the panel's pixel clock, for the skip rule.
    slot_clocks: u32,
    clock_hz: u32,
    /// The queue has been brought level with the beam once (the first
    /// drain after boot); later catch-ups are stalls and count `late`.
    synced: bool,
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
            slots: &mut [],
            slot_words: 0,
            order: Vec::new(),
            frames: [
                luxel_core::arena::empty(),
                luxel_core::arena::empty(),
                luxel_core::arena::empty(),
                luxel_core::arena::empty(),
            ],
            tables: None,
            tables_b5: u8::MAX,
            pads: PairPads::new(0),
            remap: None,
            last_frame_pass: u32::MAX,
            pack_cycles: 0,
            slot_clocks: 1,
            clock_hz: 1,
            synced: false,
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
        let pads = PairPads::for_geometry(g);
        // SAFETY: the block is `n * slot_words` zeroed, 16-byte-aligned u16s
        // we own for the life of the driver (leaked below).
        let slots: &'static mut [u16] =
            unsafe { core::slice::from_raw_parts_mut(slots_block.ptr.cast::<u16>(), n as usize * slot_words) };
        // Pre-fill: slot k carries row pair k (dark) with its template.
        for (k, slot) in slots.chunks_exact_mut(slot_words).enumerate() {
            ring::format_slot_for(slot, g, c, &s, k % g.rows);
        }
        let slot_bases: Vec<*const u8> =
            (0..n as usize).map(|k| slots[k * slot_words..].as_ptr().cast::<u8>()).collect();
        // SAFETY: `descs` EMPTY-initialised descriptors in a block we own.
        let descriptors: &'static mut [DmaDescriptor] = unsafe {
            let p = desc_block.ptr.cast::<DmaDescriptor>();
            for i in 0..descs {
                p.add(i).write(DmaDescriptor::EMPTY);
            }
            core::slice::from_raw_parts_mut(p, descs)
        };

        // ---- the RGB frames, in the arena
        let frames = [
            luxel_core::arena::frame(g.pixels()),
            luxel_core::arena::frame(g.pixels()),
            luxel_core::arena::frame(g.pixels()),
            luxel_core::arena::frame(g.pixels()),
        ];
        if frames.iter().any(|f| f.len() != g.pixels()) {
            return Err("RGB frame alloc failed");
        }

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
                    frames,
                    tables: Some(tables),
                    tables_b5: u8::MAX,
                    pads,
                    remap,
                    last_frame_pass: u32::MAX,
                    pack_cycles: 0,
                    slot_clocks: ring::slot_clocks(&s, g.cols) as u32,
                    clock_hz,
                    synced: false,
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

    /// Where the beam is, as the ring's absolute counter: ring wraps (the
    /// EOF count — one EOF per ring, on its last slot) times `n` plus the
    /// slot of the descriptor being read. A raised-but-unserviced EOF is a
    /// wrap the count has not seen yet.
    fn abs_dma(&self) -> Option<u32> {
        let hub75 = self.hub75.as_ref()?;
        let (_ring, idx, eof_pending) = hub75.dma_position()?;
        let wraps = hub75.frame_count().wrapping_add(u32::from(eof_pending));
        let slot = (idx / self.order.len()) as u32;
        Some(self.ring.abs(wraps, slot))
    }

    /// Pack one claimed row pair into its slot.
    fn pack_claim(&mut self, claim: Claim) {
        let slot = self.ring.slot(claim.abs) as usize;
        let row = self.ring.row(claim.abs) as usize;
        let Some(t) = self.tables.as_deref_mut() else { return };
        let b5 = crate::out_brightness();
        if self.tables_b5 != b5 {
            t.build_scale5(b5);
            self.tables_b5 = b5;
        }
        let words = &mut self.slots[slot * self.slot_words..(slot + 1) * self.slot_words];
        ring::format_slot_for(words, self.g, self.control, &self.sched, row);
        let frame = &self.frames[usize::from(claim.buf) % FRAMES];
        // The packer takes the PLANE rows only — `planes * cols` words from
        // `plane_row(0)` — never the ENTRY row in front of them (its length
        // assert reset the board three times on 2026-09-28 and the guard
        // rolled the slot back). Still 16-aligned: `cols` is a multiple of
        // the lane count, so the ENTRY row is a multiple of 16 bytes.
        let planes = &mut words[ring::plane_row(0) * self.g.cols..];
        // The kernel refuses only a shape the boot already refused.
        let _ = pie::pack_row_pair(planes, self.g, row, frame, self.remap, t, &mut self.pads);
    }

    /// Drain the queue: claim and pack every slot the beam has left that
    /// can still be packed in time. Returns how many were packed.
    fn drain(&mut self) -> u32 {
        let Some(abs_dma) = self.abs_dma() else { return 0 };
        let mut packed = 0u32;
        loop {
            let word = NEXT_FILL.load(Ordering::Acquire);
            let (claim, next) = ring::claim_next(word, NEWEST.load(Ordering::Acquire), &self.ring);
            if self.ring.late(claim.abs, abs_dma) {
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
                let target = self.ring.catch_up(abs_dma);
                let word2 = Claim { buf, abs: target }.encode();
                if NEXT_FILL.compare_exchange(word, word2, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                    PASS_FRAME[(self.ring.pass(target) % 2) as usize].store(buf, Ordering::Release);
                    if self.synced {
                        RING_LATE.fetch_add(self.ring.dist(claim.abs, target), Ordering::Relaxed);
                    }
                    self.synced = true;
                }
                continue;
            }
            if !self.ring.fillable(claim.abs, abs_dma) {
                if packed == 0 {
                    RING_IDLE.fetch_add(1, Ordering::Relaxed);
                }
                break;
            }
            // The skip rule (design §7): slots left before the beam, less
            // the one it may be in, must cover the worst pack plus margin.
            let left_clocks = self.ring.until_late(claim.abs, abs_dma).saturating_sub(1) * self.slot_clocks;
            let left_cycles = (u64::from(left_clocks) * 240_000_000 / u64::from(self.clock_hz.max(1))) as u32;
            let need = self.pack_cycles + (self.pack_cycles >> PACK_MARGIN_SHIFT);
            if self.pack_cycles != 0 && left_cycles < need {
                if NEXT_FILL.compare_exchange(word, next, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                    RING_LATE.fetch_add(1, Ordering::Relaxed);
                    if self.ring.row(claim.abs) == 0 {
                        PASS_FRAME[(self.ring.pass(claim.abs) % 2) as usize].store(claim.buf, Ordering::Release);
                    }
                }
                continue;
            }
            if NEXT_FILL.compare_exchange(word, next, Ordering::AcqRel, Ordering::Relaxed).is_err() {
                continue; // another packer took it (the core-1 steal)
            }
            if self.ring.row(claim.abs) == 0 {
                PASS_FRAME[(self.ring.pass(claim.abs) % 2) as usize].store(claim.buf, Ordering::Release);
            }
            let t0 = esp_hal::xtensa_lx::timer::get_cycle_count();
            self.pack_claim(claim);
            let dt = esp_hal::xtensa_lx::timer::get_cycle_count().wrapping_sub(t0);
            self.pack_cycles = if self.pack_cycles == 0 { dt } else { (self.pack_cycles * 7 + dt) / 8 };
            RING_PACK_US.store(self.pack_cycles / 240, Ordering::Relaxed);
            RING_PACK_US_MAX.fetch_max(dt / 240, Ordering::Relaxed);
            RING_PACKED_CORE0.fetch_add(1, Ordering::Relaxed);
            packed += 1;
        }
        packed
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

    /// Publish: copy the wire frame into a free RGB frame and point
    /// `newest` at it. The next pass claimed reads it.
    fn write_frame(&mut self, rgb: &[[u8; 3]], _brightness5: u8) -> bool {
        let Some(hub75) = self.hub75.as_ref() else { return false };
        crate::shared::RESCANS.store(hub75.frame_count(), Ordering::Relaxed);
        let Some(i) = self.free_frame() else { return false };
        let dst = &mut self.frames[usize::from(i)];
        let n = rgb.len().min(dst.len());
        dst[..n].copy_from_slice(&rgb[..n]);
        for px in &mut dst[n..] {
            *px = [0; 3];
        }
        NEWEST.store(i, Ordering::Release);
        self.last_frame_pass = self.ring.pass(Claim::decode(NEXT_FILL.load(Ordering::Acquire)).abs);
        true
    }

    /// The refill: every turn of the output task's loop.
    fn flush(&mut self) -> bool {
        self.adopt_blank();
        self.drain();
        true
    }
}

impl Hub75Ring {
    /// A latch-blanking change (Gitea #778): the template is rewritten at
    /// every claim, so adopting it is one field and one schedule refit.
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
