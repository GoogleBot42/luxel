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
//! - **The copy is the refill, and the GDMA does it** (#892 step 2). A slot
//!   the beam has left is claimed out of one queue word (`ring::claim_next`:
//!   the counter and, in its top bit, which packed frame this pass reads).
//!   A turn of a copier claims up to [`BATCH_SLOTS`] such slots and hands
//!   the batch to GDMA channel 1 in memory-to-memory mode ([`Engine`]): one
//!   descriptor chain over the row pairs in the packed frame (PSRAM, read in
//!   64-byte bursts straight off the bus — no cache-line fills, which is what
//!   made the CPU copy 15 MB/s beside a 16384-px engine), one over the slots
//!   (internal SRAM). The CPU's part of a pass is a few microseconds of
//!   descriptor writes; the copy runs while the CPU does something else, and
//!   a completion interrupt (bound on the AppCpu with the steal) accounts
//!   the batch and starts the next — whatever the `steal` lever says, which
//!   is the core-1 TIMER's turns only. The packed frame is written back from
//!   the data cache after every eight row pairs of `pack_frame`, so the DMA
//!   reads what the packer wrote. Claims are taken only while
//!   [`Ring::fillable`] and while the slots left before the beam cover the
//!   batch's own measured copy time; a claim that cannot make it is SKIPPED
//!   and counted `late` — the slot then shows the row it already held, at
//!   that row's own address (stale, never a mixed-address row, design §7).
//!   The CPU `memcpy` of step 1 stays as the fallback when the DMA engine
//!   could not be built, and as the A/B lever (`POST /api/ring
//!   {"dma":false}`).
//! - **The ring is capped by the heap** ([`RING_HEAP_RESERVE`]): `ring_ms`
//!   asks for a slack, the boot gives the slots that slack needs OR as many
//!   as leave the boot floor plus a reserve for the engine, whichever is
//!   fewer. A 4x1 at `ring_ms 3` asked for 22 slots of 4 KB and left 15 KB
//!   of heap (2026-09-29); `pass.ring.asked` beside `rows` says when the
//!   cap bit.
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
//! Counters (`/api/status` `pass.ring`): `rows` (slots) and `asked` (what
//! `ring_ms` wanted before the heap cap), `slack_us`, `late` (claims
//! skipped), `blanked` and `backoffs` (#852, 0 here), `packed_core0`,
//! `packed_core1` (row pairs copied on behalf of each core — the core that
//! started the batch), `pack_us` / `pack1_us` (the CPU fallback copy per
//! core), `pack_us_max`, `frame_pack_us` (the once-per-frame pack, write-back
//! included), `dma` (the lever), `dma_us` (the DMA's time per slot, EWMA),
//! `dma_us_max`, `dma_cal_us` (the boot calibration, an idle bus),
//! `dma_batches`, `dma_isr` (batches the completion interrupt started),
//! `dma_errors`, `steal`.

use core::cell::{RefCell, UnsafeCell};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use alloc::vec::Vec;
use esp_hal::dma::{Channel, DmaDescriptor, Owner};
use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::peripherals::{DMA, DMA_CH0, DMA_CH1, LCD_CAM, SPI3, TIMG1};
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

/// Row pairs one turn of a copier hands to the DMA at most. The descriptor
/// pools are sized for this many slots per side, and a batch's own copy
/// time (its length × `dma_us`) is what the skip rule checks each claim
/// against. Eight is a quarter of a 32-row frame: 8–32 KB per batch, 100–
/// 500 µs at the bus rates seen, under any poll interval the ring runs at.
const BATCH_SLOTS: usize = 8;

/// The GDMA channel the copy runs on. Channel 0 is the panel's own LCD_CAM
/// chain (`esp_hub75`); every AHB GDMA channel on the S3 can do
/// memory-to-memory and reach PSRAM.
const COPY_CH: usize = 1;

/// The peripheral select the copy channel is parked on. Memory-to-memory
/// mode (`mem_trans_en`) takes no peripheral handshake, but the select
/// must name a port nothing else drives DMA through — SPI3, which a panel
/// board never wires (`Hub75Ring::new` takes the handle to keep it so;
/// esp-hal's own `Mem2Mem` does the same with SPI2).
const COPY_PERI: u8 = 1;

/// The external-memory burst the copy reads PSRAM with: 64 bytes, the
/// largest the S3's GDMA offers (`out_ext_mem_bk_size` 2). Source rows are
/// 16-aligned and a multiple of 16 bytes; a transmit descriptor on the S3
/// needs no alignment at all, so the burst is a bandwidth setting only.
const EXT_BURST_64: u8 = 2;

/// Internal heap kept free BEYOND `BOOT_HEAP_FLOOR` when the ring is sized
/// from `ring_ms` (`capped_slots`). The floor (100 KB) is what WiFi, the
/// net stack and the web pool need to come up at all; the reserve is for
/// the engine after them. Measured on the 4x1 (16384 px, 2026-09-29): a
/// 22-slot ring passed the floor and left **15 KB** at runtime — no
/// pattern loads under the 20 KB `RUNTIME_FLOOR` — and a 9-slot ring left
/// 62–73 KB with Aurora rendering; the runtime residue is what the panel
/// left minus ~105 KB at that pixel count. 48 KB of reserve puts a capped
/// ring at ~50 KB of runtime heap, where the 1x1 boards run today. The ask
/// still wins when it is smaller; `pass.ring.asked` beside `rows` shows a
/// cap that bit.
pub(crate) const RING_HEAP_RESERVE: usize = 48 * 1024;

/// A batch still in flight this long after it started is presumed wedged
/// — a flash write suspends the cache the DMA reads PSRAM through (#852),
/// or an error the interrupt did not see. The channel is reset, the
/// batch's claims count as `late` (their slots keep the old rows at the
/// right addresses), and the next turn starts over.
const DMA_STALL_US: u64 = 20_000;

/// Latency the skip rule adds on top of a batch's copy time, microseconds:
/// the turn that starts a batch runs behind the beam by whatever the poll
/// cost, and the interrupt that chains the next batch by its own latency
/// (`isr_lat_max` reads ~200 µs worst on this board, ~10 typical).
const DMA_MARGIN_US: u32 = 40;

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
/// Slots `ring_ms` asked for before the heap cap (`rows` is what it got).
pub static RING_ASKED: AtomicU32 = AtomicU32::new(0);
/// The second lever (#892 step 2): copy by GDMA, or by the CPU `memcpy` of
/// step 1. Runtime, not persisted — `POST /api/ring {"dma":false}` for an
/// A/B on one boot. Forced off when the engine could not be built.
pub static DMA_ON: AtomicBool = AtomicBool::new(true);
/// The `hybrid` lever (`POST /api/ring {"hybrid":…}`, default off): while a
/// DMA batch is in flight, core 0's turn (the output task, which has
/// nothing else to do) copies further claims itself instead of returning —
/// the bus rate of the DMA plus whatever the cache serves the CPU. Core 1
/// stays DMA-only (its interrupts must stay short).
pub static HYBRID: AtomicBool = AtomicBool::new(false);
/// The DMA's time per slot, microseconds (clipped EWMA — the skip rule's
/// number), and the raw worst.
pub static RING_DMA_US: AtomicU32 = AtomicU32::new(0);
pub static RING_DMA_US_MAX: AtomicU32 = AtomicU32::new(0);
/// The boot calibration copy's time per slot, microseconds — the DMA on an
/// idle bus, before WiFi or an engine (0 = the calibration failed).
pub static RING_DMA_CAL_US: AtomicU32 = AtomicU32::new(0);
/// Batches completed; batches the completion interrupt started (the rest
/// were started by a copier's turn); batches that ended in a descriptor
/// error or a stall reset.
pub static RING_DMA_BATCHES: AtomicU32 = AtomicU32::new(0);
pub static RING_DMA_ISR: AtomicU32 = AtomicU32::new(0);
pub static RING_DMA_ERRORS: AtomicU32 = AtomicU32::new(0);
/// A copy batch is in flight (mirrors `Engine::busy` without the lock):
/// what the flash fence reads before a flash op (`output::transfer_busy`).
static COPY_BUSY: AtomicBool = AtomicBool::new(false);
/// The steal's timer, kept so its handler can clear the interrupt.
static STEAL_TIMER: BlockingMutex<CriticalSectionRawMutex, RefCell<Option<PeriodicTimer<'static, Blocking>>>> =
    BlockingMutex::new(RefCell::new(None));
/// The copy channel's esp-hal handle: the peripheral guard and the
/// interrupt binding. Every register the engine programs is reached
/// through `DMA::regs()` directly (`Channel` keeps its transfer API
/// crate-private), so this is held for what it owns, not used.
static COPY_CHANNEL: BlockingMutex<CriticalSectionRawMutex, RefCell<Option<Channel<Blocking, DMA_CH1<'static>>>>> =
    BlockingMutex::new(RefCell::new(None));
/// The DMA copy engine — one, shared by both copiers under the critical
/// section (the batch build, the start and the completion accounting are
/// each a few microseconds; the two cores contend at that scale only).
static ENGINE: BlockingMutex<CriticalSectionRawMutex, RefCell<Option<Engine>>> = BlockingMutex::new(RefCell::new(None));

/// The memory-to-memory copy engine on GDMA channel [`COPY_CH`].
///
/// One batch in flight at a time: `slots` claims whose row pairs are being
/// copied, started at `t0` by `core`. The two descriptor pools (internal
/// SRAM — the DMA reads its descriptors from internal memory only) hold
/// [`BATCH_SLOTS`] slots' worth each; a slot is `per_slot` descriptors of
/// `chunk_rows` slot rows (a descriptor carries at most 4095 bytes, a slot
/// is 1–8 KB).
struct Engine {
    busy: bool,
    slots: u32,
    t0: u64,
    core: usize,
    /// Started by the completion interrupt (credited to `dma_isr`).
    by_isr: bool,
    /// The DMA's time per slot, microseconds — clipped EWMA (a sample can
    /// at most double it), seeded by the boot calibration.
    slot_us: u32,
    tx: *mut DmaDescriptor,
    rx: *mut DmaDescriptor,
    per_slot: usize,
    chunk_rows: usize,
    row_bytes: usize,
    slot_rows: usize,
}

// SAFETY: the raw descriptor pointers are used only under `ENGINE`'s
// critical section, and the pools are leaked internal SRAM.
unsafe impl Send for Engine {}

/// The copy channel's register block.
macro_rules! copy_ch {
    () => {
        DMA::regs().ch(COPY_CH)
    };
}

impl Engine {
    /// Program the channel for memory-to-memory copies from PSRAM into
    /// internal SRAM. Once, at boot; a batch start only resets the FSMs.
    fn configure() {
        let ch = copy_ch!();
        Self::reset_fsm();
        // SAFETY (the `bits` writes): field-width values from the TRM —
        // burst size 2 = 64 B, peripheral select `COPY_PERI`.
        ch.in_conf0().modify(|_, w| w.mem_trans_en().set_bit().indscr_burst_en().set_bit().in_data_burst_en().set_bit());
        ch.in_conf1().modify(|_, w| unsafe { w.in_ext_mem_bk_size().bits(EXT_BURST_64) }.in_check_owner().clear_bit());
        ch.in_peri_sel().modify(|_, w| unsafe { w.peri_in_sel().bits(COPY_PERI) });
        ch.out_conf0().modify(|_, w| {
            w.outdscr_burst_en().set_bit().out_data_burst_en().set_bit().out_auto_wrback().set_bit().out_eof_mode().set_bit()
        });
        ch.out_conf1().modify(|_, w| unsafe { w.out_ext_mem_bk_size().bits(EXT_BURST_64) }.out_check_owner().clear_bit());
        ch.out_peri_sel().modify(|_, w| unsafe { w.peri_out_sel().bits(COPY_PERI) });
    }

    /// Reset both halves' state machines and FIFOs (configuration bits
    /// stay), and clear every raw interrupt.
    fn reset_fsm() {
        let ch = copy_ch!();
        ch.in_conf0().modify(|_, w| w.in_rst().set_bit());
        ch.in_conf0().modify(|_, w| w.in_rst().clear_bit());
        ch.out_conf0().modify(|_, w| w.out_rst().set_bit());
        ch.out_conf0().modify(|_, w| w.out_rst().clear_bit());
        Self::clear_ints();
    }

    fn clear_ints() {
        let ch = copy_ch!();
        // SAFETY: all-ones into write-1-to-clear registers.
        ch.in_int().clr().write(|w| unsafe { w.bits(0xffff_ffff) });
        ch.out_int().clr().write(|w| unsafe { w.bits(0xffff_ffff) });
    }

    /// Enable the completion and error interrupts on the IN half (the
    /// handler is bound by [`arm_steal`] on the core that runs it).
    fn listen() {
        let ch = copy_ch!();
        ch.in_int().ena().modify(|_, w| {
            w.in_suc_eof().set_bit().in_dscr_err().set_bit().in_dscr_empty().set_bit().in_err_eof().set_bit()
        });
        ch.out_int().ena().modify(|_, w| w.out_dscr_err().set_bit());
    }

    /// `(done, failed)` for the batch in flight, from the raw interrupt
    /// bits: done = the IN half received the chain's EOF; failed = a
    /// descriptor error or an empty IN chain on either half.
    fn poll() -> (bool, bool) {
        let ch = copy_ch!();
        let i = ch.in_int().raw().read();
        let o = ch.out_int().raw().read();
        let failed =
            i.in_dscr_err().bit_is_set() || i.in_dscr_empty().bit_is_set() || i.in_err_eof().bit_is_set() || o.out_dscr_err().bit_is_set();
        (i.in_suc_eof().bit_is_set(), failed)
    }

    /// Start a batch: RX chain first, then TX (the order the TRM asks for
    /// on every chip with a memory-to-memory mode).
    fn start(rx_head: *const DmaDescriptor, tx_head: *const DmaDescriptor) {
        let ch = copy_ch!();
        Self::reset_fsm();
        // SAFETY: the low 20 bits of an internal-SRAM descriptor address,
        // the field's width; the DMA supplies the rest.
        ch.in_link().modify(|_, w| unsafe { w.inlink_addr().bits((rx_head as u32) & 0xf_ffff) });
        ch.out_link().modify(|_, w| unsafe { w.outlink_addr().bits((tx_head as u32) & 0xf_ffff) });
        ch.in_link().modify(|_, w| w.inlink_start().set_bit());
        ch.out_link().modify(|_, w| w.outlink_start().set_bit());
    }

    /// Fill slot `i`'s descriptors in both pools for one claim: `src` is
    /// the row pair in the packed frame (PSRAM), `dst` its slot (internal).
    /// Every descriptor links to the next; [`Engine::close`] cuts the
    /// chains after the batch's last slot.
    ///
    /// # Safety
    /// `i < BATCH_SLOTS`; `src`/`dst` point at `slot_rows * row_bytes`
    /// readable/writable bytes that nothing else touches until the batch
    /// completes.
    unsafe fn fill(&mut self, i: usize, src: *const u8, dst: *mut u8) {
        let chunk_bytes = self.chunk_rows * self.row_bytes;
        let slot_bytes = self.slot_rows * self.row_bytes;
        for c in 0..self.per_slot {
            let off = c * chunk_bytes;
            let bytes = chunk_bytes.min(slot_bytes - off);
            let k = i * self.per_slot + c;
            // SAFETY: `k` is within the pools (`i < BATCH_SLOTS`), and the
            // buffer offsets are within the caller's row pair / slot.
            unsafe {
                let mut d = DmaDescriptor::EMPTY;
                d.set_size(bytes);
                d.set_length(bytes);
                d.set_owner(Owner::Dma);
                d.set_suc_eof(false);
                d.buffer = src.add(off).cast_mut();
                d.next = self.tx.add(k + 1);
                self.tx.add(k).write_volatile(d);
                let mut d = DmaDescriptor::EMPTY;
                d.set_size(bytes);
                d.set_length(0);
                d.set_owner(Owner::Dma);
                d.buffer = dst.add(off);
                d.next = self.rx.add(k + 1);
                self.rx.add(k).write_volatile(d);
            }
        }
    }

    /// Close the chains after `slots` filled slots: the last TX descriptor
    /// carries the EOF (what the IN half's `in_suc_eof` reports) and both
    /// tails point nowhere.
    fn close(&mut self, slots: usize) {
        debug_assert!(slots >= 1 && slots <= BATCH_SLOTS);
        let k = slots * self.per_slot - 1;
        // SAFETY: `k` is within the pools; the descriptors were written by
        // `fill` and are not yet handed to the DMA.
        unsafe {
            let mut d = self.tx.add(k).read_volatile();
            d.set_suc_eof(true);
            d.next = core::ptr::null_mut();
            self.tx.add(k).write_volatile(d);
            let mut d = self.rx.add(k).read_volatile();
            d.next = core::ptr::null_mut();
            self.rx.add(k).write_volatile(d);
        }
    }

    /// Account for the batch in flight, if any: `Busy` (still copying),
    /// `Idle` (nothing in flight, or it just completed and was credited),
    /// `Failed` (reset; its claims counted late). Called under the engine's
    /// critical section by every turn and by the completion interrupt.
    fn settle(&mut self, now_us: u64, slack_us: u32) -> Settled {
        if !self.busy {
            return Settled::Idle;
        }
        let (done, failed) = Self::poll();
        let age = now_us.saturating_sub(self.t0);
        if failed || (!done && age > DMA_STALL_US) {
            Self::reset_fsm();
            self.busy = false;
            COPY_BUSY.store(false, Ordering::Release);
            RING_DMA_ERRORS.fetch_add(1, Ordering::Relaxed);
            RING_LATE.fetch_add(self.slots, Ordering::Relaxed);
            return Settled::Failed;
        }
        if !done {
            return Settled::Busy;
        }
        Self::clear_ints();
        self.busy = false;
        COPY_BUSY.store(false, Ordering::Release);
        let per_slot = (age / u64::from(self.slots.max(1))) as u32;
        // Clipped like the CPU copy's estimate: at most double per sample,
        // the first bounded by the slack (the poll that found it done ran
        // behind the completion by up to the poll interval, so the number
        // is an upper bound until the interrupt reports one).
        let clipped = if self.slot_us == 0 { per_slot.min(slack_us.max(1)) } else { per_slot.min(self.slot_us * 2) };
        self.slot_us = if self.slot_us == 0 { clipped.max(1) } else { ((self.slot_us * 7 + clipped) / 8).max(1) };
        RING_DMA_US.store(self.slot_us, Ordering::Relaxed);
        RING_DMA_US_MAX.fetch_max(per_slot, Ordering::Relaxed);
        RING_DMA_BATCHES.fetch_add(1, Ordering::Relaxed);
        if self.by_isr {
            RING_DMA_ISR.fetch_add(1, Ordering::Relaxed);
        }
        let ctr = if self.core == 0 { &RING_PACKED_CORE0 } else { &RING_PACKED_CORE1 };
        ctr.fetch_add(self.slots, Ordering::Relaxed);
        Settled::Idle
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Settled {
    Idle,
    Busy,
    Failed,
}

/// Microseconds since boot, the systimer — one clock for both cores (the
/// cycle counters are per core and start at different times).
fn now_us() -> u64 {
    esp_hal::time::Instant::now().duration_since_epoch().as_micros()
}

/// Write the data cache back for `len` bytes at `addr` (PSRAM): what the
/// packer wrote through the cache becomes what the DMA reads off the bus.
/// ROM routine (esp32s3.rom.ld), the one esp-hal's own DMA buffers and the
/// JIT's code publish use.
fn cache_writeback(addr: *const u8, len: usize) {
    unsafe extern "C" {
        fn rom_Cache_WriteBack_Addr(addr: u32, items: u32);
    }
    // SAFETY: a ROM routine over an address range this driver owns.
    unsafe { rom_Cache_WriteBack_Addr(addr as u32, len as u32) };
}

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
    ///
    /// `copy` is the GDMA channel the refill copies on (#892 step 2) and
    /// `_spi3` the peripheral select it is parked on — taken so nothing
    /// else can drive DMA through that port (see [`COPY_PERI`]).
    pub fn new(
        lcd_cam: LCD_CAM<'static>,
        pins: Hub75Pins16<'static>,
        channel: DMA_CH0<'static>,
        copy: DMA_CH1<'static>,
        _spi3: SPI3<'static>,
    ) -> Self {
        let m = crate::layout::matrix();
        let d = crate::layout::driver();
        match Self::try_boot(lcd_cam, pins, channel, copy, m, d, false) {
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
                let (lcd_cam, channel, copy, pins) =
                    unsafe { (LCD_CAM::steal(), DMA_CH0::steal(), DMA_CH1::steal(), crate::board::hub75_pins_stolen()) };
                match Self::try_boot(lcd_cam, pins, channel, copy, dm, dd, true) {
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
        copy: DMA_CH1<'static>,
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
        let slot_words = ring::slot_words(g.planes, g.cols);
        let order: Vec<u8> = ring::slot_emissions(&s).map(|r| r as u8).collect();
        let row_bytes = g.cols * 2;
        // The ring: what `ring_ms` asks for, capped by the heap (see
        // `RING_HEAP_RESERVE`).
        let asked = ring::slots_for_slack(u32::from(d.ring_ms) * 1000, &s, g.cols, clock_hz, g.rows);
        let Some(n) = capped_slots(asked, &s, g) else {
            return Err("the heap cannot hold even the smallest ring beside the boot floor and the engine's reserve");
        };
        if n < asked {
            println!(
                "hub75-ring: ring_ms {} asked for {} slots; the heap allows {} (floor {} B + reserve {} B)",
                d.ring_ms, asked, n, BOOT_HEAP_FLOOR, RING_HEAP_RESERVE
            );
        }
        let ring_ = Ring::new(n, g.rows as u32);

        // ---- memory: slots (internal, 16-aligned), descriptors, tables, pads
        let slots_layout = core::alloc::Layout::from_size_align(n as usize * slot_words * 2, 16)
            .map_err(|_| "bad slot layout")?;
        let slots_block = Block::zeroed(slots_layout).ok_or("ring slot alloc failed")?;
        let descs = esp_hub75::ring_descriptor_count(n as usize, order.len(), row_bytes);
        let desc_layout = core::alloc::Layout::array::<DmaDescriptor>(descs).map_err(|_| "bad descriptor layout")?;
        let desc_block = Block::zeroed(desc_layout).ok_or("DMA descriptor alloc failed")?;
        let (tables_block, tables) = alloc_tables().ok_or("packer table alloc failed")?;
        // The copy engine's descriptor pools: `BATCH_SLOTS` slots per side,
        // a slot in as many ≤ 4095-byte chunks of whole slot rows as it
        // takes (1 at 64 and 128 columns, 2 at 256, 3 at 512).
        let slot_rows = ring::slot_rows(g.planes);
        let chunk_rows = slot_rows.min(4095 / row_bytes.max(1)).max(1);
        let per_slot = slot_rows.div_ceil(chunk_rows);
        let pool = BATCH_SLOTS * per_slot;
        let copy_layout = core::alloc::Layout::array::<DmaDescriptor>(2 * pool).map_err(|_| "bad copy descriptor layout")?;
        let copy_block = Block::zeroed(copy_layout).ok_or("copy descriptor alloc failed")?;
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

        // ---- the copy engine (#892 step 2): the channel handle for its
        // guard and interrupt binding, the registers programmed directly,
        // and one synchronous calibration batch — row pairs 0.. of packed
        // frame 0 into slots 0.. (identical dark content, and the LCD DMA
        // is not running yet) — that proves the copy works and seeds the
        // skip rule's per-slot time. If it does not, the CPU copy of step
        // 1 stays in charge and says so.
        let copy_channel = Channel::new(copy);
        let mut engine = Engine {
            busy: false,
            slots: 0,
            t0: 0,
            core: 0,
            by_isr: false,
            slot_us: 0,
            tx: copy_block.ptr.cast::<DmaDescriptor>(),
            // SAFETY: the block holds `2 * pool` descriptors.
            rx: unsafe { copy_block.ptr.cast::<DmaDescriptor>().add(pool) },
            per_slot,
            chunk_rows,
            row_bytes,
            slot_rows,
        };
        Engine::configure();
        let cal = (n as usize).min(BATCH_SLOTS);
        for i in 0..cal {
            // SAFETY: row pair `i` of frame 0 and slot `i` are in bounds
            // (`cal ≤ n ≤ rows`), and nothing reads or writes either yet.
            unsafe {
                engine.fill(i, packed_ptr[0].add(i * slot_words).cast::<u8>().cast_const(), slots.add(i * slot_words).cast::<u8>());
            }
        }
        engine.close(cal);
        let t0 = now_us();
        Engine::start(engine.rx, engine.tx);
        let mut dma_ok = false;
        loop {
            let (done, failed) = Engine::poll();
            if failed {
                break;
            }
            if done {
                dma_ok = true;
                break;
            }
            if now_us().saturating_sub(t0) > 5_000 {
                break;
            }
        }
        let cal_us = now_us().saturating_sub(t0);
        Engine::reset_fsm();
        if dma_ok {
            engine.slot_us = ((cal_us / cal as u64) as u32).max(1);
            RING_DMA_CAL_US.store(engine.slot_us, Ordering::Relaxed);
            DMA_ON.store(true, Ordering::Relaxed);
            println!(
                "hub75-ring: GDMA ch{} copies the refill — {} slots of {} B in {} us ({} us a slot, {} MB/s), {} descriptors a slot, 64 B bursts",
                COPY_CH,
                cal,
                slot_words * 2,
                cal_us,
                engine.slot_us,
                (cal as u64 * (slot_words * 2) as u64) / cal_us.max(1),
                per_slot,
            );
        } else {
            DMA_ON.store(false, Ordering::Relaxed);
            println!(
                "hub75-ring: GDMA ch{} calibration copy {} after {} us — the CPU copies the refill",
                COPY_CH,
                if cal_us > 5_000 { "timed out" } else { "failed" },
                cal_us
            );
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
                RING_ASKED.store(asked, Ordering::Relaxed);
                RING_SLACK_US.store(slack_us, Ordering::Relaxed);
                NEXT_FILL.store(Claim { buf: 0, abs: ring_.first_claim() }.encode(), Ordering::Relaxed);
                let _ = slots_block.leak();
                let _ = desc_block.leak();
                let _ = tables_block.leak();
                let _ = copy_block.leak();
                if dma_ok {
                    ENGINE.lock(|c| *c.borrow_mut() = Some(engine));
                }
                COPY_CHANNEL.lock(|c| *c.borrow_mut() = Some(copy_channel));
                // The panel chain wins the GDMA arbitration (see `set_priority`).
                set_priority(true);
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

}

/// Internal heap the ring's fixed parts take beside the slots: the packer
/// tables (2 KB), the vector pads, the copy engine's descriptor pools and
/// the `Vec`s — rounded up.
const RING_FIXED_BYTES: usize = 8 * 1024;

/// The slots a ring may have: `asked` (what `ring_ms` resolves to) or as
/// many as the heap allows, whichever is fewer — `None` when not even
/// `GUARD_SLOTS + 1` fit. The allowance is the heap before the panel
/// allocated (`hub75::heap_before_panel`) less `BOOT_HEAP_FLOOR`, less
/// [`RING_HEAP_RESERVE`], less the fixed parts, divided by what one slot
/// costs with its share of the DMA chain. No reading (a board that never
/// booted a panel) means no cap. Shared with `hub75::boot_cost` so `POST
/// /api/layout` predicts the ring the boot will build.
pub(crate) fn capped_slots(asked: u32, s: &Schedule, g: Geometry) -> Option<u32> {
    let heap0 = crate::hub75::heap_before_panel();
    if heap0 == 0 {
        return Some(asked);
    }
    let per_slot = ring::slot_bytes(g.planes, g.cols)
        + ring::descs_per_slot(s, g.cols, esp_hub75::max_dma_chunk_size()) * core::mem::size_of::<DmaDescriptor>();
    let budget = heap0.saturating_sub(BOOT_HEAP_FLOOR + RING_HEAP_RESERVE + RING_FIXED_BYTES);
    let cap = (budget / per_slot.max(1)) as u32;
    let n = asked.min(cap);
    (n > ring::GUARD_SLOTS).then_some(n)
}

impl Hub75Ring {
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
        // Row pairs per chunk: pack, write the chunk back from the data
        // cache (the DMA reads PSRAM off the bus, not through the cache),
        // then give the queue a turn.
        const CHUNK: usize = 8;
        for r in 0..rows {
            // SAFETY: frame `i` is free (`free_frame`: no pass reads it and
            // it is not `newest`) and the output task is the only packer.
            let words = unsafe { core::slice::from_raw_parts_mut(dst.add(r * slot_words), slot_words) };
            ring::format_slot_for(words, g, control, &sched, r);
            let planes = &mut words[ring::plane_row(0) * g.cols..];
            let _ = pie::pack_row_pair(planes, g, r, rgb, self.remap, t, &mut self.pads);
            if r % CHUNK == CHUNK - 1 || r + 1 == rows {
                let from = r / CHUNK * CHUNK;
                // SAFETY: rows `from..=r` of frame `i`, just written.
                cache_writeback(unsafe { dst.add(from * slot_words) }.cast::<u8>().cast_const(), (r + 1 - from) * slot_words * 2);
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

/// Drain the queue with copier `p` on `core`: claim every slot the beam
/// has left that can still be filled in time and copy it — by the DMA
/// engine ([`drain_dma`]) when it exists and the lever is on, by this
/// core's `memcpy` ([`drain_cpu`]) otherwise. Returns how many claims were
/// taken. Reentrant across cores: the queue word is the only shared write
/// on the CPU path, and the engine's critical section serializes the DMA
/// path.
fn drain(p: &mut Packer, s: &Shared, core: usize) -> u32 {
    if DMA_ON.load(Ordering::Relaxed) {
        return drain_dma(p, s, core, false);
    }
    drain_cpu(p, s, core)
}

/// Microseconds the beam needs to reach claim `a` from where it is, less
/// the slot it may already be in: what a copy of `a` has to beat.
fn left_us(s: &Shared, a: u32, abs_dma: u32) -> u32 {
    let left_clocks = s.ring.until_late(a, abs_dma).saturating_sub(1) * s.slot_clocks;
    (u64::from(left_clocks) * 1_000_000 / u64::from(s.clock_hz.max(1))) as u32
}

/// Re-join the queue behind a beam that has overtaken its head (the first
/// drain after boot, or a stall that lapped the ring); counts the skipped
/// claims `late` once the copier has synced once. Shared by both paths.
fn rejoin(p: &mut Packer, s: &Shared, word: u32, claim: Claim, abs_dma: u32) {
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
}

/// The DMA path of [`drain`]: under the engine's critical section, settle
/// the batch in flight (still copying → nothing to do this turn), then
/// claim up to [`BATCH_SLOTS`] fillable slots whose deadlines the batch's
/// own copy time can meet, and start them as one transfer. `by_isr` marks
/// a turn taken by the completion interrupt. The claims are credited to
/// `core` when the batch completes.
fn drain_dma(p: &mut Packer, s: &Shared, core: usize, by_isr: bool) -> u32 {
    ENGINE.lock(|c| {
        let mut b = c.borrow_mut();
        let Some(e) = b.as_mut() else {
            // No engine: `DMA_ON` is forced off when the calibration fails
            // and `set_dma` refuses to turn it on, so this is a boot race
            // at worst — the next turn takes the CPU path.
            return 0;
        };
        let slack_us = RING_SLACK_US.load(Ordering::Relaxed);
        if e.settle(now_us(), slack_us) == Settled::Busy {
            if core == 0 && !by_isr && HYBRID.load(Ordering::Relaxed) {
                // The DMA is still copying: the output task copies the
                // next claims itself meanwhile (the lock is held for the
                // copies — core 1's turns are DMA kicks and find it busy
                // for at most a few slots' worth).
                return drain_cpu(p, s, 0);
            }
            return 0;
        }
        let Some(abs_dma) = abs_dma(s) else {
            Engine::clear_ints();
            return 0;
        };
        let mut k = 0usize;
        while k < BATCH_SLOTS {
            let word = NEXT_FILL.load(Ordering::Acquire);
            let (claim, next) = ring::claim_next(word, NEWEST.load(Ordering::Acquire), &s.ring);
            if s.ring.late(claim.abs, abs_dma) {
                rejoin(p, s, word, claim, abs_dma);
                continue;
            }
            if !s.ring.fillable(claim.abs, abs_dma) {
                if k == 0 {
                    RING_IDLE.fetch_add(1, Ordering::Relaxed);
                }
                break;
            }
            // The skip rule (design §7) for a batch: this claim's slot is
            // written when the DMA has done the `k` before it and this one,
            // plus the turn's own latency. Half again as margin.
            let need_us = DMA_MARGIN_US + (k as u32 + 1) * e.slot_us * 3 / 2;
            if left_us(s, claim.abs, abs_dma) < need_us {
                if NEXT_FILL.compare_exchange(word, next, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                    RING_LATE.fetch_add(1, Ordering::Relaxed);
                    if s.ring.row(claim.abs) == 0 {
                        PASS_FRAME[(s.ring.pass(claim.abs) % 2) as usize].store(claim.buf, Ordering::Release);
                    }
                }
                continue;
            }
            if NEXT_FILL.compare_exchange(word, next, Ordering::AcqRel, Ordering::Relaxed).is_err() {
                continue;
            }
            if s.ring.row(claim.abs) == 0 {
                PASS_FRAME[(s.ring.pass(claim.abs) % 2) as usize].store(claim.buf, Ordering::Release);
            }
            let slot = s.ring.slot(claim.abs) as usize;
            let row = s.ring.row(claim.abs) as usize;
            // SAFETY: the claim is ours by the CAS; the DMA has left the
            // slot (`fillable`); the packed frame a pass reads is kept out
            // of `write_frame`'s choice while the pass is in flight.
            unsafe {
                e.fill(
                    k,
                    s.packed[usize::from(claim.buf) % FRAMES].add(row * s.slot_words).cast::<u8>(),
                    s.slots.add(slot * s.slot_words).cast::<u8>(),
                );
            }
            k += 1;
        }
        if k == 0 {
            Engine::clear_ints();
            return 0;
        }
        e.close(k);
        e.busy = true;
        COPY_BUSY.store(true, Ordering::Release);
        e.slots = k as u32;
        e.core = core;
        e.by_isr = by_isr;
        e.t0 = now_us();
        Engine::start(e.rx, e.tx);
        k as u32
    })
}

/// The copy channel's interrupt (`in_suc_eof` and the error bits, bound by
/// [`arm_steal`] on the AppCpu): account for the batch that just finished
/// and start the next one at once, so the DMA is never idle while the
/// queue has claims. This chaining runs whatever the `steal` lever says —
/// that lever is the core-1 TIMER's turns; the completion interrupt costs
/// this core a few microseconds per batch and only when a batch ends, so
/// "steal off" measures the chain alone and "steal on" the chain plus the
/// timer. (`dma` off leaves the handler to the accounting only.)
extern "C" fn copy_isr() {
    let Some(s) = shared() else {
        Engine::clear_ints();
        return;
    };
    // SAFETY: `CORE1` is this core's interrupt-context packer (the steal
    // timer and this handler share one priority level, so neither
    // re-enters the other).
    let p = unsafe { &mut *core::ptr::addr_of_mut!(CORE1) };
    let core = if Cpu::current() == Cpu::AppCpu { 1 } else { 0 };
    if DMA_ON.load(Ordering::Relaxed) {
        drain_dma(p, s, core, true);
    } else {
        ENGINE.lock(|c| match c.borrow_mut().as_mut() {
            Some(e) => {
                let _ = e.settle(now_us(), RING_SLACK_US.load(Ordering::Relaxed));
                if !e.busy {
                    Engine::clear_ints();
                }
            }
            None => Engine::clear_ints(),
        });
    }
}

/// The CPU path of [`drain`] (#892 step 1): claim and `memcpy` every slot
/// the beam has left that can still be filled in time, on this core.
fn drain_cpu(p: &mut Packer, s: &Shared, core: usize) -> u32 {
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
            rejoin(p, s, word, claim, abs_dma);
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
    // The copy engine's completion interrupt, on this core too (#892 step
    // 2): `set_interrupt_handler` binds on the calling core, and the
    // handler's re-drain is the steal's work. Until it is bound the engine
    // runs polled — every copier's turn settles the batch in flight.
    let bound = COPY_CHANNEL.lock(|c| match c.borrow_mut().as_mut() {
        Some(ch) if ENGINE.lock(|e| e.borrow().is_some()) => {
            ch.set_interrupt_handler(InterruptHandler::new(copy_isr, Priority::Priority1));
            Engine::listen();
            true
        }
        _ => false,
    });
    if bound {
        println!("hub75-ring: GDMA ch{} completion interrupt bound on {:?}", COPY_CH, Cpu::current());
    }
}

extern "C" fn steal_isr() {
    STEAL_TIMER.lock(|c| {
        if let Some(t) = c.borrow_mut().as_mut() {
            t.clear_interrupt();
        }
    });
    steal();
}

/// The panel chain's own channel (GDMA 0) and the LCD, raw, for a freeze
/// diagnosis over HTTP (`pass.ring.lcd`, #892 step 2 — the 2x1 `lsb 7`
/// chain stopped after a minute of DMA copying on 2026-09-29): `[ch0
/// OUT_INT_RAW, ch0 OUT_STATE, ch0 OUT_LINK, ch0 OUTFIFO_STATUS, ch0
/// OUT_DSCR, LCD_USER, LC_DMA_INT_RAW, copy-channel OUT_INT_RAW, copy-channel
/// IN_INT_RAW, ch0 OUT_PRI]`.
pub fn lcd_probe() -> [u32; 10] {
    let ch0 = DMA::regs().ch(0);
    let lcd = LCD_CAM::regs();
    let cp = copy_ch!();
    [
        ch0.out_int().raw().read().bits(),
        ch0.out_state().read().bits(),
        ch0.out_link().read().bits(),
        ch0.outfifo_status().read().bits(),
        ch0.out_dscr().read().bits(),
        lcd.lcd_user().read().bits(),
        lcd.lc_dma_int_raw().read().bits(),
        cp.out_int().raw().read().bits(),
        cp.in_int().raw().read().bits(),
        ch0.out_pri().read().bits(),
    ]
}

/// GDMA arbitration priorities: on (the boot default) puts the panel chain
/// (channel 0, TX) at the top (9) and the copy channel at the bottom (0);
/// off is the reset state (all equal) — `POST /api/ring {"pri":false}` for
/// the A/B. **Load-bearing** (2026-09-29): at equal priority the copy
/// channel's 64-byte PSRAM bursts held the panel chain off the bus long
/// enough for the LCD's FIFO to run dry at 2x1 `lsb 7` (27 MB/s of copy
/// beside 40 MB/s of scan-out), and the LCD_CAM treats an empty FIFO as the
/// end of its continuous transaction: `lcd_start` clears, `trans_done`
/// fires, the panel freezes on its last rows with channel 0 still cycling
/// (`pass.ring.lcd` showed exactly that: `LCD_USER` bit 27 clear,
/// `LC_DMA_INT_RAW` 3, `OUTFIFO_STATUS` full). `lcd_restarts` counts the
/// watchdog that puts `lcd_start` back if it ever happens again.
pub static PRI: AtomicBool = AtomicBool::new(false);
/// Times `flush` found the LCD stopped (`lcd_start` clear) and restarted
/// it (`pass.ring.lcd_restarts`). 0 is the claim.
pub static RING_LCD_RESTARTS: AtomicU32 = AtomicU32::new(0);

/// The LCD watchdog: if the LCD_CAM has ended its continuous transaction
/// (an underflow — see [`set_priority`]), restart it. Channel 0 keeps
/// cycling the ring regardless, so the restart resumes from wherever the
/// chain is; every row carries its own address, so nothing is mis-shown.
fn lcd_watchdog() {
    let lcd = LCD_CAM::regs();
    if lcd.lcd_user().read().lcd_start().bit_is_set() {
        return;
    }
    RING_LCD_RESTARTS.fetch_add(1, Ordering::Relaxed);
    lcd.lc_dma_int_clr().write(|w| w.lcd_trans_done_int_clr().set_bit());
    lcd.lcd_user().modify(|_, w| w.lcd_always_out_en().set_bit().lcd_dout().set_bit());
    lcd.lcd_user().modify(|_, w| w.lcd_update().set_bit().lcd_start().set_bit());
}
pub fn set_priority(on: bool) {
    let ch0 = DMA::regs().ch(0);
    let cp = copy_ch!();
    let hi = if on { 9 } else { 0 };
    // SAFETY: 4-bit priority fields, 0..=9 per the TRM.
    ch0.out_pri().write(|w| unsafe { w.tx_pri().bits(hi) });
    cp.out_pri().write(|w| unsafe { w.tx_pri().bits(0) });
    cp.in_pri().write(|w| unsafe { w.rx_pri().bits(0) });
    PRI.store(on, Ordering::Relaxed);
}

/// Is a copy batch still reading PSRAM? The flash fence (`core1.rs`) asks
/// before every flash op and waits until this is false: a GDMA burst that
/// is stalled on a PSRAM read while SPI1 programs flash sits on the
/// engine's shared read path and holds off the panel chain's own reads on
/// channel 0 for the whole op — the LCD then underflows (2026-09-29, the
/// 2x1 `lsb 7` chain stopped ~60 s after boot, at the boot-ok store).
/// Nothing can START a batch during the op (the other core is parked, the
/// fencing core's interrupts are masked), so waiting out the one in flight
/// is the whole rule. One raw-bit read; no lock.
pub fn copy_busy() -> bool {
    COPY_BUSY.load(Ordering::Acquire) && !Engine::poll().0
}

/// The `dma` lever (`POST /api/ring {"dma":…}`): on only when the engine
/// exists (a board whose calibration copy failed stays on the CPU path
/// whatever is asked). Flipping it off mid-batch is safe — the batch in
/// flight is settled by the next DMA turn, which every ISR still takes.
pub fn set_dma(on: bool) {
    let have = ENGINE.lock(|c| c.borrow().is_some());
    DMA_ON.store(on && have, Ordering::Relaxed);
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
        if self.hub75.is_some() {
            lcd_watchdog();
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
