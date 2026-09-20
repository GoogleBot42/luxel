//! AppCpu liveness gate for the RTC watchdog (Gitea #603).
//!
//! On the dual-core boards `render_task` runs on the AppCpu while the RTC
//! watchdog is fed only from the ProCpu (`core1::watchdog_task` every
//! [`TICK_MS`], plus `core1::fenced` every 64 fences). A render loop that
//! *wedges* — spins, or waits on something that never completes — therefore
//! left the board dark with a healthy ProCpu feeding the watchdog forever,
//! and only a hands-on power cycle recovered it. That is the shape of the
//! failure Jeremy hit on the Seengreat panel (#601).
//!
//! The render loop now stamps a counter once per iteration (`core1::beat`)
//! and the watchdog task refuses to feed once that counter has been frozen
//! for [`STALL_LIMIT_MS`]. This module is the decision itself, kept pure —
//! no HAL, no atomics, no clock — so `cargo test -p appwdt-check` can run
//! the whole truth table on the host (`tools/appwdt-check`).
//!
//! ## Why a *credit* for the watchdog task's own lateness
//!
//! Plenty of legitimate states freeze the heartbeat for many seconds, and a
//! false trip here reboots a healthy device — inside the boot-loop guard's
//! window, mid-flash-write, which is how you brick one. The states that
//! matter, and what each does:
//!
//! * **No pattern loaded / a pattern rejected with `out_fps 0`** — the loop
//!   still iterates (a 50 ms idle sleep, then `continue`), so the heartbeat
//!   advances. The gate is on the render LOOP, never on frames emitted.
//! * **A legitimately slow frame** — `ripples-2d` is 234 ms/frame on the
//!   panel (docs/boards.md). [`STALL_LIMIT_MS`] clears that by 40x.
//! * **A long flash burst on the ProCpu** — a 728 KB asset install is ~15 s
//!   of erases and a garbage-collecting pattern save was measured at 25 s
//!   (#292). During one the AppCpu is parked for every op (the flash fence),
//!   and on a pipelined board the render loop *also* waits on
//!   `pipeline::output_task`, which is on the blocked ProCpu executor — so
//!   the heartbeat can legitimately stand still for the whole burst.
//! * **A flash write from the render task itself** (`persist_current_pattern`)
//!   — the AppCpu takes the fence and parks the *ProCpu* for each op.
//! * **OTA** — the same thing, at image scale.
//!
//! Every one of those blocks the ProCpu executor (or parks it outright), and
//! `watchdog_task` lives on that executor: its own tick arrives late by
//! exactly the time it was kept off the CPU. So the gate credits the AppCpu
//! with the excess over [`TICK_MS`] of its own scheduling delay. What is
//! left — the heartbeat frozen while this task keeps ticking on time — is
//! precisely a wedged AppCpu and nothing else. The fence's own
//! `feed_from_fence` keeps the RWDT alive across those bursts as before.
//!
//! The counter is bumped by exactly one writer (the render task) and read by
//! exactly one reader, so it needs no ordering beyond being an atomic; the
//! clock is the ProCpu's, which keeps the whole judgement in one time domain
//! (and keeps RTC memory — which the AppCpu must not touch, see `core1.rs` —
//! out of the hot path).

/// How often `core1::watchdog_task` wakes to feed the RWDT. Also the
/// baseline against which its own lateness is measured, so a change here
/// changes what counts as "the ProCpu was blocked".
pub const TICK_MS: u32 = 3_000;

/// A heartbeat this stale (with the ProCpu awake for all of it) is a wedge.
/// Must clear the slowest legitimate single iteration with margin and still
/// leave room under `core1::WATCHDOG_SECS` (20 s) for the RWDT to expire:
/// worst-case recovery is `STALL_LIMIT_MS` + one tick + 20 s.
pub const STALL_LIMIT_MS: u32 = 10_000;

/// What the watchdog task should do this tick.
#[cfg_attr(test, derive(Debug))]
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Tick {
    /// Feed the RWDT — the AppCpu is alive, or there is nothing to judge.
    Feed,
    /// Stop feeding: the render loop has not iterated in `stalled_ms` of
    /// ProCpu-awake time. The RWDT expires and resets the board.
    Stall { stalled_ms: u32 },
}

/// The gate's state, owned by `core1::watchdog_task`.
#[cfg_attr(test, derive(Debug))]
pub struct StallWatch {
    /// The render task has stamped at least once. Until then there is
    /// nothing to judge: the task may not be spawned yet, may never be
    /// (`LUXEL_QUIET`), or may have fallen back to the ProCpu — where a
    /// wedge stops this task too and the RWDT catches it the old way.
    armed: bool,
    /// Heartbeat value as of the last tick that saw it move.
    beat: u32,
    /// When that was (ProCpu clock, ms).
    beat_ms: u32,
    /// When this task last ticked.
    tick_ms: u32,
    /// Time since `beat_ms` this task was itself kept off the CPU — a
    /// parked core or a blocked executor, i.e. time the AppCpu could not
    /// have made progress in either. Subtracted from the stall age.
    blocked_ms: u32,
    /// The trip is one-way: once the RWDT is being starved on purpose,
    /// nothing re-arms it short of the reset.
    tripped: bool,
}

impl StallWatch {
    pub const fn new() -> StallWatch {
        StallWatch {
            armed: false,
            beat: 0,
            beat_ms: 0,
            tick_ms: 0,
            blocked_ms: 0,
            tripped: false,
        }
    }

    /// One watchdog tick: `now_ms` is a monotonic ProCpu clock in ms,
    /// `beat` the render loop's iteration counter (0 = it has never run).
    ///
    /// Everything here is 32-bit WRAPPING millisecond arithmetic — the
    /// chip is 32-bit, and every interval measured is seconds long against
    /// a counter that wraps every 49 days, so `wrapping_sub` is exact
    /// where 64-bit math would only be bigger (the clock itself comes from
    /// `Instant::now()` truncated the same way `netin.rs` truncates it).
    pub fn tick(&mut self, now_ms: u32, beat: u32) -> Tick {
        let prev_tick = core::mem::replace(&mut self.tick_ms, now_ms);
        if self.tripped {
            return Tick::Stall {
                stalled_ms: now_ms.wrapping_sub(self.beat_ms),
            };
        }
        if !self.armed {
            // `beat` wraps eventually, but 0 can only mean "not yet
            // stamped" here: arming is sticky, so a wrap past 0 lands in
            // the armed path below.
            if beat != 0 {
                self.armed = true;
                self.beat = beat;
                self.beat_ms = now_ms;
            }
            return Tick::Feed;
        }
        // Credit the excess of our own scheduling delay before judging the
        // heartbeat against it (see the module note).
        self.blocked_ms = self
            .blocked_ms
            .saturating_add(now_ms.wrapping_sub(prev_tick).saturating_sub(TICK_MS));
        if beat != self.beat {
            self.beat = beat;
            self.beat_ms = now_ms;
            self.blocked_ms = 0;
            return Tick::Feed;
        }
        let stalled = now_ms
            .wrapping_sub(self.beat_ms)
            .saturating_sub(self.blocked_ms);
        if stalled >= STALL_LIMIT_MS {
            self.tripped = true;
            Tick::Stall { stalled_ms: stalled }
        } else {
            Tick::Feed
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Drive on-time ticks with a given heartbeat behaviour, returning the
    /// first non-`Feed` verdict and the clock it came at.
    fn run(
        w: &mut StallWatch,
        t0: u32,
        ticks: usize,
        mut beat: impl FnMut(usize) -> u32,
    ) -> Option<(u32, Tick)> {
        for i in 0..ticks {
            let now = t0 + i as u32 * TICK_MS;
            let v = w.tick(now, beat(i));
            if v != Tick::Feed {
                return Some((now, v));
            }
        }
        None
    }

    /// The first on-time tick at or after `t`, counting from `t0`.
    fn tick_at_or_after(t0: u32, t: u32) -> u32 {
        t0 + t.saturating_sub(t0).div_ceil(TICK_MS) * TICK_MS
    }

    #[test]
    fn never_armed_never_trips() {
        // LUXEL_QUIET, or the render task not spawned yet: a heartbeat of 0
        // forever must feed forever — there is no AppCpu loop to judge.
        let mut w = StallWatch::new();
        assert_eq!(run(&mut w, 0, 200, |_| 0), None);
    }

    #[test]
    fn healthy_loop_feeds() {
        let mut w = StallWatch::new();
        assert_eq!(run(&mut w, 1_000, 200, |i| i as u32 + 1), None);
    }

    #[test]
    fn slow_pattern_feeds() {
        // The heartbeat only has to move once per STALL_LIMIT_MS; a frame
        // an order of magnitude slower than the panel's worst (ripples-2d,
        // 234 ms) still feeds.
        let mut w = StallWatch::new();
        let per_tick = (STALL_LIMIT_MS / TICK_MS) as usize;
        assert_eq!(run(&mut w, 0, 400, |i| (i / per_tick) as u32 + 1), None);
    }

    #[test]
    fn idle_with_no_pattern_feeds() {
        // A rejected pattern renders nothing (out_fps 0) but the loop still
        // iterates every 50 ms — the gate is on iterations, not frames.
        let mut w = StallWatch::new();
        assert_eq!(run(&mut w, 0, 200, |i| i as u32 * 60 + 1), None);
    }

    #[test]
    fn wedged_loop_trips_just_past_the_limit() {
        let mut w = StallWatch::new();
        let (at, v) = run(&mut w, 0, 100, |_| 7).expect("must trip");
        assert_eq!(at, tick_at_or_after(0, STALL_LIMIT_MS));
        assert!(matches!(v, Tick::Stall { stalled_ms } if stalled_ms >= STALL_LIMIT_MS));
    }

    #[test]
    fn a_trip_is_latched() {
        let mut w = StallWatch::new();
        run(&mut w, 0, 100, |_| 7).expect("must trip");
        // Even a heartbeat that came back must not re-arm the feed: the
        // RWDT reset is already the only way out.
        for i in 0..10 {
            assert!(matches!(
                w.tick(100_000 + i * TICK_MS, 8 + i as u32),
                Tick::Stall { .. }
            ));
        }
    }

    #[test]
    fn a_blocked_procpu_is_forgiven_in_full() {
        // A garbage-collecting pattern save blocks the ProCpu executor for
        // 25 s in one blocking call (Gitea #292) — this task does not tick
        // at all, and on a pipelined board the render loop is waiting on
        // pipeline::output_task, which lives on that blocked executor. The
        // late tick must be credited in full.
        let mut w = StallWatch::new();
        assert_eq!(w.tick(0, 5), Tick::Feed);
        assert_eq!(w.tick(25_000, 5), Tick::Feed);
        // and a second one back to back (an OTA's erase bursts)
        assert_eq!(w.tick(55_000, 5), Tick::Feed);
        // the loop comes back as soon as the burst ends
        assert_eq!(w.tick(58_000, 6), Tick::Feed);
        assert_eq!(run(&mut w, 61_000, 100, |i| i as u32 + 7), None);
    }

    #[test]
    fn awake_time_between_late_ticks_still_accumulates() {
        // The credit is the EXCESS over one tick interval, not the whole
        // gap: a ProCpu that keeps coming back to its executor for a tick's
        // worth of idle time was awake for that long, and an AppCpu that
        // never moved across enough of those windows is wedged after all.
        let mut w = StallWatch::new();
        assert_eq!(w.tick(0, 5), Tick::Feed);
        let mut t = 0;
        let mut tripped = None;
        for _ in 0..20 {
            t += 3 * TICK_MS; // 6 s late, so 3 s of awake time each
            if let Tick::Stall { .. } = w.tick(t, 5) {
                tripped = Some(t);
                break;
            }
        }
        // 10 s of awake time = the 4th such window
        assert_eq!(tripped, Some(4 * 3 * TICK_MS));
    }

    #[test]
    fn a_wedge_after_a_burst_still_trips() {
        let mut w = StallWatch::new();
        assert_eq!(w.tick(0, 5), Tick::Feed); // armed
        assert_eq!(w.tick(25_000, 5), Tick::Feed); // burst, forgiven (3 s awake)
        // executor healthy again, heartbeat still frozen: 7 s of awake time
        // left to run before the limit.
        let t0 = 25_000 + TICK_MS;
        let (at, _) = run(&mut w, t0, 100, |_| 5).expect("must trip");
        assert_eq!(at, tick_at_or_after(t0, 25_000 + (STALL_LIMIT_MS - TICK_MS)));
    }

    #[test]
    fn the_credit_resets_when_the_loop_moves() {
        // A burst's credit must not bank against a later wedge.
        let mut w = StallWatch::new();
        assert_eq!(w.tick(0, 1), Tick::Feed);
        assert_eq!(w.tick(60_000, 2), Tick::Feed); // 57 s of credit, then progress
        let t0 = 60_000 + TICK_MS;
        let (at, _) = run(&mut w, t0, 100, |_| 2).expect("must trip");
        assert_eq!(at, tick_at_or_after(t0, 60_000 + STALL_LIMIT_MS));
    }

    #[test]
    fn recovery_fits_under_the_rwdt() {
        // The whole point: detection plus the RWDT's own 20 s is a bounded
        // reboot, not a hands-on power cycle.
        assert!(STALL_LIMIT_MS + TICK_MS < 20_000);
    }
}
