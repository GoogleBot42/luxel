//! State shared between the web server and the render task. The engine is
//! owned exclusively by the render task: writes flow in through a message
//! queue, reads come from JSON snapshots the render task publishes.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::channel::Channel;
use luxel_core::fixed::Fx;

/// Writes from HTTP handlers to the engine. Patterns cross as the RAW LXP1
/// envelope buffer (name + source + bytecode), decode-validated by the
/// sender but parsed only by the render task — the producer side must not
/// allocate source/blob copies: on a heap dominated by the *running*
/// pattern those copies OOM'd (soak v5), while the render task frees the
/// outgoing engine before it decodes, so peak memory lands where the most
/// is free.
pub enum Msg {
    /// Run this pattern now. `id` is the library id ("" = ad-hoc push) and
    /// travels IN the message so the render task stamps identity and
    /// read-back location atomically with the swap — senders setting the id
    /// separately raced the queue (the playlist task set it after send, so
    /// a fast render task could bind the PREVIOUS item's id to the new
    /// pattern's read-back).
    Code { env: Vec<u8>, id: String },
    /// Drop the running engine to free its heap (strip freezes on the last
    /// frame). Sent before an OTA (a reboot follows anyway) and when a
    /// pattern upload can't allocate its buffer — the next Code revives
    /// rendering.
    Freeze,
    Control(String, Vec<Fx>),
    Var(String, Fx),
    /// New pixel count — the render task rebuilds the engine + SPI buffer live.
    Config(u32),
    /// New LED protocol — the render task reconfigures SPI + resizes the buffer
    /// live (0 = SK9822, 1 = WS2812; see leds::Protocol::from_u8).
    Protocol(u8),
    /// Like Code, but crossfade from the current pattern over `ms` (playlist
    /// transitions): the render task keeps the outgoing engine and blends.
    Crossfade { env: Vec<u8>, ms: u32, id: String },
    /// Run a LIBRARY pattern by id, crossfading over `ms` (0 = cut). No
    /// envelope travels: the render task decodes straight from the
    /// pattern's mapped arena slot (patterns::code_of — no blob Vec, no
    /// source Vec, no envelope Vec) or, without a slot, from a transient
    /// chunk-store read. Identity/read-back come from patterns::source_stat.
    Library { id: String, ms: u32 },
}

pub static MSG_QUEUE: Channel<CriticalSectionRawMutex, Msg, 8> = Channel::new();

/// Frames rendered in the last full second, updated by the render task.
pub static FPS: AtomicU32 = AtomicU32::new(0);

/// Per-stage frame timing: average microseconds per rendered frame over the
/// last full second (Gitea #260 — profiling groundwork). Written by the render
/// task on the same once-a-second tick as [FPS], read back by `/api/status`.
///
/// Only PATTERN frames are timed: the live-input (DDP/E1.31) path and idle
/// loops with no engine are skipped, so the divisor is the number of *timed*
/// frames in the window, not [FPS]. A window with no pattern frames stores 0.
///
/// The stages nest: `FRAME_US` ≈ `VM_US` + `PIPE_US` + `OUT_US`, plus the small
/// per-frame bookkeeping between them (delta math, crossfade blend, vmerr).
///
/// Whole engine branch: the delta math through `write_frame` returning.
pub static FRAME_US: AtomicU32 = AtomicU32::new(0);
/// `Engine::frame` only — the VM evaluating the pattern (plus the outgoing
/// engine's frame and the blend while a crossfade is running).
pub static VM_US: AtomicU32 = AtomicU32::new(0);
/// Preview copy (`set_pixels`) + `apply_outpipe` (gamma / palette / blur).
pub static PIPE_US: AtomicU32 = AtomicU32::new(0);
/// `BoardOutput::write_frame` only — the LED / HUB75 driver.
pub static OUT_US: AtomicU32 = AtomicU32::new(0);

/// Frames handed to the output driver in the last full second, on boards
/// that pipeline the output stage onto the other core (pipeline.rs, Gitea
/// #306). [`FPS`] counts frames the VM *rendered*; when compose is the
/// slower half the render task free-runs ahead and its surplus frames are
/// dropped, so the two differ and both are meaningful.
///
/// **Frames the driver actually took**, on every board (Gitea #378 —
/// `OutputDriver::write_frame` returns whether it wrote, and a panel whose
/// previous buffer swap has not landed says no). On a strip that is frames
/// on the wire; on the panel it is frames the panel scanned out. It used to
/// count every `write_frame` CALL, which on the panel made it a compose rate
/// reading up to 125 against a 115 Hz rescan.
///
/// With vsync pacing (Gitea #387) this and [`FPS`] should be the same
/// number, both a hair under [`RESCAN_HZ`]: the render loop is paced by the
/// panel, so a composed frame is never thrown away.
///
/// Always 0 on a non-pipelined board, where every rendered frame is written
/// by construction.
pub static OUT_FPS: AtomicU32 = AtomicU32::new(0);

/// The HUB75 panel's own rescan rate in Hz — how many times a second the
/// panel is actually redrawn from the framebuffer, from esp-hub75's BCM
/// frame counter. 0 on every board without a HUB75 panel.
///
/// The panel's clock, and since Gitea #387 the render loop's: the output
/// task holds each frame until the previous swap has landed, so exactly one
/// frame is composed, swapped and displayed per rescan and [`FPS`] and
/// [`OUT_FPS`] both settle just under this number. It stays the CEILING on
/// what can be displayed — a compose that overruns its rescan window shows
/// the previous frame for one more rescan, which is why `OUT_FPS` reads a
/// few below rather than exactly equal. Measured 115 Hz on the 64x64 bench
/// panel at 7 planes / 30 MHz.
///
/// Sampled where the frames are: the counter is read in `write_frame`, so
/// it stops advancing when nothing renders — exactly when `OUT_FPS` is 0
/// too.
pub static RESCAN_HZ: AtomicU32 = AtomicU32::new(0);

/// Rendered frames the fixture never showed, cumulative since boot —
/// `/api/status` `dropped`.
///
/// Written by the output task on a pipelined board (pipeline.rs), which
/// derives it from the gap between the sequence numbers of consecutive
/// DISPLAYED frames rather than by counting known loss paths — so it covers
/// every way a frame can go missing between the VM and the panel, including
/// ones the code does not enumerate. Always 0 on a non-pipelined board, where
/// every rendered frame is written by construction.
///
/// Under vsync pacing (Gitea #387) the pipeline is lossless and this must
/// stay 0. A nonzero value is the ground truth that separates a real dropped
/// frame from a camera that missed one.
pub static DROPPED: AtomicU32 = AtomicU32::new(0);

/// HUB75 swap diagnostics from the patched esp-hub75 (Gitea #387), absolute
/// since boot. `SWAP_EOF_RACE` counts swaps armed while an `out_eof` was
/// raised but unserviced — the window the driver's pending-EOF check closes,
/// and before that check the rate at which a framebuffer was handed back
/// while the DMA was still scanning it out (a frame both torn and never
/// displayed, invisible to [`DROPPED`] because `write_frame` succeeded).
/// `SWAP_SLOW_PATH` counts swaps that took the two-EOF fallback for any
/// reason, each costing one extra panel frame of latency. Both 0 on boards
/// without a panel.
pub static SWAP_EOF_RACE: AtomicU32 = AtomicU32::new(0);
/// See [`SWAP_EOF_RACE`].
pub static SWAP_SLOW_PATH: AtomicU32 = AtomicU32::new(0);

/// HUB75 pass-length forensics (Gitea #395), from the patched esp-hub75.
///
/// A "pass" is one full traversal of a DMA descriptor ring. `suc_eof` sits on
/// a ring's last descriptor only, so consecutive EOFs are exactly one ring
/// apart and every pass must be the same length. [`PASS_SHORT`] counts passes
/// under 0.9x nominal — each one is the engine having entered a ring off its
/// head, which nothing in the driver should be able to cause. It is the
/// ground truth for a frame that vanishes with no frame accounting noticing:
/// `write_frame` succeeded, so [`DROPPED`] and `out_fps` see nothing wrong.
pub static PASS_COUNT: AtomicU32 = AtomicU32::new(0);
/// Shortest pass seen, microseconds. See [`PASS_COUNT`].
pub static PASS_MIN_US: AtomicU32 = AtomicU32::new(0);
/// Longest pass seen, microseconds. See [`PASS_COUNT`].
pub static PASS_MAX_US: AtomicU32 = AtomicU32::new(0);
/// Slow EWMA of pass length, the yardstick for short/long. See [`PASS_COUNT`].
pub static PASS_NOMINAL_US: AtomicU32 = AtomicU32::new(0);
/// Passes under 0.9x nominal — must be 0. See [`PASS_COUNT`].
pub static PASS_SHORT: AtomicU32 = AtomicU32::new(0);
/// Passes over 1.5x nominal; a coalesced/missed EOF looks like this.
pub static PASS_LONG: AtomicU32 = AtomicU32::new(0);
/// Worst frame-count ISR dispatch latency, in descriptors (~34 us each). The
/// pass timestamps are taken inside that ISR, so this is the noise floor on
/// [`PASS_MIN_US`]/[`PASS_MAX_US`] — and the reason the classified pass length
/// is corrected by it before anything is called short.
pub static PASS_ISR_LAT_MAX: AtomicU32 = AtomicU32::new(0);
/// Most rescans the panel ran between two consecutive DISPLAYED frames. 1 =
/// every rescan showed something new, 2 = one repeat, higher = the compose is
/// overrunning its window badly.
pub static PASS_PER_FRAME_MAX: AtomicU32 = AtomicU32::new(0);
/// FEWEST rescans between two consecutive displayed frames, and the sharpest
/// test there is for the artefact Jeremy filmed. A frame handed to the DMA in
/// the same rescan as the one before it was **never scanned out** — the panel
/// skips it, while `write_frame` succeeded and `dropped`/`out_fps` see a
/// perfectly healthy frame. So `per_frame_min == 0` IS the skip, directly
/// observed, and `>= 1` rules this mechanism out entirely.
pub static PASS_PER_FRAME_MIN: AtomicU32 = AtomicU32::new(u32::MAX);
/// Times `per_frame_min` was 0 — i.e. a displayed frame that the panel never
/// actually scanned out.
pub static PASS_ZERO_RESCAN: AtomicU32 = AtomicU32::new(0);

/// How many recent short passes [`pass_shorts`] keeps.
pub const PASS_SHORT_LOG: usize = 8;
static PASS_SHORT_US: [AtomicU32; PASS_SHORT_LOG] =
    [const { AtomicU32::new(0) }; PASS_SHORT_LOG];
static PASS_SHORT_FLAGS: [AtomicU32; PASS_SHORT_LOG] =
    [const { AtomicU32::new(0) }; PASS_SHORT_LOG];
static PASS_SHORT_FRAME: [AtomicU32; PASS_SHORT_LOG] =
    [const { AtomicU32::new(0) }; PASS_SHORT_LOG];
static PASS_SHORT_N: AtomicU32 = AtomicU32::new(0);

/// Mirror the driver's short-pass log so `/api/status` can read it without
/// touching the driver, which lives on the output task's core.
pub fn set_pass_shorts(n: u32, log: &[(u32, u32, u32); PASS_SHORT_LOG]) {
    for (k, &(us, flags, frame)) in log.iter().enumerate() {
        PASS_SHORT_US[k].store(us, Ordering::Relaxed);
        PASS_SHORT_FLAGS[k].store(flags, Ordering::Relaxed);
        PASS_SHORT_FRAME[k].store(frame, Ordering::Relaxed);
    }
    PASS_SHORT_N.store(n, Ordering::Relaxed);
}

/// `(total short passes ever, ring of the last few as (us, flags, frame))`.
/// Flags are the driver's `pass_flag` bits: 1 armed-in-pass, 2 previous EOF
/// restored a tail, 4 fast landing, 8 an eof_race within two passes, 16 a
/// flip was armed and unlanded at that EOF.
pub fn pass_shorts() -> (u32, [(u32, u32, u32); PASS_SHORT_LOG]) {
    let mut out = [(0u32, 0u32, 0u32); PASS_SHORT_LOG];
    for (k, slot) in out.iter_mut().enumerate() {
        *slot = (
            PASS_SHORT_US[k].load(Ordering::Relaxed),
            PASS_SHORT_FLAGS[k].load(Ordering::Relaxed),
            PASS_SHORT_FRAME[k].load(Ordering::Relaxed),
        );
    }
    (PASS_SHORT_N.load(Ordering::Relaxed), out)
}

/// Displayed-frame ground truth (Gitea #395), the firmware-side equivalent of
/// reading sweep columns off a video.
///
/// The driver logs which FRAMEBUFFER each panel pass actually scanned out; the
/// panel driver maps those pointers back to the frame sequence numbers it
/// composed into them. A sequence number that never appears in that log is a
/// frame the panel never displayed, even though `write_frame` succeeded and
/// `DROPPED`/`out_fps` saw nothing wrong. That is the artefact filmed on the
/// bench: one frame skipped, the next shown twice, frame count conserved.
pub static SHOWN_SKIPS: AtomicU32 = AtomicU32::new(0);
/// Frames the panel displayed for more than one consecutive pass.
pub static SHOWN_REPEATS: AtomicU32 = AtomicU32::new(0);
/// Passes audited so far.
pub static SHOWN_AUDITED: AtomicU32 = AtomicU32::new(0);
/// Descriptor index the last swap armed at, and the highest ever taken on the
/// fast path — if a skip is seen, the latter bounds the GDMA prefetch depth.
pub static SHOWN_ARM_IDX: AtomicU32 = AtomicU32::new(0);
/// See [`SHOWN_ARM_IDX`].
pub static SHOWN_ARM_IDX_MAX: AtomicU32 = AtomicU32::new(0);
/// Arm index of the swap most recently implicated in a skip, 0 if none.
pub static SHOWN_SKIP_ARM_IDX: AtomicU32 = AtomicU32::new(0);
/// Landings taken while the engine was provably still looping the OLD ring
/// (Gitea #395) — a false landing, which hands the caller a buffer that is
/// still pending display and lets the ISR restore undo the flip. Must be 0.
pub static LANDING_MISMATCH: AtomicU32 = AtomicU32::new(0);
/// Swaps armed while a previous flip was still pending. Correct double
/// buffering cannot do this; a false landing can.
pub static DOUBLE_ARM: AtomicU32 = AtomicU32::new(0);

/// The last few DISPLAYED frame tags, oldest first — the firmware-side
/// version of reading sweep columns off a video. A healthy panel reads
/// `...,7,8,9,10,...`; the filmed artefact reads `...,2,4,4,5,...`, one frame
/// never shown and the next shown twice. Captured around a skip so the
/// sequence can be inspected instead of inferred from a counter.
pub const TAG_LOG: usize = 24;
static TAGS: [AtomicU32; TAG_LOG] = [const { AtomicU32::new(0) }; TAG_LOG];
static TAGS_N: AtomicU32 = AtomicU32::new(0);
/// Frozen copy of the tag sequence around the FIRST skip seen, so it survives
/// however long it takes anyone to look.
static SKIP_TAGS: [AtomicU32; TAG_LOG] = [const { AtomicU32::new(0) }; TAG_LOG];
static SKIP_TAGS_SET: AtomicBool = AtomicBool::new(false);

/// Record one displayed tag.
pub fn push_tag(tag: u32) {
    let i = TAGS_N.fetch_add(1, Ordering::Relaxed) as usize % TAG_LOG;
    TAGS[i].store(tag, Ordering::Relaxed);
}

/// Freeze the current tag window the first time a skip is seen.
pub fn freeze_skip_tags() {
    if SKIP_TAGS_SET.swap(true, Ordering::Relaxed) {
        return;
    }
    for k in 0..TAG_LOG {
        SKIP_TAGS[k].store(TAGS[k].load(Ordering::Relaxed), Ordering::Relaxed);
    }
    SKIP_TAGS[0].store(TAGS_N.load(Ordering::Relaxed), Ordering::Relaxed);
}

/// `(total, live window oldest-first, frozen-at-first-skip window)`.
pub fn tag_log() -> (u32, [u32; TAG_LOG], [u32; TAG_LOG]) {
    let n = TAGS_N.load(Ordering::Relaxed);
    let mut live = [0u32; TAG_LOG];
    let mut frozen = [0u32; TAG_LOG];
    for k in 0..TAG_LOG {
        live[k] = TAGS[k].load(Ordering::Relaxed);
        frozen[k] = SKIP_TAGS[k].load(Ordering::Relaxed);
    }
    (n, live, frozen)
}

/// Displayed passes the audit could not see because the driver's log lapped
/// (the panel emits EOFs slightly faster than frames are composed). These are
/// NOT skips, and keeping them separate is the difference between a counter
/// that measures the panel and one that measures its own lag.
pub static SHOWN_LAPSED: AtomicU32 = AtomicU32::new(0);

/// Raw BCM frame count from the panel driver, absolute since boot.
/// [`RESCAN_HZ`] is its once-a-second delta; nothing else should read it.
pub static RESCANS: AtomicU32 = AtomicU32::new(0);

/// Heap the CURRENTLY loaded pattern's engine occupies, in bytes — measured
/// at load time as free-heap-before minus free-heap-after, with no engine
/// resident on either side of the subtraction. 0 = nothing loaded, or the
/// last load couldn't be measured.
///
/// Reported as `/api/status` `engine_heap` purely so the playground can
/// predict the NEXT load correctly. The render task drops the outgoing
/// engine before it decodes the incoming program, so a swap is measured
/// against `heap_free + engine_heap`, not `heap_free` — see
/// `luxel_core::budget::load_base` (Gitea #287). Without it the editor
/// charged every incoming pattern for the resident one and warned about
/// patterns that load fine.
pub static ENGINE_HEAP: AtomicU32 = AtomicU32::new(0);

/// Largest single allocation the heap can satisfy right now, in bytes.
///
/// `heap_free` is a SUM over the free list — it says nothing about whether
/// one contiguous run of that size exists, and that gap is exactly what
/// Gitea #390 walked into: a 30 KB pattern upload refused with 74 KB free,
/// cleared by a reboot. Every 30–45 KB envelope alloc/free cycle chips away
/// at the longest free run until the upload's `try_reserve_exact` can no
/// longer be met, while `heap_free` barely moves.
///
/// **esp-alloc 0.10.0 has no API for this.** `HEAP.free()`, `HEAP.used()`
/// and `HeapStats { region_stats: [RegionStats { size, used, free }; 3],
/// size, current_usage }` are all sums, and neither backend behind them
/// surfaces a largest-run figure either (the pinned rev's
/// `esp-alloc/src/lib.rs` and `esp-alloc/src/heap/{llff,tlsf}.rs` — LLFF
/// wraps `linked_list_allocator::Heap`, TLSF wraps `rlsf::Tlsf`, and only
/// `size`/`used`/`free` are re-exported from either). So measure it the one
/// way that is also the definition callers care about: binary-search the
/// largest block the global allocator actually hands back, freeing each
/// probe immediately.
///
/// Notes on cost and safety:
/// - ~8 probes (the bracket stops at [`PROBE_RESOLUTION`]); each is one
///   allocator pass, so this is microseconds, not milliseconds.
/// - `GlobalAlloc::alloc` returns null on failure instead of panicking
///   (unlike `Vec`, which routes through `handle_alloc_error`), so a failed
///   probe costs nothing.
/// - An alloc immediately followed by a `dealloc` of the same layout leaves
///   both backends' free lists exactly as they were — probing does not
///   itself fragment the heap.
/// - A successful probe momentarily HOLDS the block it found, so the search
///   is bounded twice over. Each probe runs inside a critical section, which
///   keeps an interrupt on this core from allocating into a heap the probe
///   has emptied (esp-radio's mallocs are the ones that don't null-check);
///   and the search never probes above `heap_free - `[`PROBE_RESERVE`], so
///   even the other core — whose allocations a critical section here does
///   not exclude — always has that much left. The reserve is why the result
///   saturates: `heap_largest == heap_free - PROBE_RESERVE` means "as
///   contiguous as this can report", i.e. not fragmented.
pub fn largest_free_block() -> usize {
    let free = esp_alloc::HEAP.free();
    let ceiling = free.saturating_sub(PROBE_RESERVE);
    // The common, unfragmented case: the whole free heap is one run.
    if probe_alloc(ceiling) {
        return ceiling;
    }
    let mut lo = 0usize; // known to fit
    let mut hi = ceiling; // known not to
    while hi - lo > PROBE_RESOLUTION {
        let mid = lo + (hi - lo) / 2;
        if probe_alloc(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Bracket width at which [`largest_free_block`] stops halving. The figure
/// is consumed in KB, so finer than this buys nothing and costs probes.
const PROBE_RESOLUTION: usize = 512;

/// Heap [`largest_free_block`] never takes, so a concurrent allocation on
/// the other core cannot be starved by the probe. Also the amount by which
/// the reported figure can under-read on a perfectly unfragmented heap —
/// irrelevant next to a 30-45 KB pattern envelope, which is what the number
/// exists to predict.
const PROBE_RESERVE: usize = 4 * 1024;

/// Can the allocator hand back `size` bytes in one piece? See
/// [`largest_free_block`] for why this is the only way to ask.
fn probe_alloc(size: usize) -> bool {
    if size == 0 {
        return true;
    }
    // Word alignment: every real allocation here is at least word-aligned,
    // so a probe at align 4 never over-reports what a `Vec` could get.
    let Ok(layout) = core::alloc::Layout::from_size_align(size, 4) else {
        return false;
    };
    critical_section::with(|_| {
        // SAFETY: non-zero layout; the pointer is freed with the same
        // layout before anything else can allocate, and never read.
        unsafe {
            let p = alloc::alloc::alloc(layout);
            if p.is_null() {
                return false;
            }
            alloc::alloc::dealloc(p, layout);
        }
        true
    })
}

/// Global output brightness, 0–31. The render task reads it every frame and
/// feeds it to the encoder (SK9822's 5-bit current field; a software scale for
/// WS2812). HTTP `/api/brightness` writes it; boot seeds it from flash (else
/// the compile-time default). Cheap enough to apply per-frame with no cost.
pub static BRIGHTNESS: AtomicU8 = AtomicU8::new(4);

/// Active pixel count. Runtime-configurable (`/api/config`): only the render
/// task writes it — on `Msg::Config` it rebuilds the engine + SPI buffer to
/// match, no reboot. Everyone else (status, compile-validation) reads it. Boot
/// seeds it from flash (else the board default).
pub static PIXEL_COUNT: AtomicU32 = AtomicU32::new(300);

/// Hard cap on a runtime pixel count, to bound heap use (engine buffers + the
/// SPI encode buffer). Per-board since Gitea #74 — 2048 on strip boards,
/// 4096 where a 64x64 HUB75 panel needs it — and defined next to the rest of
/// the board identity; re-exported here because every consumer (`/api/config`
/// validation, `/api/status`, the render task's clamp) reads it from `shared`.
pub use crate::board::MAX_PIXELS;

/// Active LED protocol as a code (0 = SK9822, 1 = WS2812; leds::Protocol
/// from_u8/as_u8). Runtime-configurable (`/api/protocol`): only the render task
/// writes it — on `Msg::Protocol` it reconfigures the SPI clock + resizes the
/// buffer, no reboot. Boot seeds it from flash (else the board default).
pub static PROTOCOL: AtomicU8 = AtomicU8::new(0);

/// The *requested* pixel count / protocol — what the persisted settings
/// record must reflect. PIXEL_COUNT/PROTOCOL above lag behind a POST until
/// the render task drains the message; persisting from those could clobber
/// a concurrent change with a stale value (e.g. POST /api/config then POST
/// /api/protocol before the render task ran → the protocol write persisted
/// the OLD pixel count). HTTP handlers store these when they enqueue; boot
/// seeds them alongside the applied values.
pub static WANT_PIXEL_COUNT: AtomicU32 = AtomicU32::new(300);
pub static WANT_PROTOCOL: AtomicU8 = AtomicU8::new(0);

/// The GPIO the strip's DATA line is actually on — bound once at boot when
/// main.rs builds the SPI driver (a stored override, else the board
/// default). Read by `/api/config` and by `gpio::pin_is_free`, which keeps
/// patterns off it. Gitea #154.
pub static DATA_PIN: AtomicU8 = AtomicU8::new(0);
/// The *stored* override, +1 (0 = board default) — the requested value the
/// settings record must reflect, and what `/api/datapin` changes. Differs
/// from `DATA_PIN` only between a POST and the reboot that applies it.
pub static WANT_DATA_PIN: AtomicU8 = AtomicU8::new(0);

pub fn want_data_pin() -> Option<u8> {
    WANT_DATA_PIN.load(core::sync::atomic::Ordering::Relaxed).checked_sub(1)
}

pub fn set_want_data_pin(p: Option<u8>) {
    WANT_DATA_PIN.store(p.map_or(0, |p| p + 1), core::sync::atomic::Ordering::Relaxed);
}

type Shared<T> = BlockingMutex<CriticalSectionRawMutex, RefCell<T>>;

fn share_get<T: Clone>(cell: &Shared<T>) -> T {
    cell.lock(|c| c.borrow().clone())
}

/// Most recent runtime (vmerr) message with source location, cleared when a
/// new pattern is accepted.
pub static LAST_VMERR: Shared<Option<String>> = BlockingMutex::new(RefCell::new(None));

pub fn set_vmerr(msg: Option<String>) {
    LAST_VMERR.lock(|c| *c.borrow_mut() = msg);
}

pub fn get_vmerr() -> Option<String> {
    share_get(&LAST_VMERR)
}

/// Snapshot of the last rendered frame (RGB bytes, 3 per pixel) for
/// `GET /api/pixels`.
///
/// Not on a pipelined board (pipeline.rs, Gitea #306): there the frame
/// buffer travelling to the output task already holds the last complete
/// frame, and `pipeline::preview` serves the API straight out of it — a
/// second 12 KB copy of a frame that already exists is exactly the RAM the
/// pipeline needed.
#[cfg(not(pipelined))]
pub static PIXELS: Shared<Vec<u8>> = BlockingMutex::new(RefCell::new(Vec::new()));

#[cfg(not(pipelined))]
pub fn set_pixels(rgb: &[[u8; 3]]) {
    PIXELS.lock(|c| {
        let mut v = c.borrow_mut();
        v.clear();
        // one memcpy of the whole frame, not one bounds-checked 3-byte
        // extend per pixel — this runs on every rendered frame
        v.extend_from_slice(rgb.as_flattened());
    });
}

#[cfg(not(pipelined))]
pub fn get_pixels() -> Vec<u8> {
    share_get(&PIXELS)
}

// --- running pattern read-back (flash-resident; see patterns::store_current) ---
//
// The source + LXBC blob of the *currently running* pattern used to sit in
// two standing heap copies (PATTERN_SRC / PATTERN_BC) purely to serve them
// back: GET /api/pattern, the sync envelope (/api/pattern.lxp), and the
// engine rebuild on a pixel-count / map change. On a heap already dominated
// by that same running pattern those copies were its single largest resident
// cost (~40 KB for a big pattern) — and nothing on the render path ever reads
// them. They now live in the flash storage partition and are streamed on
// demand; only these tiny location scalars stay in RAM. The compile-time
// default is `&'static` rodata, so it serves with zero heap and zero flash.

/// Where the running pattern's SOURCE currently lives, for read-back.
#[derive(Clone, Copy)]
pub enum SrcLoc {
    /// Compile-time default (rodata) — no heap, no flash write.
    Default(&'static str),
    /// In the flash read-back slot; the usize is its exact byte length (for
    /// the streamer and Content-Length).
    Flash(usize),
    /// A LIBRARY pattern: the bytes already live in the pattern store under
    /// [CURRENT_PATTERN_ID], so the swap wrote nothing — read-back loads
    /// them transiently via patterns::source_of. The usize is the length
    /// snapshot from the swapped envelope (Content-Length); a re-save or
    /// delete mid-request degrades to truncate/pad, never a panic. This is
    /// what makes playlist churn flash-WEAR-free: the raw slot's fixed
    /// sectors are no longer erased on every item advance (~17k cycles/day
    /// at 5 s items against a ~100k NOR spec before this existed).
    Library(usize),
    /// The swap's flash write failed — nothing to serve until the next swap.
    Gone,
}

/// Where the running pattern's LXBC BLOB currently lives — the mirror of
/// [SrcLoc] for the sync envelope and engine rebuilds.
#[derive(Clone, Copy)]
pub enum BcLoc {
    Default(&'static [u8]),
    Flash(usize),
    /// See [SrcLoc::Library]; loads via patterns::bytecode_of.
    Library(usize),
    Gone,
}

/// The running pattern's read-back locations. Only tiny scalars/pointers —
/// the actual source + blob bytes are in flash (or rodata for the default),
/// never a standing heap copy.
struct CurrentMeta {
    src: SrcLoc,
    bc: BcLoc,
}

static CURRENT: Shared<CurrentMeta> =
    BlockingMutex::new(RefCell::new(CurrentMeta { src: SrcLoc::Gone, bc: BcLoc::Gone }));

/// FNV-1a of the running source — the sync beacon's pattern identity. Stays
/// in RAM (a single u32) as before; every swap restamps it.
pub static PATTERN_HASH: AtomicU32 = AtomicU32::new(0);

/// Restamp the sync pattern-identity hash from the running source.
pub fn set_pattern_hash(src: &str) {
    set_pattern_hash_raw(luxel_core::netin::fnv1a(src.as_bytes()));
}

/// Same, from a hash computed elsewhere (patterns::source_stat streams the
/// source out of flash without materializing it).
pub fn set_pattern_hash_raw(h: u32) {
    use core::sync::atomic::Ordering;
    PATTERN_HASH.store(h, Ordering::Relaxed);
}

/// Boot / built-in default: source + blob are compile-time `&'static` rodata,
/// so read-back serves them directly with zero heap and zero flash writes.
/// Also the standing fallback whenever no pattern has been swapped in this
/// session — the flash read-back slot is only authoritative once a swap
/// rewrites it (at boot it may still hold the previous session's pattern).
pub fn set_current_default(src: &'static str, bc: &'static [u8]) {
    set_pattern_hash(src);
    CURRENT.lock(|c| {
        *c.borrow_mut() = CurrentMeta { src: SrcLoc::Default(src), bc: BcLoc::Default(bc) };
    });
}

/// A swapped-in pattern was persisted to the flash read-back slot: point
/// read-back at flash, recording the byte lengths. The caller restamps the
/// hash (it holds the source).
pub fn set_current_flash(src_len: usize, bc_len: usize) {
    CURRENT.lock(|c| {
        *c.borrow_mut() = CurrentMeta { src: SrcLoc::Flash(src_len), bc: BcLoc::Flash(bc_len) };
    });
}

/// A swapped-in LIBRARY pattern: read-back serves from the pattern store
/// (under the already-stamped [CURRENT_PATTERN_ID]) — no slot write
/// happened. Lengths are the swapped envelope's, for Content-Length.
pub fn set_current_library(src_len: usize, bc_len: usize) {
    CURRENT.lock(|c| {
        *c.borrow_mut() =
            CurrentMeta { src: SrcLoc::Library(src_len), bc: BcLoc::Library(bc_len) };
    });
}

/// The swap's flash write failed — read-back has nothing to serve until the
/// next swap (/api/status then reports src=false, bc=false).
pub fn set_current_gone() {
    CURRENT.lock(|c| *c.borrow_mut() = CurrentMeta { src: SrcLoc::Gone, bc: BcLoc::Gone });
}

/// Snapshot of where the running source lives (the /api/pattern streamer).
pub fn current_src() -> SrcLoc {
    CURRENT.lock(|c| c.borrow().src)
}

/// Snapshot of where the running blob lives (sync envelope + engine rebuild).
pub fn current_bc() -> BcLoc {
    CURRENT.lock(|c| c.borrow().bc)
}

/// /api/status observability: is each read-back copy currently serveable?
/// True for the rodata default or a good flash write; false only after a
/// flash write shed the copy (fragmentation / flash busy) — the soak-log
/// signal that shedding happened.
pub fn current_src_available() -> bool {
    !matches!(current_src(), SrcLoc::Gone)
}
pub fn current_bc_available() -> bool {
    !matches!(current_bc(), BcLoc::Gone)
}

/// Master power (Home Assistant's light switch): when false the render task
/// outputs black — the engine keeps ticking so ON resumes mid-animation.
/// Written by the MQTT task, read by the render task and state publishing.
pub static POWER: AtomicBool = AtomicBool::new(true);

/// Library id of the running pattern ("" = ad-hoc code push / built-in
/// default). Stamped by the RENDER TASK at the swap, from the id carried in
/// Msg::Code/Crossfade — always consistent with the running content and the
/// read-back location (senders stamping it after send raced the queue). The
/// MQTT pattern-select state and the Library read-back paths read it.
pub static CURRENT_PATTERN_ID: Shared<String> = BlockingMutex::new(RefCell::new(String::new()));

pub fn set_current_pattern_id(id: &str) {
    CURRENT_PATTERN_ID.lock(|c| {
        let mut s = c.borrow_mut();
        s.clear();
        s.push_str(id);
    });
}

pub fn get_current_pattern_id() -> String {
    share_get(&CURRENT_PATTERN_ID)
}

/// Control values explicitly set since the last pattern swap (name → raw
/// 16.16), for single-pattern reboot resume (resume.rs). Reset on
/// activation (controls return to the pattern's defaults) and seeded from
/// the playlist item's saved values on playlist entry.
pub static CURRENT_CONTROLS: Shared<Vec<(String, Vec<i32>)>> =
    BlockingMutex::new(RefCell::new(Vec::new()));

/// Record one explicitly-set control (replaces a previous value by name).
pub fn record_control(name: &str, values: &[Fx]) {
    CURRENT_CONTROLS.lock(|c| {
        let mut list = c.borrow_mut();
        let raw: Vec<i32> = values.iter().map(|v| v.raw()).collect();
        if let Some(entry) = list.iter_mut().find(|(n, _)| n == name) {
            entry.1 = raw;
        } else {
            list.push((String::from(name), raw));
        }
    });
}

/// Replace the whole set (activation reset / playlist entry / boot resume).
pub fn set_current_controls(controls: Vec<(String, Vec<i32>)>) {
    CURRENT_CONTROLS.lock(|c| *c.borrow_mut() = controls);
}

pub fn get_current_controls() -> Vec<(String, Vec<i32>)> {
    share_get(&CURRENT_CONTROLS)
}

/// Poked when the MQTT broker config changes so the MQTT task reconnects
/// (or connects for the first time) without a reboot.
pub static MQTT_POKE: embassy_sync::signal::Signal<CriticalSectionRawMutex, ()> =
    embassy_sync::signal::Signal::new();

/// Luxel-to-Luxel sync role (0 off, 1 leader, 2 follower). Seeded from
/// flash at boot; POST /api/sync writes it live (and persists).
pub static SYNC_MODE: AtomicU8 = AtomicU8::new(0);

/// Engine clock in ms, published by the render task every frame (the
/// leader beacon's payload; also /api/sync). u64 needs the critical
/// section — no 64-bit atomics on these cores.
pub static ENGINE_TIME_MS: Shared<u64> = BlockingMutex::new(RefCell::new(0));

pub fn set_engine_time_ms(ms: u64) {
    ENGINE_TIME_MS.lock(|c| *c.borrow_mut() = ms);
}

pub fn engine_time_ms() -> u64 {
    share_get(&ENGINE_TIME_MS)
}

/// Last sync beacon heard as a follower: (leader boot id, leader engine ms,
/// when it arrived). The render task slews toward it.
pub static SYNC_LEADER: Shared<Option<(u32, u64, embassy_time::Instant)>> =
    BlockingMutex::new(RefCell::new(None));

pub fn set_sync_leader(boot_id: u32, time_ms: u64) {
    SYNC_LEADER.lock(|c| *c.borrow_mut() = Some((boot_id, time_ms, embassy_time::Instant::now())));
}

pub fn sync_leader() -> Option<(u32, u64, embassy_time::Instant)> {
    share_get(&SYNC_LEADER)
}

pub fn clear_sync_leader() {
    SYNC_LEADER.lock(|c| *c.borrow_mut() = None);
}

/// Output pipeline knobs (Settings → output; persisted; the render task
/// applies them between blending and protocol encoding).
pub static COLOR_ORDER: AtomicU8 = AtomicU8::new(0); // outpipe::ColorOrder code
pub static GAMMA_TENTHS: AtomicU8 = AtomicU8::new(0); // 22 = γ2.2; 0/10 = off
pub static CAP_MA: AtomicU32 = AtomicU32::new(0); // 0 = no power cap

/// Whole-frame post-process knobs (Settings → output; persisted). The
/// brightness curve reshapes the master dimmer's response (distinct from
/// GAMMA_TENTHS, which curves per-pixel content); blur and glow are the
/// spatial stages the render task runs before gamma.
pub static BRIGHT_CURVE: AtomicU8 = AtomicU8::new(0); // dimmer gamma x10; 0/10 = off
pub static POST_BLUR: AtomicU8 = AtomicU8::new(0); // percent, 0 = off
pub static POST_GLOW: AtomicU8 = AtomicU8::new(0); // percent, 0 = off

/// Device output palette (Settings → output; persisted as a pattern-store
/// blob — see outpal.rs). The stop list is read only when it
/// changes: the render task caches a cooked 256-entry LUT behind
/// POST_PALETTE_EPOCH, exactly like the gamma LUT, so the frame path never
/// takes this critical section.
pub static POST_PALETTE: Shared<Vec<(u8, [u8; 3])>> = BlockingMutex::new(RefCell::new(Vec::new()));
pub static POST_PALETTE_AMOUNT: AtomicU8 = AtomicU8::new(0); // percent, 0 = off
pub static POST_PALETTE_EPOCH: AtomicU32 = AtomicU32::new(0);

/// Install a device palette (empty stops = clear). Bumps the epoch so the
/// render task rebuilds its LUT on the next frame; the amount is stored
/// separately because changing only the blend needs no rebuild.
pub fn set_post_palette(stops: Vec<(u8, [u8; 3])>, amount_pct: u8) {
    use core::sync::atomic::Ordering;
    // The epoch bump rides inside the same critical section as the swap —
    // the C3 (riscv32imc) has no atomic read-modify-write, so `fetch_add`
    // doesn't exist there; the lock is what makes this indivisible anyway.
    POST_PALETTE.lock(|c| {
        *c.borrow_mut() = stops;
        let next = POST_PALETTE_EPOCH.load(Ordering::Relaxed).wrapping_add(1);
        POST_PALETTE_EPOCH.store(next, Ordering::Relaxed);
    });
    POST_PALETTE_AMOUNT.store(amount_pct, Ordering::Relaxed);
}

/// The installed device palette stops (a clone — callers are the LUT cook
/// and the API, both off the per-frame path).
pub fn post_palette_stops() -> Vec<(u8, [u8; 3])> {
    share_get(&POST_PALETTE)
}

/// The persisted settings record built from the live atomics — write sites
/// override the one field they change instead of hand-assembling the
/// (ever-growing) struct. Pixel count and protocol come from the WANT_*
/// (requested) atomics: the applied ones lag until the render task drains
/// the message, and persistence must never lose an in-flight change.
pub fn device_config_snapshot() -> crate::config::DeviceConfig {
    use core::sync::atomic::Ordering;
    crate::config::DeviceConfig {
        brightness: BRIGHTNESS.load(Ordering::Relaxed),
        protocol: WANT_PROTOCOL.load(Ordering::Relaxed),
        sync_mode: SYNC_MODE.load(Ordering::Relaxed),
        pixel_count: WANT_PIXEL_COUNT.load(Ordering::Relaxed),
        tz_minutes: TZ_MINUTES.load(Ordering::Relaxed) as i16,
        color_order: COLOR_ORDER.load(Ordering::Relaxed),
        gamma_tenths: GAMMA_TENTHS.load(Ordering::Relaxed),
        cap_ma: CAP_MA.load(Ordering::Relaxed) as u16,
        bright_curve_tenths: BRIGHT_CURVE.load(Ordering::Relaxed),
        blur_pct: POST_BLUR.load(Ordering::Relaxed),
        glow_pct: POST_GLOW.load(Ordering::Relaxed),
        data_pin: want_data_pin(),
    }
}

/// Wall clock: (unix seconds at the last NTP sync, when it landed).
/// None = never synced — clock builtins stay at 0, like before.
pub static WALL_CLOCK: Shared<Option<(i64, embassy_time::Instant)>> =
    BlockingMutex::new(RefCell::new(None));

/// Local-time offset from UTC in minutes (Settings → clock; persisted).
pub static TZ_MINUTES: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

pub fn set_wall_clock(unix: i64) {
    WALL_CLOCK.lock(|c| *c.borrow_mut() = Some((unix, embassy_time::Instant::now())));
}

/// Current LOCAL unix-style seconds (UTC + tz), if synced.
pub fn wall_now_local() -> Option<i64> {
    use core::sync::atomic::Ordering;
    let (base, at) = share_get(&WALL_CLOCK)?;
    Some(base + at.elapsed().as_secs() as i64 + TZ_MINUTES.load(Ordering::Relaxed) as i64 * 60)
}

/// Latest sensor frame (PB sensor-board serial or POST /api/sensors) + a
/// sequence counter so the render task applies each frame exactly once.
pub static SENSOR_FRAME: Shared<Option<luxel_core::engine::SensorFrame>> =
    BlockingMutex::new(RefCell::new(None));
pub static SENSOR_SEQ: AtomicU32 = AtomicU32::new(0);

pub fn set_sensor_frame(s: luxel_core::engine::SensorFrame) {
    use core::sync::atomic::Ordering;
    // seq bump inside the critical section (rv32imc has no fetch_add, and
    // there are multiple writers: the UART task and HTTP handlers)
    SENSOR_FRAME.lock(|c| {
        *c.borrow_mut() = Some(s);
        SENSOR_SEQ.store(SENSOR_SEQ.load(Ordering::Relaxed).wrapping_add(1), Ordering::Relaxed);
    });
}

/// The newest unapplied sensor frame, if any (tracks per-caller via `seen`).
pub fn take_sensor_frame(seen: &mut u32) -> Option<luxel_core::engine::SensorFrame> {
    use core::sync::atomic::Ordering;
    let seq = SENSOR_SEQ.load(Ordering::Relaxed);
    if seq == *seen {
        return None;
    }
    *seen = seq;
    share_get(&SENSOR_FRAME)
}

/// Injected external events (POST /api/events) awaiting the render task —
/// a small batch buffer, drained whole between frames. The engine's own
/// queue enforces the drop-oldest cap; this only bridges tasks, so it
/// carries the same bound. Alloc is fallible: a heap-starved push drops
/// the event (best-effort input, like sensor frames).
pub static EVENTS: Shared<Vec<[luxel_core::fixed::Fx; 4]>> =
    BlockingMutex::new(RefCell::new(Vec::new()));

pub fn push_events(evs: &[[luxel_core::fixed::Fx; 4]]) {
    EVENTS.lock(|c| {
        let mut q = c.borrow_mut();
        for &e in evs {
            if q.len() >= luxel_core::vm::MAX_EVENTS {
                q.remove(0);
            }
            if q.len() == q.capacity() && q.try_reserve(1).is_err() {
                return;
            }
            q.push(e);
        }
    });
}

/// All pending events (empty Vec if none — no alloc on the idle path).
pub fn take_events() -> Vec<[luxel_core::fixed::Fx; 4]> {
    EVENTS.lock(|c| core::mem::take(&mut *c.borrow_mut()))
}

/// Network input (DDP/E1.31): the assembled RGB frame. While packets flow
/// (see LIVE_MARK_MS) the render task outputs this instead of the engine.
pub static LIVE_PIXELS: Shared<Vec<u8>> = BlockingMutex::new(RefCell::new(Vec::new()));

/// embassy now() ms of the last network-input packet (0 = never), and which
/// protocol sent it (0 = none, 1 = DDP, 2 = E1.31). Written by the netin
/// task, read by the render task and /api/status.
pub static LIVE_MARK_MS: AtomicU32 = AtomicU32::new(0);
pub static LIVE_PROTO: AtomicU8 = AtomicU8::new(0);

/// How long after the last DDP/E1.31 packet the pattern takes back over.
pub const LIVE_TIMEOUT_MS: u32 = 2500;

/// The protocol currently overriding the engine, if any (shared by the
/// render task's frame gate and the status JSON).
pub fn live_proto(now_ms: u32) -> Option<&'static str> {
    use core::sync::atomic::Ordering;
    let mark = LIVE_MARK_MS.load(Ordering::Relaxed);
    if mark == 0 || now_ms.wrapping_sub(mark) >= LIVE_TIMEOUT_MS {
        return None;
    }
    match LIVE_PROTO.load(Ordering::Relaxed) {
        1 => Some("ddp"),
        2 => Some("e131"),
        _ => None,
    }
}

/// JSON snapshots published by the render task (see luxel_core::jsonview):
/// controls on pattern swap; vars/readouts every ~250 ms.
pub static CONTROLS_JSON: Shared<String> = BlockingMutex::new(RefCell::new(String::new()));
pub static VARS_JSON: Shared<String> = BlockingMutex::new(RefCell::new(String::new()));
pub static READOUTS_JSON: Shared<String> = BlockingMutex::new(RefCell::new(String::new()));

pub fn publish(cell: &Shared<String>, json: String) {
    cell.lock(|c| *c.borrow_mut() = json);
}

pub fn snapshot(cell: &Shared<String>) -> String {
    let s = share_get(cell);
    if s.is_empty() {
        String::from("{}")
    } else {
        s
    }
}
