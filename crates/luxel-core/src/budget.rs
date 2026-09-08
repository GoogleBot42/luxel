//! The device heap budget — the ONE place the firmware's capacity rules live.
//!
//! The firmware builds every pattern's engine with an array-arena byte budget
//! derived from free heap, then rejects the pattern outright if the loaded
//! engine left the heap under a runtime floor (see `docs/firmware.md`,
//! "Heap economics"). Both halves are pure arithmetic over `heap_free`, so
//! they live here rather than in `firmware/`: the wasm build imports the same
//! constants to tell the user *before* a push whether their pattern will fit
//! the device they are connected to (Gitea #15).
//!
//! Changing a constant here changes the device's behaviour AND the web
//! editor's prediction of it in lockstep — which is the entire point.

/// Heap the rest of the firmware needs while a pattern runs, and which a
/// pattern may therefore never eat into: jsonview snapshots (~8.5 KB peak
/// for var-heavy patterns — an 8 KB floor was lost to exactly that once),
/// MQTT publishes, SPI buffer resizes, WiFi-blob mallocs (which do NOT
/// null-check), plus two HTTP connection buffers (4 KB each — bodies
/// STREAM, so connections never need body-sized buffers).
///
/// `try_budgeted_engine` measures free heap immediately after building the
/// engine; below this it drops the engine and reports the friendly
/// "pattern too large for this device" vmerr instead of panicking later on a
/// routine allocation (soak v5 found an 8.5 KB jsonview alloc panicking —
/// i.e. rebooting — right after such a pattern loaded). Soak-proven to
/// reject, never panic.
pub const RUNTIME_FLOOR: usize = 20 * 1024;

/// Slack between the array arena's ceiling and the runtime floor, so a
/// maxed-out arena doesn't sit EXACTLY on the floor and lose the post-load
/// check to a few bytes of churn.
pub const BUDGET_HEADROOM: usize = 4 * 1024;

/// Floor on the array budget itself: ordinary strip patterns (a few arrays of
/// `pixelCount`) must keep working even when free heap reads low mid-churn.
/// If that minimum genuinely doesn't fit, the post-load floor check rejects
/// the pattern instead — soak-proven to be a rejection, never a panic.
pub const MIN_ARRAY_BUDGET: usize = 16 * 1024;

/// Percentage of the load headroom a pattern may model before the editor
/// calls the fit "tight". The model is a measurement, not a size heuristic,
/// but the device's heap moves underneath it between the status read and the
/// load (WiFi buffers, HTTP connection buffers, MQTT publishes, jsonview
/// snapshots), so the last stretch of headroom is not honestly spendable.
pub const TIGHT_PERCENT: usize = 85;

/// The array-arena byte budget the firmware grants a pattern loading against
/// `heap_free` bytes of free heap.
///
/// Byte-accurate (elements × 8 + per-array overhead), so one big array isn't
/// taxed for overhead that only swarms of tiny arrays pay.
///
/// The device reads `esp_alloc::HEAP.free()` inside `budgeted_engine`, i.e.
/// with the incoming `Program` ALREADY decoded and the upload envelope
/// already dropped. A predictor must pass the same thing: the load base
/// ([`load_base`]) minus the resident program, not the raw `/api/status`
/// `heap_free`.
pub const fn array_budget(heap_free: usize) -> usize {
    let b = heap_free.saturating_sub(RUNTIME_FLOOR + BUDGET_HEADROOM);
    if b < MIN_ARRAY_BUDGET {
        MIN_ARRAY_BUDGET
    } else {
        b
    }
}

/// Bytes of an external array arena the firmware keeps back for itself.
///
/// The arena is only ever used for pattern-array *storage*, so unlike
/// [`RUNTIME_FLOOR`] nothing else competes for it. The reserve exists so a
/// pattern that maxes out its budget still leaves room for the copy-on-write
/// promotion of a const array and for the transient double-allocation a
/// `Vec` growth does (old + new alive at once) — both of which charge the
/// byte ledger only *after* the allocation succeeded.
pub const ARENA_RESERVE: usize = 256 * 1024;

/// The array-arena byte budget on a board with a dedicated external arena
/// (Gitea #253): the arena's own free space, less [`ARENA_RESERVE`], and
/// never below what the main-heap rule would have granted.
///
/// `heap_free` is still the internal free heap, because a pattern's
/// non-array cost (its `Program`, VM globals, the pixel buffer) comes out of
/// internal RAM exactly as before — this only changes where the *arrays*
/// come from, so the internal-heap answer stays the floor.
pub const fn external_array_budget(heap_free: usize, arena_free: usize) -> usize {
    let internal = array_budget(heap_free);
    let external = arena_free.saturating_sub(ARENA_RESERVE);
    if external > internal {
        external
    } else {
        internal
    }
}

/// Bytes one arena element costs. Pinned against the real `Value` below,
/// because [`external_element_budget`] converts between the two ledgers.
pub const BYTES_PER_ELEMENT: usize = 8;
const _: () = assert!(core::mem::size_of::<crate::vm::Value>() == BYTES_PER_ELEMENT);

/// Element ledger to pair with [`external_array_budget`].
///
/// PB's 10,236-unit ledger (`crate::vm::DEFAULT_ARRAY_BUDGET`) is a
/// *memory* budget in disguise — 40 KiB of 4-byte elements on a device with
/// no other option. A board with a dedicated external arena has a real byte
/// budget, so the element ledger is set just high enough to stop binding and
/// the bytes do the work. `crate::vm::MAX_ARENA_SLOTS` independently bounds
/// the slot vector, which stays on the ordinary allocator.
///
/// This is a deliberate, board-scoped divergence from PB: a pattern that a
/// real Pixel Blaze rejects with "array element budget exceeded" can run
/// here. Every board without an arena keeps the PB number exactly.
pub const fn external_element_budget(array_byte_budget: usize) -> usize {
    array_byte_budget / BYTES_PER_ELEMENT
}

/// Free heap a pattern load actually starts from, given the two numbers
/// `/api/status` reports: `heap_free` (free right now, with the CURRENT
/// pattern's engine resident) and `engine_heap` (what that engine costs).
///
/// The firmware drops the outgoing engine *before* it decodes the incoming
/// program — `engine = None; drop_prev(&mut prev);` is the first thing both
/// `Msg::Code` and a non-crossfading `Msg::Library` do — so the heap the new
/// pattern is measured against is `heap_free` PLUS everything the old engine
/// gives back. Predicting against `heap_free` alone charges the incoming
/// pattern for the outgoing one, which is how the editor came to warn about
/// patterns that load fine (Gitea #287): the fatter the resident pattern,
/// the lower `heap_free`, the more the editor cried wolf.
///
/// `engine_heap` 0 means the firmware doesn't report it (pre-#287 builds, the
/// native mirror): fall back to `heap_free`, which is the old, conservative
/// behaviour.
pub const fn load_base(heap_free: usize, engine_heap: usize) -> usize {
    heap_free.saturating_add(engine_heap)
}

/// How many bytes of RESIDENT engine a pattern may leave behind before the
/// post-load floor check rejects it, given `base_free` bytes free at the
/// start of the load (see [`load_base`]).
///
/// The comparison is against what the pattern still occupies once the load
/// has settled, NOT the transient peak of the load window: the firmware
/// drops the upload envelope before it builds the engine (`drop(env)` sits
/// above `engine_or_vmerr` in `Msg::Code` for exactly this reason), so the
/// floor check never sees the envelope. The transient peak matters only for
/// whether the decode can allocate at all — that is [`fit`]'s `peak`
/// argument, compared against the whole of `base_free`.
pub const fn load_headroom(base_free: usize) -> usize {
    base_free.saturating_sub(RUNTIME_FLOOR)
}

/// How a modelled pattern fits the device it would be pushed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Loads with room to spare.
    Fits,
    /// Loads, but inside the last [`TIGHT_PERCENT`] of the headroom, where
    /// ordinary heap churn between the status read and the load could still
    /// tip it over.
    Tight,
    /// The device would reject it (or fail to allocate during the decode).
    Over,
}

impl Fit {
    /// Stable lowercase name, for the wasm model's JSON and the UI.
    pub const fn as_str(self) -> &'static str {
        match self {
            Fit::Fits => "fits",
            Fit::Tight => "tight",
            Fit::Over => "over",
        }
    }
}

/// The device's verdict on a modelled load.
///
/// * `resident` — bytes the pattern still occupies once the load has settled
///   (program tables + owned code words if any + engine + arrays). This is
///   what `try_budgeted_engine`'s floor check measures.
/// * `peak` — the transient high-water of the load window (upload envelope +
///   decoded program + the store's 4 KiB write staging, before the envelope
///   is dropped). It never reaches the floor check, but it must fit in free
///   heap or the decode's fallible allocations fail.
/// * `base_free` — free heap at the start of the load, from [`load_base`].
pub const fn fit(resident: usize, peak: usize, base_free: usize) -> Fit {
    let headroom = load_headroom(base_free);
    if resident > headroom || peak > base_free {
        Fit::Over
    } else if resident * 100 > headroom * TIGHT_PERCENT {
        Fit::Tight
    } else {
        Fit::Fits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_tracks_free_heap() {
        // roomy device: everything above floor + headroom is array arena
        assert_eq!(array_budget(100 * 1024), 76 * 1024);
        // starved device: never below the minimum
        assert_eq!(array_budget(30 * 1024), MIN_ARRAY_BUDGET);
        assert_eq!(array_budget(0), MIN_ARRAY_BUDGET);
    }

    #[test]
    fn external_arena_never_reads_below_the_main_heap_rule() {
        // 8 MB arena, idle S3 panel heap: the arena wins by three orders
        // of magnitude
        assert_eq!(
            external_array_budget(51_704, 8 * 1024 * 1024),
            8 * 1024 * 1024 - ARENA_RESERVE
        );
        // an arena smaller than the reserve is worse than no arena, so the
        // main-heap rule still applies
        assert_eq!(
            external_array_budget(100 * 1024, 64 * 1024),
            array_budget(100 * 1024)
        );
        assert_eq!(external_array_budget(100 * 1024, 0), 76 * 1024);
    }

    #[test]
    fn headroom_saturates_at_the_floor() {
        assert_eq!(load_headroom(100 * 1024), 80 * 1024);
        assert_eq!(load_headroom(RUNTIME_FLOOR), 0);
        assert_eq!(load_headroom(1024), 0);
    }

    #[test]
    fn load_base_credits_the_outgoing_engine() {
        // firmware that reports `engine_heap`: the pattern about to be
        // replaced hands its heap back before the new one is measured
        assert_eq!(load_base(31_000, 40_000), 71_000);
        // firmware that doesn't (0): fall back to the raw free-heap number
        assert_eq!(load_base(104_832, 0), 104_832);
    }

    /// The regression Gitea #287 reported: patterns that load fine were
    /// warned about. Every number below is MEASURED under the same counting
    /// allocator the wasm model uses, at 300 px, on the gallery at this
    /// revision — `crates/luxel-cli/tests/heapstat.rs` replays the same
    /// lifecycles for every pattern.
    ///
    /// The device modelled is one with a fat pattern already loaded: 25,000 B
    /// of `heap_free` and a 30,000 B resident engine, i.e. a load base of
    /// 55,000 B. That is the shape the old model got most wrong, because it
    /// read 25,000 B free, subtracted the 20 KB floor, and had 4,520 B of
    /// "headroom" left to judge every pattern's transient PEAK against —
    /// which flagged all 305 gallery patterns. Three are genuinely over.
    #[test]
    fn gallery_patterns_that_load_no_longer_cry_wolf() {
        let base = load_base(25_000, 30_000);
        assert_eq!(load_headroom(base), 34_520);

        // "Chasing Rainbows & HSLuv": 14,228 B envelope, 26,070 B transient
        // peak, 18,407 B resident. The old model compared that peak against
        // 4,520 B and said "too large for this device". It is not.
        assert!(26_070 > load_headroom(25_000));
        assert_eq!(fit(18_407, 26_070, base), Fit::Fits);

        // "Fireworks Finale" and "Utility: Palettes" — same story.
        assert_eq!(fit(21_941, 29_335, base), Fit::Fits);
        assert_eq!(fit(13_580, 30_444, base), Fit::Fits);

        // "Infinite Snake v2" really does sit near the edge:
        // 32,454 B resident of 34,520 B headroom. Tight, not over.
        assert_eq!(fit(32_454, 34_307, base), Fit::Tight);

        // Seengreat HUB75 S3 at 4096 px, idle `heap_free` 51,704 B with the
        // rainbow default resident (~5 KB). The panel's pixel-count-sized
        // arrays and buffers cost real heap, but not enough to warn about
        // ordinary patterns.
        let s3 = load_base(51_704, 5_000);
        assert_eq!(load_headroom(s3), 36_224);
        assert_eq!(fit(21_987, 26_070, s3), Fit::Fits);
        assert_eq!(fit(21_425, 29_335, s3), Fit::Fits);

        // Athom / classic ESP32 at 60 px, 104,832 B idle: the gallery's
        // largest pattern ("Main Stage", 45,746 B envelope) fits even on
        // firmware too old to report `engine_heap`.
        assert_eq!(fit(28_055, 70_987, load_base(104_832, 0)), Fit::Fits);
    }

    /// The warning must still fire where the device really would reject.
    #[test]
    fn genuinely_oversized_patterns_still_warn() {
        let base = load_base(25_000, 30_000);

        // "Main Stage" live-pushed at 300 px: its 45,746 B envelope plus the
        // decoded program plus the store's 4 KiB write staging peak at
        // 70,987 B — more than the device has, so the decode's fallible
        // allocations fail before the floor check ever runs. Resident
        // (34,535 B) would have fitted; the upload does not. Activated from
        // the device's own library it costs 22,323 B and does fit — which is
        // why the model reports both paths.
        assert_eq!(fit(34_535, 70_987, base), Fit::Over);
        assert_eq!(fit(22_323, 22_451, base), Fit::Fits);

        // "2D Fireworks Fade" on the S3 panel at 4096 px: 43,838 B of
        // settled engine against 36,224 B of headroom. Over however it
        // arrives — 45,402 B even on the borrowing path.
        let s3 = load_base(51_704, 5_000);
        assert_eq!(fit(43_838, 48_242, s3), Fit::Over);
        assert_eq!(fit(45_402, 45_626, s3), Fit::Over);
    }

    #[test]
    fn tight_is_the_last_slice_of_headroom() {
        // headroom = 80 KB; 85 % of it is 69,632
        let base = 100 * 1024;
        assert_eq!(fit(69_632, 0, base), Fit::Fits);
        assert_eq!(fit(69_633, 0, base), Fit::Tight);
        assert_eq!(fit(80 * 1024, 0, base), Fit::Tight);
        assert_eq!(fit(80 * 1024 + 1, 0, base), Fit::Over);
    }
}
