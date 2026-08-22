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

/// The array-arena byte budget the firmware grants a pattern loading against
/// `heap_free` bytes of free heap.
///
/// Byte-accurate (elements × 8 + per-array overhead), so one big array isn't
/// taxed for overhead that only swarms of tiny arrays pay.
pub const fn array_budget(heap_free: usize) -> usize {
    let b = heap_free.saturating_sub(RUNTIME_FLOOR + BUDGET_HEADROOM);
    if b < MIN_ARRAY_BUDGET {
        MIN_ARRAY_BUDGET
    } else {
        b
    }
}

/// How many bytes a pattern may consume before the post-load floor check
/// rejects it, given `heap_free` bytes free at the moment the push arrives.
///
/// The firmware builds the new engine while the OLD one is still resident
/// (`engine = Some(e)` only after the build succeeds), so free heap at status
/// time really is the headroom the incoming pattern has to fit inside.
pub const fn load_headroom(heap_free: usize) -> usize {
    heap_free.saturating_sub(RUNTIME_FLOOR)
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
    fn headroom_saturates_at_the_floor() {
        assert_eq!(load_headroom(100 * 1024), 80 * 1024);
        assert_eq!(load_headroom(RUNTIME_FLOOR), 0);
        assert_eq!(load_headroom(1024), 0);
    }
}
