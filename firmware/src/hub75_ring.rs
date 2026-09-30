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
//!   the counter and, in its top two bits, which packed frame this pass reads).
//!   A turn of a copier claims up to [`BATCH_SLOTS`] such slots and hands
//!   the batch to GDMA channel 1 in memory-to-memory mode ([`Engine`]): one
//!   descriptor chain over the row pairs in the packed frame (PSRAM, read in
//!   64-byte bursts straight off the bus — no cache-line fills, which is what
//!   made the CPU copy 15 MB/s beside a 16384-px engine), one over the slots
//!   (internal SRAM). The CPU's part of a pass is a few microseconds of
//!   descriptor writes; the copy runs while the CPU does something else.
//!   Since #897 a turn that finds the DMA busy does not wait for it: the
//!   descriptor pools are circular rings with `check_owner` on (the DMA
//!   stops at the first CPU-owned entry), and the turn APPENDS its run
//!   behind the chain in flight — owner bits handed over, then the
//!   `inlink_restart`/`outlink_restart` bits for a DMA that had already
//!   stopped — so the DMA does not idle between batches for an interrupt's
//!   latency. The skip rule counts the slots still queued ahead of each
//!   claim. Completion is per run (one EOF each), read from the receive
//!   descriptors' owner write-back; the completion interrupt (bound on the
//!   AppCpu with the steal) retires runs and appends the next — whatever
//!   the `steal` lever says, which is the core-1 TIMER's turns only. The
//!   `append` lever (default on when the boot's append probe passed) and
//!   `minfill` (a minimum run length while the slack allows, default off)
//!   are the A/Bs. The packed frame is written back from
//!   the data cache after every eight row pairs of `pack_frame`, so the DMA
//!   reads what the packer wrote. Claims are taken only while
//!   [`Ring::fillable`] and while the slots left before the beam cover the
//!   batch's own measured copy time; a claim that cannot make it is SKIPPED
//!   and counted `late` — the slot then shows the row it already held, at
//!   that row's own address (stale, never a mixed-address row, design §7).
//!   The CPU `memcpy` of step 1 stays as the fallback when the DMA engine
//!   could not be built, and as the A/B lever (`POST /api/ring
//!   {"dma":false}`).
//! - **At a full-frame ring the copy is skipped while the frame is
//!   unchanged** (#896): with `n == rows` slot `r` carries row pair `r`
//!   on every pass, so a claim is only worth a copy when the pass's packed
//!   frame is not the one the slot was last filled from. Every publish
//!   stamps the frame with a fresh generation (`FRAME_GEN`), every landed
//!   copy stamps its slot (`SLOT_GEN`), and a claim whose stamps match is
//!   advanced without a copy — `pass.ring.reused` counts them. PSRAM
//!   traffic then follows the RENDER rate instead of the pass rate
//!   (2x1 `lsb 7`: 32 row pairs × 20 fps instead of × 400 passes/s),
//!   which is what makes the low-plane schedules watchable on the 2x1.
//!   Below a full frame nothing changes: the stamps are kept but never
//!   match, because the same slot holds a different row pair every pass.
//! - **The ring is capped by the heap** ([`RING_HEAP_RESERVE`], or
//!   [`RING_HEAP_RESERVE_FULL`] for a full-frame ask — `capped_slots`):
//!   `ring_ms` asks for a slack, the boot gives the slots that slack needs
//!   OR as many as leave the boot floor plus a reserve for the engine,
//!   whichever is fewer. A 4x1 at `ring_ms 3` asked for 22 slots of 4 KB and left 15 KB
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
//! `dma_batches` (runs completed), `dma_isr` (runs the completion interrupt started),
//! `dma_errors`, `appends` (runs appended to a chain in flight),
//! `dma_restarts` (appends the restart bits resumed), `deferred` (`minfill`),
//! `append`, `minfill`, `steal`; and since #914 `torn` (copies that landed
//! after the beam had reached their slot — the design's forbidden case,
//! the oracle for "garbage on the far panels"), `dma_us_win` / `run_us_max`
//! / `run_us_win` (the per-slot copy's and a whole run's worst of the last
//! few seconds, `WindowMax`) and `steal_lat_max` / `steal_lat_win` (core
//! 1's interrupt latency as the steal timer sees it).

use core::cell::{RefCell, UnsafeCell};
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU8, Ordering};

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

/// Entries in the copy engine's circular descriptor rings (#897): two runs
/// of [`BATCH_SLOTS`] — the run the DMA is on, and one appended behind it
/// while it copies. An entry is freed when its run completes, so this many
/// slots at most are ever queued ahead of a claim.
const QUEUE_SLOTS: usize = 2 * BATCH_SLOTS;

/// Runs in flight at most — a run is often 1–3 slots (the queue is drained
/// as fast as the beam frees slots), so this, not [`QUEUE_SLOTS`], is what
/// usually bounds the chain.
const MAX_RUNS: usize = 6;

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

/// The reserve a FULL-FRAME ring may cut into instead (`capped_slots`,
/// #896, 2026-09-30): the 2x1 needs 32 slots of 2 KB for the ring that
/// skips its copies, and 48 KB trimmed it to 25. The 16 KB comes out of
/// what #905 freed — the per-request and load-time transients (HTTP
/// bodies, the jsonview snapshots, the resume staging, the JIT compile) are
/// arena blocks now. Measured on the 2x1 with Jeremy's two-engine scene
/// resident: 22–28 KB of runtime heap at `lsb 7` / `lsb 3` on the 32-slot
/// ring, 0.05 % / 0.16 % late where the 25-slot ring ran 6.9 % / 16 %.
pub(crate) const RING_HEAP_RESERVE_FULL: usize = 32 * 1024;

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
/// The `append` lever (#897, `POST /api/ring {"append":…}`, default on): a
/// turn that finds the DMA busy appends a run to the chain in flight. Off
/// is step 2's shape — one run in flight, the next started by the
/// completion interrupt or a later turn — for the A/B.
pub static APPEND: AtomicBool = AtomicBool::new(true);
/// The `minfill` lever (#897, `POST /api/ring {"minfill":N}`, default 1 =
/// off): a turn that finds fewer than N claims fillable leaves them for a
/// later turn while the queue head's slack allows (`ring::defer_run`), so
/// runs are longer and fewer. Clamped to 1..=`BATCH_SLOTS`.
pub static MINFILL: AtomicU32 = AtomicU32::new(1);
/// Runs appended to a chain in flight (#897), and of those, the ones that
/// found the previous run already complete — where the restart bits are
/// what resumed the DMA.
pub static RING_APPENDS: AtomicU32 = AtomicU32::new(0);
pub static RING_DMA_RESTARTS: AtomicU32 = AtomicU32::new(0);
/// Turns that left a short run for later under `minfill`.
pub static RING_DEFERRED: AtomicU32 = AtomicU32::new(0);
/// The DMA's time per slot, microseconds (clipped EWMA — the skip rule's
/// number), and the raw worst.
pub static RING_DMA_US: AtomicU32 = AtomicU32::new(0);
pub static RING_DMA_US_MAX: AtomicU32 = AtomicU32::new(0);
/// Copies that landed AFTER the beam had reached their slot — a slot the
/// DMA was still writing while channel 0 read it, i.e. a torn row pair
/// (mixed frames, or mixed row pairs below a full-frame ring). The skip
/// rule's promise (design §7) is that this reads 0; it is what the eye
/// sees as garbage on the far end of a chain (2026-09-30, Mandelbrot 2D
/// on the 4x1).
pub static RING_TORN: AtomicU32 = AtomicU32::new(0);
/// A run's whole latency, hand-over to landing, microseconds — the worst
/// ever and the worst of the last second or so (`WindowMax`). The per-slot
/// numbers hide a run that queued behind a stalled chain.
pub static RING_RUN_US_MAX: AtomicU32 = AtomicU32::new(0);
pub static RING_RUN_US_WIN: WindowMax = WindowMax::new();
/// The per-slot copy time's worst of the last few seconds — `dma_us_max`
/// is a lifetime worst and reads the WiFi bring-up stall for ever. A
/// diagnostic, NOT the skip rule's number: budgeting claims at it was
/// tried and starved the ring (`ring::dma_need_us` has the story).
pub static RING_DMA_US_WIN: WindowMax = WindowMax::new();

/// A maximum over a sliding window of a few seconds: the worst of the
/// current [`WindowMax::BUCKET_US`] bucket and of the previous one, `read`
/// the larger of the two — so a value is held for two to four seconds,
/// long enough to be read by a status poll after the burst it recorded.
pub struct WindowMax {
    cur: AtomicU32,
    last: AtomicU32,
    sec: AtomicU32,
}

impl WindowMax {
    const BUCKET_US: u64 = 2_000_000;

    const fn new() -> Self {
        Self { cur: AtomicU32::new(0), last: AtomicU32::new(0), sec: AtomicU32::new(0) }
    }

    fn sample(&self, now_us: u64, v: u32) {
        let s = (now_us / Self::BUCKET_US) as u32;
        if self.sec.load(Ordering::Relaxed) != s {
            self.last.store(self.cur.swap(0, Ordering::Relaxed), Ordering::Relaxed);
            self.sec.store(s, Ordering::Relaxed);
        }
        self.cur.fetch_max(v, Ordering::Relaxed);
    }

    pub fn read(&self) -> u32 {
        self.last.load(Ordering::Relaxed).max(self.cur.load(Ordering::Relaxed))
    }
}
/// The boot calibration copy's time per slot, microseconds — the DMA on an
/// idle bus, before WiFi or an engine (0 = the calibration failed).
pub static RING_DMA_CAL_US: AtomicU32 = AtomicU32::new(0);
/// Batches completed; batches the completion interrupt started (the rest
/// were started by a copier's turn); batches that ended in a descriptor
/// error or a stall reset.
pub static RING_DMA_BATCHES: AtomicU32 = AtomicU32::new(0);
pub static RING_DMA_ISR: AtomicU32 = AtomicU32::new(0);
pub static RING_DMA_ERRORS: AtomicU32 = AtomicU32::new(0);
/// Claims advanced WITHOUT a copy because the slot already held that row
/// pair of that packed frame (Gitea #896) — only ever non-zero on a
/// full-frame ring (`rows == asked == the panel's row pairs`), where slot
/// `r` carries row pair `r` on every pass and the copy is needed only
/// when the frame changed. `reused + packed_core0 + packed_core1 + late`
/// is the claim count; on the 2x1 at `lsb 7` the copies fall from the pass
/// rate (~400/s × 32) to the render rate (~20/s × 32).
pub static RING_REUSED: AtomicU32 = AtomicU32::new(0);
/// The packed-frame generation counter: bumped by every `write_frame`
/// publish, stamped on the published frame ([`FRAME_GEN`]) and, once a
/// slot has been copied from it, on the slot ([`SLOT_GEN`]). Two equal
/// stamps mean byte-identical content: a frame index is recycled every
/// four publishes, a generation never is.
static GEN: AtomicU32 = AtomicU32::new(0);
/// Generation of the frame each packed-frame index currently holds.
static FRAME_GEN: [AtomicU32; FRAMES] = [const { AtomicU32::new(0) }; FRAMES];
/// Generation of the packed frame each slot was last COPIED from — written
/// after the copy landed (by the copying core, or by the batch's
/// completion) — `SLOT_GEN_NONE` when unknown: never copied, or a batch
/// that failed with the slot half-written.
static SLOT_GEN: [AtomicU32; MAX_SCAN] = [const { AtomicU32::new(SLOT_GEN_NONE) }; MAX_SCAN];
const SLOT_GEN_NONE: u32 = u32::MAX;
/// The flash-write blank (Gitea #852, design §7): the panel chain's
/// descriptors, each one's live buffer pointer and its stand-in in one
/// dark slot at the same offset. Published once at boot; the fence swaps
/// every `buffer` to the dark pointer before an erase/program and back
/// after, from the fencing core with its interrupts masked — a 32-bit
/// store the DMA picks up at its next descriptor fetch, the way the
/// atomic-swap patch flips a ring.
struct Blank {
    descs: *mut DmaDescriptor,
    n: usize,
    live: *const *mut u8,
    dark: *const *mut u8,
}
struct BlankCell(UnsafeCell<Option<Blank>>);
// SAFETY: written once at boot before the fence can matter, read-only after.
unsafe impl Sync for BlankCell {}
static BLANK: BlankCell = BlankCell(UnsafeCell::new(None));
static BLANK_READY: AtomicBool = AtomicBool::new(false);

/// The chain is pointed at the dark slot right now.
static BLANKED: AtomicBool = AtomicBool::new(false);
/// Erase/program fences currently inside `fence_blank`..`fence_unblank`
/// (the restore never runs while one is open).
static IN_FENCE: AtomicU32 = AtomicU32::new(0);
/// Milliseconds since boot at which the chain may be restored: the last
/// fence's release plus [`BLANK_HOLD_MS`].
static UNBLANK_AT_MS: AtomicU32 = AtomicU32::new(0);
/// Quiet time after the last erase/program before the picture comes back.
/// A flash write is never alone — an OTA is a chunk every ~50 ms for 15 s,
/// a pattern save several hundred fences in half a second — and restoring
/// between them made the whole panel flicker at the chunk rate (Jeremy,
/// 2026-09-30, the first OTA onto the ring default). With the hold an OTA
/// is one dark stretch and a save one short one; a lone config write costs
/// a 200 ms blink.
const BLANK_HOLD_MS: u32 = 200;

fn now_ms() -> u32 {
    (now_us() / 1000) as u32
}

fn set_chain(b: &Blank, dark: bool) {
    let from = if dark { b.dark } else { b.live };
    for i in 0..b.n {
        // SAFETY: the descriptors and the pointer tables are leaked internal
        // SRAM the driver owns; a 32-bit store the DMA reads at its next
        // descriptor fetch.
        unsafe { core::ptr::addr_of_mut!((*b.descs.add(i)).buffer).write_volatile(*from.add(i)) };
    }
}

fn blank_table() -> Option<&'static Blank> {
    if !BLANK_READY.load(Ordering::Acquire) {
        return None;
    }
    // SAFETY: read-only after the release.
    unsafe { (*BLANK.0.get()).as_ref() }
}

/// Point the panel chain at the dark slot for an erase/program fence.
/// Returns false when there is no ring to blank (the fence then skips the
/// release call). Idempotent across back-to-back fences: the chain stays
/// dark until [`blank_tick`] sees the quiet time out.
pub fn fence_blank() -> bool {
    let Some(b) = blank_table() else { return false };
    IN_FENCE.fetch_add(1, Ordering::AcqRel);
    if !BLANKED.swap(true, Ordering::AcqRel) {
        set_chain(b, true);
        RING_BLANKED.fetch_add(1, Ordering::Relaxed);
    }
    true
}

/// The fence is over: start (or restart) the quiet timer. The picture comes
/// back from the output task's next turn after [`BLANK_HOLD_MS`] without
/// another fence; the rows the beam passed meanwhile are refilled by then
/// (the copiers run while the chain is dark), so nothing stale is shown.
pub fn fence_unblank() {
    UNBLANK_AT_MS.store(now_ms().wrapping_add(BLANK_HOLD_MS), Ordering::Release);
    IN_FENCE.fetch_sub(1, Ordering::AcqRel);
}

/// Blank for good: the board is about to reset (`reboot_task`, after an
/// OTA or `POST /api/reboot`). Same as a fence that is never released, so
/// the picture does not come back for the 400 ms between the last flash
/// write and the reset (Jeremy saw that frame, 2026-09-30) and the DMA is
/// not stopped mid-pass with rows lit.
pub fn hold_blank() {
    let _ = fence_blank();
}

/// From the output task's turn: restore the chain once the flash has been
/// quiet for the hold and no fence is open.
fn blank_tick() {
    if !BLANKED.load(Ordering::Acquire) || IN_FENCE.load(Ordering::Acquire) != 0 {
        return;
    }
    let due = UNBLANK_AT_MS.load(Ordering::Acquire);
    if (now_ms().wrapping_sub(due) as i32) < 0 {
        return;
    }
    let Some(b) = blank_table() else { return };
    set_chain(b, false);
    BLANKED.store(false, Ordering::Release);
}

/// A copy run is in flight (mirrors `Engine::nruns > 0` without the lock):
/// what the flash fence reads before a flash op (`output::transfer_busy`).
static COPY_BUSY: AtomicBool = AtomicBool::new(false);
/// The last receive descriptor of the NEWEST run in flight: once it has
/// landed, the whole chain has (runs complete in order), whatever the
/// engine's bookkeeping has seen yet.
static COPY_TAIL: AtomicPtr<DmaDescriptor> = AtomicPtr::new(core::ptr::null_mut());
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
/// A chain that is EXTENDED while in flight (#897). The two descriptor
/// pools (internal SRAM — the DMA reads its descriptors from internal
/// memory only) are each a CIRCULAR ring of [`QUEUE_SLOTS`] entries, an
/// entry being one slot's `per_slot` descriptors of `chunk_rows` slot rows
/// (a descriptor carries at most 4095 bytes, a slot is 1–8 KB). The `next`
/// links are written once, at boot, and never change; both halves run with
/// `check_owner` on, so the DMA stops at the first descriptor the CPU owns
/// — the entry after the last one handed over — and nothing else ends a
/// chain. A turn hands over a RUN of up to [`BATCH_SLOTS`] entries at
/// `tail` by writing each descriptor's buffer and then its flag word with
/// the DMA owner bit (one 32-bit store; the receive side first, so the
/// transmit side never pushes data the receive side has no descriptor
/// for), the run's last transmit descriptor carrying the EOF, so the IN
/// half's `in_suc_eof` fires once per run. With nothing in flight the run
/// is started fresh (`start`: reset, link address, start). With runs in
/// flight it is APPENDED: the owner bits are all it takes while the DMA
/// has not reached the stop, and `inlink_restart`/`outlink_restart` —
/// issued after every append, ESP-IDF's `async_memcpy` pattern — make a
/// DMA that had already stopped re-read the descriptor it stopped on,
/// which is now the new run's head. `dma_restarts` counts the appends that
/// found the previous run already complete, i.e. where the restart is what
/// resumed the copy.
///
/// Completion is per run and read from the descriptors, not from the
/// interrupt bits (which coalesce): the GDMA always writes a receive
/// descriptor back with the owner bit cleared once its buffer is full, so
/// a run whose last receive descriptor reads CPU-owned has landed.
/// [`Engine::settle`] retires completed runs oldest first — `pending`
/// stamps [`SLOT_GEN`] per entry (#896), the per-slot time is measured
/// from the later of the run's hand-over and the previous run's completion
/// — and resets the whole queue on an error bit or a stall of the oldest
/// run (every run still in flight then counts `late`, its slots unknown).
struct Engine {
    /// Runs in flight, oldest at `run_head`; `nruns` of them.
    runs: [Run; MAX_RUNS],
    run_head: u8,
    nruns: u8,
    /// The oldest entry in flight, the next free entry, and how many are
    /// in flight (`head + inflight == tail` modulo the ring).
    head: u8,
    tail: u8,
    inflight: u8,
    /// When the last run completion was seen — the earliest the next run
    /// in the chain can have started copying.
    done_at: u64,
    /// The DMA's time per slot, microseconds — clipped EWMA (a sample can
    /// at most double it), seeded by the boot calibration.
    slot_us: u32,
    tx: *mut DmaDescriptor,
    rx: *mut DmaDescriptor,
    per_slot: usize,
    chunk_rows: usize,
    row_bytes: usize,
    slot_rows: usize,
    /// `(slot, generation)` of each entry's claim, stamped into
    /// [`SLOT_GEN`] when the entry's run completes (#896).
    pending: [(u8, u32); QUEUE_SLOTS],
    /// The claim (absolute emission) each entry copies, for the torn check
    /// at retirement (`RING_TORN`).
    pending_abs: [u32; QUEUE_SLOTS],
}

/// One run of entries handed to the DMA by one turn.
#[derive(Clone, Copy)]
struct Run {
    first: u8,
    len: u8,
    /// The core whose turn handed it over (credited `packed_core0/1`).
    core: u8,
    /// Handed over by the completion interrupt (credited to `dma_isr`).
    by_isr: bool,
    t0: u64,
}

impl Run {
    const EMPTY: Self = Self { first: 0, len: 0, core: 0, by_isr: false, t0: 0 };
}

// SAFETY: the raw descriptor pointers are used only under `ENGINE`'s
// critical section (and `copy_busy`'s single read of an owner bit), and
// the pools are leaked internal SRAM.
unsafe impl Send for Engine {}

/// The copy channel's register block.
macro_rules! copy_ch {
    () => {
        DMA::regs().ch(COPY_CH)
    };
}

/// The receive descriptor has been written back by the GDMA (owner CPU):
/// its buffer is full. A volatile read of the flag word.
fn landed(d: *const DmaDescriptor) -> bool {
    // SAFETY: `d` is a descriptor in the engine's leaked pools.
    !unsafe { core::ptr::addr_of!((*d).flags).read_volatile() }.owner()
}

impl Engine {
    /// Program the channel for memory-to-memory copies from PSRAM into
    /// internal SRAM. Once, at boot; a fresh start only resets the FSMs.
    /// `check_owner` on both halves: the owner bit is what ends the chain
    /// (the descriptor rings are circular).
    fn configure() {
        let ch = copy_ch!();
        Self::reset_fsm();
        // SAFETY (the `bits` writes): field-width values from the TRM —
        // burst size 2 = 64 B, peripheral select `COPY_PERI`.
        ch.in_conf0().modify(|_, w| w.mem_trans_en().set_bit().indscr_burst_en().set_bit().in_data_burst_en().set_bit());
        ch.in_conf1().modify(|_, w| unsafe { w.in_ext_mem_bk_size().bits(EXT_BURST_64) }.in_check_owner().set_bit());
        ch.in_peri_sel().modify(|_, w| unsafe { w.peri_in_sel().bits(COPY_PERI) });
        ch.out_conf0().modify(|_, w| {
            w.outdscr_burst_en().set_bit().out_data_burst_en().set_bit().out_auto_wrback().set_bit().out_eof_mode().set_bit()
        });
        ch.out_conf1().modify(|_, w| unsafe { w.out_ext_mem_bk_size().bits(EXT_BURST_64) }.out_check_owner().set_bit());
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
    /// handler is bound by [`arm_steal`] on the core that runs it). Not the
    /// descriptor-error bits: with `check_owner` on, reaching the CPU-owned
    /// entry after the last run is how every chain ends, and the owner
    /// error it may raise is the normal stop, not a fault.
    fn listen() {
        let ch = copy_ch!();
        ch.in_int().ena().modify(|_, w| w.in_suc_eof().set_bit().in_dscr_empty().set_bit().in_err_eof().set_bit());
    }

    /// A fault on the channel, from the raw interrupt bits: the IN half ran
    /// out of receive descriptors with data still arriving (the two chains
    /// out of step), or a data error.
    fn faulted() -> bool {
        let i = copy_ch!().in_int().raw().read();
        i.in_dscr_empty().bit_is_set() || i.in_err_eof().bit_is_set()
    }

    /// Start the chain at entry `e`: RX first, then TX (the order the TRM
    /// asks for on every chip with a memory-to-memory mode).
    fn start(&self, e: usize) {
        let ch = copy_ch!();
        Self::reset_fsm();
        let (rx_head, tx_head) = (self.rx_desc(e, 0), self.tx_desc(e, 0));
        // SAFETY: the low 20 bits of an internal-SRAM descriptor address,
        // the field's width; the DMA supplies the rest.
        ch.in_link().modify(|_, w| unsafe { w.inlink_addr().bits((rx_head as u32) & 0xf_ffff) });
        ch.out_link().modify(|_, w| unsafe { w.outlink_addr().bits((tx_head as u32) & 0xf_ffff) });
        ch.in_link().modify(|_, w| w.inlink_start().set_bit());
        ch.out_link().modify(|_, w| w.outlink_start().set_bit());
    }

    /// Make appended descriptors visible to a DMA that may have stopped on
    /// the first of them: re-read the descriptor at the current link
    /// address, RX first.
    fn restart() {
        let ch = copy_ch!();
        ch.in_link().modify(|_, w| w.inlink_restart().set_bit());
        ch.out_link().modify(|_, w| w.outlink_restart().set_bit());
    }

    fn tx_desc(&self, e: usize, c: usize) -> *mut DmaDescriptor {
        // SAFETY: `e < QUEUE_SLOTS`, `c < per_slot` — inside the pool.
        unsafe { self.tx.add(e * self.per_slot + c) }
    }

    fn rx_desc(&self, e: usize, c: usize) -> *mut DmaDescriptor {
        // SAFETY: as `tx_desc`.
        unsafe { self.rx.add(e * self.per_slot + c) }
    }

    /// Bytes descriptor `c` of an entry carries.
    fn chunk_bytes(&self, c: usize) -> usize {
        let chunk = self.chunk_rows * self.row_bytes;
        chunk.min(self.slot_rows * self.row_bytes - c * chunk)
    }

    /// Write both pools as circular rings, every descriptor CPU-owned
    /// (the stop). Once, at boot, before the DMA is ever started.
    fn link_rings(&mut self) {
        let total = QUEUE_SLOTS * self.per_slot;
        for k in 0..total {
            let nk = (k + 1) % total;
            // SAFETY: `k`, `nk` < `total`, the pools' length; nothing runs yet.
            unsafe {
                let mut d = DmaDescriptor::EMPTY;
                d.set_owner(Owner::Cpu);
                d.next = self.tx.add(nk);
                self.tx.add(k).write_volatile(d);
                d.next = self.rx.add(nk);
                self.rx.add(k).write_volatile(d);
            }
        }
    }

    /// Point entry `e` at one claim: `src` is the row pair in the packed
    /// frame (PSRAM), `dst` its slot (internal). Buffers only — the entry
    /// stays CPU-owned (invisible to the DMA) until [`Engine::launch`].
    ///
    /// # Safety
    /// `e` is a free entry (not in flight); `src`/`dst` point at
    /// `slot_rows * row_bytes` readable/writable bytes that nothing else
    /// touches until the entry's run completes.
    unsafe fn stage(&mut self, e: usize, src: *const u8, dst: *mut u8) {
        let chunk = self.chunk_rows * self.row_bytes;
        for c in 0..self.per_slot {
            let off = c * chunk;
            // SAFETY: in-pool descriptors of a free entry; offsets inside
            // the caller's row pair / slot.
            unsafe {
                core::ptr::addr_of_mut!((*self.tx_desc(e, c)).buffer).write_volatile(src.add(off).cast_mut());
                core::ptr::addr_of_mut!((*self.rx_desc(e, c)).buffer).write_volatile(dst.add(off));
            }
        }
    }

    /// Set the flag words of entries `first..first + len` to DMA-owned —
    /// all receive descriptors, then all transmit ones, the last transmit
    /// descriptor with the EOF. Each flag word is one 32-bit store after the
    /// buffers are in place (`fence`), so the DMA sees an entry whole or
    /// not at all.
    fn hand_over(&mut self, first: usize, len: usize) {
        core::sync::atomic::fence(Ordering::SeqCst);
        for j in 0..len {
            let e = (first + j) % QUEUE_SLOTS;
            for c in 0..self.per_slot {
                let bytes = self.chunk_bytes(c);
                let mut d = DmaDescriptor::EMPTY;
                d.set_size(bytes);
                d.set_length(0);
                d.set_owner(Owner::Dma);
                // SAFETY: an in-pool descriptor of an entry being handed over.
                unsafe { core::ptr::addr_of_mut!((*self.rx_desc(e, c)).flags).write_volatile(d.flags) };
            }
        }
        core::sync::atomic::fence(Ordering::SeqCst);
        for j in 0..len {
            let e = (first + j) % QUEUE_SLOTS;
            for c in 0..self.per_slot {
                let bytes = self.chunk_bytes(c);
                let mut d = DmaDescriptor::EMPTY;
                d.set_size(bytes);
                d.set_length(bytes);
                d.set_owner(Owner::Dma);
                d.set_suc_eof(j + 1 == len && c + 1 == self.per_slot);
                // SAFETY: as above.
                unsafe { core::ptr::addr_of_mut!((*self.tx_desc(e, c)).flags).write_volatile(d.flags) };
            }
        }
        core::sync::atomic::fence(Ordering::SeqCst);
    }

    /// Give entry `e` back to the CPU on both halves — the stop the DMA
    /// halts on. The GDMA clears the owner bit itself on a written-back
    /// descriptor; this makes the stop certain after a reset too.
    fn reclaim(&mut self, e: usize) {
        for c in 0..self.per_slot {
            for d in [self.tx_desc(e, c), self.rx_desc(e, c)] {
                // SAFETY: an in-pool descriptor the DMA is done with (its
                // run completed, or the channel was reset).
                unsafe {
                    let p = core::ptr::addr_of_mut!((*d).flags);
                    let mut f = p.read_volatile();
                    f.set_owner(false);
                    p.write_volatile(f);
                }
            }
        }
    }

    /// The last receive descriptor of `run`: landed ⇔ the run is done.
    fn run_last(&self, run: &Run) -> *mut DmaDescriptor {
        let e = (usize::from(run.first) + usize::from(run.len) - 1) % QUEUE_SLOTS;
        self.rx_desc(e, self.per_slot - 1)
    }

    /// Entries a new run may take now: none while [`MAX_RUNS`] runs are in
    /// flight, else up to [`BATCH_SLOTS`] of the free ring.
    fn room(&self) -> usize {
        if usize::from(self.nruns) >= MAX_RUNS {
            return 0;
        }
        BATCH_SLOTS.min(QUEUE_SLOTS - usize::from(self.inflight))
    }

    /// Hand the `len` entries staged at `tail` to the DMA as one run —
    /// started fresh when nothing is in flight, appended to the chain
    /// otherwise.
    fn launch(&mut self, len: usize, core: usize, by_isr: bool, now: u64) {
        let first = usize::from(self.tail);
        let appending = self.nruns > 0;
        // Whether the chain had already drained before this run exists:
        // then the DMA sits (or is about to sit) on the stop that is now
        // this run's head, and the restart below is what resumes it.
        let drained = appending && {
            let prev = self.runs[(usize::from(self.run_head) + usize::from(self.nruns) - 1) % MAX_RUNS];
            landed(self.run_last(&prev))
        };
        self.hand_over(first, len);
        let run = Run { first: first as u8, len: len as u8, core: core as u8, by_isr, t0: now };
        self.runs[(usize::from(self.run_head) + usize::from(self.nruns)) % MAX_RUNS] = run;
        self.nruns += 1;
        self.tail = ((first + len) % QUEUE_SLOTS) as u8;
        self.inflight += len as u8;
        COPY_TAIL.store(self.run_last(&run), Ordering::Release);
        COPY_BUSY.store(true, Ordering::Release);
        if appending {
            Self::restart();
            RING_APPENDS.fetch_add(1, Ordering::Relaxed);
            if drained {
                RING_DMA_RESTARTS.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.start(first);
        }
    }

    /// Account for the runs in flight: retire every completed one (oldest
    /// first), then `Busy` (runs still copying), `Idle` (nothing left in
    /// flight) or `Failed` (an error bit or a stalled oldest run: the
    /// channel reset, every run still in flight counted late). Called under
    /// the engine's critical section by every turn and by the completion
    /// interrupt.
    fn settle(&mut self, now_us: u64, slack_us: u32, beam: Option<(u32, &Ring)>) -> Settled {
        // The fault bits first, then clear: a run that completes after the
        // owner reads below raises `in_suc_eof` again and is seen next time.
        let faulted = self.nruns > 0 && Self::faulted();
        Self::clear_ints();
        while self.nruns > 0 {
            let run = self.runs[usize::from(self.run_head)];
            if !landed(self.run_last(&run)) {
                break;
            }
            self.retire(run, now_us, slack_us, beam);
        }
        if self.nruns == 0 {
            COPY_BUSY.store(false, Ordering::Release);
            return Settled::Idle;
        }
        let oldest = self.runs[usize::from(self.run_head)];
        let age = now_us.saturating_sub(oldest.t0.max(self.done_at));
        if faulted || age > DMA_STALL_US {
            self.fail_all();
            return Settled::Failed;
        }
        Settled::Busy
    }

    /// Credit one completed run (the oldest) and free its entries.
    fn retire(&mut self, run: Run, now_us: u64, slack_us: u32, beam: Option<(u32, &Ring)>) {
        let len = u32::from(run.len);
        let mut torn = 0u32;
        for j in 0..usize::from(run.len) {
            let e = (usize::from(run.first) + j) % QUEUE_SLOTS;
            let (slot, gen) = self.pending[e];
            SLOT_GEN[usize::from(slot)].store(gen, Ordering::Release);
            self.reclaim(e);
            // Landed with the beam already in (or past) the slot: the copy
            // overlapped the read — torn.
            if let Some((abs_dma, ring)) = beam {
                if ring.late(self.pending_abs[e], abs_dma) {
                    torn += 1;
                }
            }
        }
        if torn > 0 {
            RING_TORN.fetch_add(torn, Ordering::Relaxed);
        }
        let run_us = now_us.saturating_sub(run.t0) as u32;
        RING_RUN_US_MAX.fetch_max(run_us, Ordering::Relaxed);
        RING_RUN_US_WIN.sample(now_us, run_us);
        // Its copy began when it was handed over or when the run ahead of
        // it finished, whichever was later.
        let per_slot = (now_us.saturating_sub(run.t0.max(self.done_at)) / u64::from(len.max(1))) as u32;
        // Clipped like the CPU copy's estimate: at most double per sample,
        // the first bounded by the slack (a poll that found it done ran
        // behind the completion by up to the poll interval, so the number
        // is an upper bound until the interrupt reports one).
        let clipped = if self.slot_us == 0 { per_slot.min(slack_us.max(1)) } else { per_slot.min(self.slot_us * 2) };
        self.slot_us = if self.slot_us == 0 { clipped.max(1) } else { ((self.slot_us * 7 + clipped) / 8).max(1) };
        RING_DMA_US.store(self.slot_us, Ordering::Relaxed);
        RING_DMA_US_MAX.fetch_max(per_slot, Ordering::Relaxed);
        RING_DMA_US_WIN.sample(now_us, per_slot);
        RING_DMA_BATCHES.fetch_add(1, Ordering::Relaxed);
        if run.by_isr {
            RING_DMA_ISR.fetch_add(1, Ordering::Relaxed);
        }
        let ctr = if run.core == 0 { &RING_PACKED_CORE0 } else { &RING_PACKED_CORE1 };
        ctr.fetch_add(len, Ordering::Relaxed);
        self.done_at = now_us;
        self.head = ((usize::from(self.head) + usize::from(run.len)) % QUEUE_SLOTS) as u8;
        self.inflight -= run.len;
        self.run_head = ((usize::from(self.run_head) + 1) % MAX_RUNS) as u8;
        self.nruns -= 1;
    }

    /// Reset the channel and drop every run in flight: their claims count
    /// late, their slots' contents are unknown (never reused), and their
    /// entries go back to the CPU so the next run starts on a clean stop.
    fn fail_all(&mut self) {
        Self::reset_fsm();
        RING_DMA_ERRORS.fetch_add(1, Ordering::Relaxed);
        RING_LATE.fetch_add(u32::from(self.inflight), Ordering::Relaxed);
        for j in 0..usize::from(self.inflight) {
            let e = (usize::from(self.head) + j) % QUEUE_SLOTS;
            SLOT_GEN[usize::from(self.pending[e].0)].store(SLOT_GEN_NONE, Ordering::Release);
            self.reclaim(e);
        }
        self.head = self.tail;
        self.inflight = 0;
        self.nruns = 0;
        COPY_BUSY.store(false, Ordering::Release);
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
    /// A full-frame ring (`n == rows`): slot `r` always carries row pair
    /// `r`, so a claim whose slot already holds that row of that packed
    /// frame is advanced without a copy (#896).
    full: bool,
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
/// the frame index of the pass being claimed in its top two bits. CAS-claimed.
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
        // The copy engine's descriptor rings: `QUEUE_SLOTS` slots per side (#897),
        // a slot in as many ≤ 4095-byte chunks of whole slot rows as it
        // takes (1 at 64 and 128 columns, 2 at 256, 3 at 512).
        let slot_rows = ring::slot_rows(g.planes);
        let chunk_rows = slot_rows.min(4095 / row_bytes.max(1)).max(1);
        let per_slot = slot_rows.div_ceil(chunk_rows);
        let pool = QUEUE_SLOTS * per_slot;
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
            "hub75-ring: {}x{} tiles of {}x{} from {} {}{} rot {}/{}, framebuffer {}x{} scan 1/{}, remap {}, est {} Hz",
            m.cols,
            m.rows,
            m.pw,
            m.ph,
            m.start.as_str(),
            m.dir.as_str(),
            if m.snake { " snake" } else { "" },
            u32::from(m.rot[0]) * 90,
            u32::from(m.rot[1]) * 90,
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
            runs: [Run::EMPTY; MAX_RUNS],
            run_head: 0,
            nruns: 0,
            head: 0,
            tail: 0,
            inflight: 0,
            done_at: 0,
            slot_us: 0,
            tx: copy_block.ptr.cast::<DmaDescriptor>(),
            // SAFETY: the block holds `2 * pool` descriptors.
            rx: unsafe { copy_block.ptr.cast::<DmaDescriptor>().add(pool) },
            per_slot,
            chunk_rows,
            row_bytes,
            slot_rows,
            pending: [(0, SLOT_GEN_NONE); QUEUE_SLOTS],
            pending_abs: [0; QUEUE_SLOTS],
        };
        engine.link_rings();
        Engine::configure();
        // Three probes over the same `cal` row pairs of frame 0 into slots
        // 0.. (identical dark content; the LCD DMA is not running yet):
        // one fresh run — the calibration, which proves the copy and seeds
        // the skip rule — then a one-slot run with the rest APPENDED while
        // it copies, and a run appended to a chain that has already drained
        // (the restart from the stop). The append probes failing leaves the
        // `append` lever off: one run at a time, step 2's shape.
        let cal = (n as usize).min(BATCH_SLOTS);
        let probe = |engine: &mut Engine, runs: &[usize], wait_between: bool| -> Option<u64> {
            let t0 = now_us();
            let mut row = 0usize;
            for (i, &len) in runs.iter().enumerate() {
                if i > 0 && wait_between {
                    // Let the chain drain WITHOUT settling it, so the next
                    // launch is an append onto a DMA sitting on the stop.
                    let last = engine.runs[(usize::from(engine.run_head) + usize::from(engine.nruns) - 1) % MAX_RUNS];
                    while !landed(engine.run_last(&last)) {
                        if now_us().saturating_sub(t0) > 5_000 {
                            engine.fail_all();
                            return None;
                        }
                    }
                }
                for j in 0..len {
                    let e = (usize::from(engine.tail) + j) % QUEUE_SLOTS;
                    let r = (row + j) % cal;
                    // SAFETY: row pair `r` of frame 0 and slot `r` are in
                    // bounds (`cal ≤ n ≤ rows`), and nothing else reads or
                    // writes either yet; entry `e` is free.
                    unsafe {
                        engine.stage(e, packed_ptr[0].add(r * slot_words).cast::<u8>().cast_const(), slots.add(r * slot_words).cast::<u8>());
                    }
                    engine.pending[e] = (r as u8, SLOT_GEN_NONE);
                }
                row += len;
                engine.launch(len, 0, false, now_us());
            }
            loop {
                match engine.settle(now_us(), 5_000, None) {
                    Settled::Idle => return Some(now_us().saturating_sub(t0)),
                    Settled::Failed => return None,
                    Settled::Busy if now_us().saturating_sub(t0) > 5_000 => {
                        engine.fail_all();
                        return None;
                    }
                    Settled::Busy => {}
                }
            }
        };
        let cal_run = probe(&mut engine, &[cal], false);
        let dma_ok = cal_run.is_some();
        let cal_us = cal_run.unwrap_or(5_001);
        let append_ok = dma_ok
            && cal >= 2
            && probe(&mut engine, &[1, cal - 1], false).is_some()
            && probe(&mut engine, &[1, cal - 1], true).is_some();
        Engine::reset_fsm();
        // The probes' own accounting is not the ring's.
        for c in [&RING_DMA_BATCHES, &RING_PACKED_CORE0, &RING_APPENDS, &RING_DMA_RESTARTS, &RING_DMA_US_MAX, &RING_LATE, &RING_DMA_ERRORS] {
            c.store(0, Ordering::Relaxed);
        }
        for g in SLOT_GEN.iter() {
            g.store(SLOT_GEN_NONE, Ordering::Relaxed);
        }
        if dma_ok {
            engine.slot_us = ((cal_us / cal as u64) as u32).max(1);
            RING_DMA_US.store(engine.slot_us, Ordering::Relaxed);
            RING_DMA_CAL_US.store(engine.slot_us, Ordering::Relaxed);
            DMA_ON.store(true, Ordering::Relaxed);
            APPEND.store(append_ok, Ordering::Relaxed);
            println!(
                "hub75-ring: GDMA ch{} copies the refill — {} slots of {} B in {} us ({} us a slot, {} MB/s), {} descriptors a slot, 64 B bursts, append {}",
                COPY_CH,
                cal,
                slot_words * 2,
                cal_us,
                engine.slot_us,
                (cal as u64 * (slot_words * 2) as u64) / cal_us.max(1),
                per_slot,
                if append_ok { "on" } else { "OFF (the append probe failed — one run in flight at a time)" },
            );
        } else {
            DMA_ON.store(false, Ordering::Relaxed);
            println!(
                "hub75-ring: GDMA ch{} calibration copy {} — the CPU copies the refill",
                COPY_CH,
                if cal_us > 5_000 { "timed out or failed" } else { "failed" },
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
                let desc_ptr_leaked = desc_block.leak();
                let _ = tables_block.leak();
                let _ = copy_block.leak();
                // The flash-write blank (#852): one dark slot (formatted for
                // row 0, colour bits zero) and, per chain descriptor, its
                // live buffer and the dark stand-in at the same offset.
                let dark_layout = core::alloc::Layout::from_size_align(slot_words * 2, 16).ok();
                match dark_layout.and_then(Block::zeroed) {
                    Some(dark_block) => {
                        // SAFETY: a fresh zeroed block of one slot, ours.
                        let dark = unsafe { core::slice::from_raw_parts_mut(dark_block.ptr.cast::<u16>(), slot_words) };
                        ring::format_slot_for(dark, g, c, &s, 0);
                        let dark_base = dark_block.leak();
                        let slots_base = slots.cast::<u8>() as usize;
                        let slot_bytes = slot_words * 2;
                        let descs_ptr = desc_ptr_leaked.cast::<DmaDescriptor>();
                        let mut live: Vec<*mut u8> = Vec::with_capacity(descs);
                        let mut darkp: Vec<*mut u8> = Vec::with_capacity(descs);
                        for i in 0..descs {
                            // SAFETY: `descs` descriptors were written by the
                            // chain builder; `buffer` points inside the slots.
                            let buf = unsafe { core::ptr::addr_of!((*descs_ptr.add(i)).buffer).read_volatile() };
                            let off = (buf as usize).wrapping_sub(slots_base) % slot_bytes;
                            live.push(buf);
                            // SAFETY: `off < slot_bytes`, inside the dark slot.
                            darkp.push(unsafe { dark_base.add(off) });
                        }
                        let (live, darkp) = (live.leak(), darkp.leak());
                        // SAFETY: the single write, before the release below.
                        unsafe {
                            *BLANK.0.get() =
                                Some(Blank { descs: descs_ptr, n: descs, live: live.as_ptr(), dark: darkp.as_ptr() })
                        };
                        BLANK_READY.store(true, Ordering::Release);
                    }
                    None => println!("hub75-ring: no heap for the dark slot — flash writes show the ring's resident rows"),
                }
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
///
/// Two reserves (#896, 2026-09-30): a FULL-FRAME ring (`asked == rows`)
/// may dip into [`RING_HEAP_RESERVE_FULL`], because at `n == rows` the
/// copy is skipped while the frame is unchanged and the ring earns its
/// slots back in bus time and `late` (the 2x1 at `lsb 7`: 6.9 % → 0.05 %);
/// any smaller ring keeps [`RING_HEAP_RESERVE`], since three more rotating
/// slots buy nothing a 4x1 at `lsb 40` can see (measured: 15 slots instead
/// of 12 for 15 KB of runtime heap and the same 0 late). A full-frame ask
/// the lower reserve still cannot complete falls back to the ordinary cap.
pub(crate) fn capped_slots(asked: u32, s: &Schedule, g: Geometry) -> Option<u32> {
    let heap0 = crate::hub75::heap_before_panel();
    if heap0 == 0 {
        return Some(asked);
    }
    let per_slot = ring::slot_bytes(g.planes, g.cols)
        + ring::descs_per_slot(s, g.cols, esp_hub75::max_dma_chunk_size()) * core::mem::size_of::<DmaDescriptor>();
    let cap_for = |reserve: usize| {
        let budget = heap0.saturating_sub(BOOT_HEAP_FLOOR + reserve + RING_FIXED_BYTES);
        (budget / per_slot.max(1)) as u32
    };
    let rows = g.rows as u32;
    if asked == rows && rows > ring::GUARD_SLOTS && cap_for(RING_HEAP_RESERVE_FULL) >= rows {
        return Some(rows);
    }
    let n = asked.min(cap_for(RING_HEAP_RESERVE));
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
            full: self.ring.n == self.ring.rows,
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
/// the runs in flight, then claim up to a run's worth of fillable slots
/// whose deadlines the copy engine can meet — counting the slots still
/// queued ahead of them (#897) — and hand them over as one run: started
/// fresh when the DMA is idle, appended to the chain in flight otherwise
/// (the `append` lever; off, a busy engine ends the turn as in step 2).
/// `by_isr` marks a turn taken by the completion interrupt. The claims are
/// credited to `core` when their run completes.
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
        let beam = abs_dma(s);
        let busy = e.settle(now_us(), slack_us, beam.map(|b| (b, &s.ring))) == Settled::Busy;
        // Nothing is handed to the DMA while a flash fence is open: the
        // fence waits for the chain to drain (`copy_busy`) and must see it
        // stay drained. (The fence parks the other core and masks this
        // one's interrupts, so no turn runs then anyway; this says so.)
        if IN_FENCE.load(Ordering::Acquire) != 0 {
            return 0;
        }
        let room = if busy && !APPEND.load(Ordering::Relaxed) { 0 } else { e.room() };
        if room == 0 {
            if core == 0 && !by_isr && HYBRID.load(Ordering::Relaxed) {
                // The DMA cannot take more: the output task copies the
                // next claims itself meanwhile (the lock is held for the
                // copies — core 1's turns are DMA kicks and find it busy
                // for at most a few slots' worth).
                return drain_cpu(p, s, 0);
            }
            return 0;
        }
        let Some(abs_dma) = beam else { return 0 };
        let queued = u32::from(e.inflight);
        // The `minfill` lever: a short run may wait for a later turn while
        // the queue head's slack covers the wait.
        let minfill = MINFILL.load(Ordering::Relaxed).min(room as u32);
        if minfill > 1 {
            let head = Claim::decode(NEXT_FILL.load(Ordering::Acquire)).abs;
            if !s.ring.late(head, abs_dma) {
                let poll_us = (slack_us / 4).max(500);
                let avail = s.ring.fillable_ahead(head, abs_dma);
                if ring::defer_run(avail, minfill, left_us(s, head, abs_dma), DMA_MARGIN_US, queued, e.slot_us, poll_us) {
                    RING_DEFERRED.fetch_add(1, Ordering::Relaxed);
                    return 0;
                }
            }
        }
        let first = usize::from(e.tail);
        let mut k = 0usize;
        while k < room {
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
            let slot = s.ring.slot(claim.abs) as usize;
            let gen = FRAME_GEN[usize::from(claim.buf) % FRAMES].load(Ordering::Acquire);
            if s.full && SLOT_GEN[slot].load(Ordering::Acquire) == gen {
                // The slot already holds this row pair of this frame (#896):
                // advance the claim, copy nothing, no deadline to meet.
                if NEXT_FILL.compare_exchange(word, next, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                    if s.ring.row(claim.abs) == 0 {
                        PASS_FRAME[(s.ring.pass(claim.abs) % 2) as usize].store(claim.buf, Ordering::Release);
                    }
                    RING_REUSED.fetch_add(1, Ordering::Relaxed);
                }
                continue;
            }
            // The skip rule (design §7) for a run: this claim's slot is
            // written when the DMA has done the `queued` slots still in
            // flight ahead of it, the `k` before it in this run, and this
            // one — plus the turn's own latency (`ring::dma_need_us`).
            let need_us = ring::dma_need_us(DMA_MARGIN_US, queued + k as u32 + 1, e.slot_us);
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
            let row = s.ring.row(claim.abs) as usize;
            let entry = (first + k) % QUEUE_SLOTS;
            // SAFETY: the claim is ours by the CAS; the DMA has left the
            // slot (`fillable`); the packed frame a pass reads is kept out
            // of `write_frame`'s choice while the pass is in flight; the
            // entry is free (`room`).
            unsafe {
                e.stage(
                    entry,
                    s.packed[usize::from(claim.buf) % FRAMES].add(row * s.slot_words).cast::<u8>(),
                    s.slots.add(slot * s.slot_words).cast::<u8>(),
                );
            }
            // Stamped into `SLOT_GEN` when the run completes; the slot is
            // half-written until then and a full-frame ring must not reuse it.
            e.pending[entry] = (slot as u8, if s.full { gen } else { SLOT_GEN_NONE });
            e.pending_abs[entry] = claim.abs;
            SLOT_GEN[slot].store(SLOT_GEN_NONE, Ordering::Release);
            k += 1;
        }
        if k == 0 {
            return 0;
        }
        e.launch(k, core, by_isr, now_us());
        k as u32
    })
}

/// The copy channel's interrupt (`in_suc_eof` — once per run — and the
/// fault bits, bound by [`arm_steal`] on the AppCpu): retire the runs that
/// finished and hand the DMA the next one at once, so the chain has work
/// while the queue has claims. This chaining runs whatever the `steal`
/// lever says — that lever is the core-1 TIMER's turns; the completion
/// interrupt costs this core a few microseconds per run and only when one
/// ends, so "steal off" measures the chain alone and "steal on" the chain
/// plus the timer. (`dma` off leaves the handler to the accounting only.)
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
                let _ = e.settle(now_us(), RING_SLACK_US.load(Ordering::Relaxed), abs_dma(s).map(|b| (b, &s.ring)));
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
        let slot = s.ring.slot(claim.abs) as usize;
        let gen = FRAME_GEN[usize::from(claim.buf) % FRAMES].load(Ordering::Acquire);
        if s.full && SLOT_GEN[slot].load(Ordering::Acquire) == gen {
            // The slot already holds this row pair of this frame (#896):
            // advance the claim and copy nothing.
            if NEXT_FILL.compare_exchange(word, next, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                if s.ring.row(claim.abs) == 0 {
                    PASS_FRAME[(s.ring.pass(claim.abs) % 2) as usize].store(claim.buf, Ordering::Release);
                }
                RING_REUSED.fetch_add(1, Ordering::Relaxed);
            }
            continue;
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
        if s.full {
            // After the copy landed, never before: a copy this core is
            // preempted in must not read as done to the other core.
            SLOT_GEN[slot].store(gen, Ordering::Release);
        }
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
    STEAL_PERIOD_US.store(us as u32, Ordering::Relaxed);
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

/// The steal timer's period, and when it last fired (low 32 bits of the
/// systimer): the interval's excess over the period is core 1's interrupt
/// latency, the worst ever and the worst of the last second or so —
/// `steal_lat_max` / `steal_lat_sec`. The copy engine's completion is
/// observed from this core's interrupts, so this bounds how late a landing
/// (and a `torn` verdict) can be seen.
static STEAL_PERIOD_US: AtomicU32 = AtomicU32::new(0);
static STEAL_LAST_US: AtomicU32 = AtomicU32::new(0);
pub static RING_STEAL_LAT_MAX: AtomicU32 = AtomicU32::new(0);
pub static RING_STEAL_LAT_WIN: WindowMax = WindowMax::new();

extern "C" fn steal_isr() {
    STEAL_TIMER.lock(|c| {
        if let Some(t) = c.borrow_mut().as_mut() {
            t.clear_interrupt();
        }
    });
    let now = now_us();
    let last = STEAL_LAST_US.swap(now as u32, Ordering::Relaxed);
    if last != 0 {
        let lat = (now as u32).wrapping_sub(last).saturating_sub(STEAL_PERIOD_US.load(Ordering::Relaxed));
        RING_STEAL_LAT_MAX.fetch_max(lat, Ordering::Relaxed);
        RING_STEAL_LAT_WIN.sample(now, lat);
    }
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

/// Is a copy run still reading PSRAM? The flash fence (`core1.rs`) asks
/// before every flash op and waits until this is false: a GDMA burst that
/// is stalled on a PSRAM read while SPI1 programs flash sits on the
/// engine's shared read path and holds off the panel chain's own reads on
/// channel 0 for the whole op — the LCD then underflows (2026-09-29, the
/// 2x1 `lsb 7` chain stopped ~60 s after boot, at the boot-ok store).
/// With runs appended to the chain (#897) "busy" is ANY run in flight: the
/// newest run's last receive descriptor has not landed ([`COPY_TAIL`] —
/// runs complete in order, so that one landing means all have). Nothing
/// can append a run during the op (the other core is parked, the fencing
/// core's interrupts are masked, and `drain_dma` hands nothing over while
/// `IN_FENCE` is open), so waiting out the chain in flight is the whole
/// rule — at most [`QUEUE_SLOTS`] slots of copy. One descriptor read; no
/// lock.
pub fn copy_busy() -> bool {
    if !COPY_BUSY.load(Ordering::Acquire) {
        return false;
    }
    let tail = COPY_TAIL.load(Ordering::Acquire);
    !tail.is_null() && !landed(tail)
}

/// The `append` lever (`POST /api/ring {"append":…}`, #897). Off takes
/// effect at the next turn: the chain in flight drains, and nothing more is
/// appended to it. On is honoured even where the boot's append probe
/// failed — an experiment; a chain that does not resume shows as
/// `dma_errors` (the 20 ms stall reset) and costs `late`, never garbage.
pub fn set_append(on: bool) {
    APPEND.store(on, Ordering::Relaxed);
}

/// The `minfill` lever (`POST /api/ring {"minfill":N}`, #897): clamped to
/// 1 (off) ..= [`BATCH_SLOTS`].
pub fn set_minfill(n: u32) {
    MINFILL.store(n.clamp(1, BATCH_SLOTS as u32), Ordering::Relaxed);
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
        // A fresh generation on the frame BEFORE it is newest: a claimer that
        // sees the index sees the stamp it will compare its slot's against.
        let gen = GEN.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        FRAME_GEN[usize::from(i)].store(if gen == SLOT_GEN_NONE { 0 } else { gen }, Ordering::Release);
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
            blank_tick();
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
