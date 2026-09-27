//! Non-base scene layers rendered on the ProCpu (Gitea #842).
//!
//! A scene's frame used to be the SUM of its layer engines' frames plus the
//! compositing: `pattern_frame(i)` ran every engine in turn on the render
//! task, on the AppCpu, while the ProCpu — WiFi, the web pool and the HUB75
//! output task's 2.5–6 ms compose — sat mostly idle through a 50 ms base
//! frame. This module runs the NON-base pattern layers on the ProCpu, in
//! parallel with the base engine, so a scene costs ~max(layer) + compose
//! instead of Σ layers + compose (docs/boards.md "Engine frames in PSRAM":
//! Aurora 51.7 + `_Fairies` 17.7 + 2.6 = 72.0 ms measured, the sum).
//!
//! ## Shape
//!
//! esp-rtos at the pinned revision exposes no way to spawn a preemptive
//! thread with its own stack (only `start`, `start_second_core`, the
//! thread-mode embassy `Executor` and an `InterruptExecutor`), and its idle
//! hook restarts from the top on every context switch. An `InterruptExecutor`
//! would run a 17 ms frame inside an interrupt — above every WiFi thread and
//! in a context the PSRAM/flash rules forbid (psram.rs "Flash fence"). So the
//! core-0 half is an ordinary embassy task on the ProCpu's main executor,
//! [`layer_task`], and it renders in SLICES: [`Engine::frame_begin`] once,
//! then [`Engine::frame_step`] of [`CHUNK`] pixels with a `yield_now`
//! between them, so the output task's compose, the network stack and the web
//! pool wait at most one chunk (~1 ms at `_Fairies`' 4.3 µs/px) rather than
//! a frame. A `renderFrame` layer is one VM call and cannot be sliced, so
//! [`post_slots`] leaves it on the render task ([`Engine::frame_chunkable`]).
//!
//! ## Protocol
//!
//! One job at a time, lock-free, owned by the render task:
//!
//! 1. [`post_slots`] (render task, inside `scenes::Runtime::render`) writes
//!    the chunkable pattern engines' addresses, the count and this frame's
//!    `delta` into the statics, bumps [`SEQ`] (Release) and signals [`WAKE`].
//! 2. [`layer_task`] (ProCpu) wakes, reads [`SEQ`] (Acquire), renders each
//!    engine's frame in chunks, then stores [`DONE`] = seq (Release).
//! 3. The render task's `pattern_frame(i)` for an offloaded slot calls
//!    [`join`] — a spin on `DONE == seq` — and reads `Engine::pixels()`;
//!    `Runtime::render` joins unconditionally before returning, so the job
//!    is always complete when `render` returns.
//!
//! Between steps 1 and 2/3 the render task does not touch those engines —
//! not `frame`, not `take_error`, not `for_each_engine`, not a drop. The
//! guards are in scenes.rs: every such access calls [`quiesce`] first, and
//! a `Runtime` whose job never completes LEAKS its engines rather than
//! freeing memory another core is executing in.
//!
//! The spin in [`join`] is task context with interrupts on, so the flash
//! fence's park request (core1.rs) still lands: a fenced write on the
//! ProCpu parks the spinning render task, finishes, and the spin resumes.
//! The layer task runs VM code from flash-mapped words and PSRAM in task
//! context, which is the one context the fence rules allow; a fence raised
//! from the render task parks the ProCpu — mid-chunk if need be — exactly
//! as it parks a web task today.
//!
//! [`join`] gives up after [`JOIN_TIMEOUT`] and counts a stall
//! (`/api/status` `core1.layer_core0[1]`); the layer draws nothing that
//! frame and the next frame tries to quiesce before posting again. A stall
//! is a bug, never expected — the AppCpu heartbeat stops during a spin, so
//! a stall that never clears is a watchdog reboot with `core1.last` set.
//!
//! ## The JIT's depth guard
//!
//! `jit::stack_limit()` is computed on the render task for the AppCpu's
//! stack. An engine running here runs on the ProCpu main stack, whose
//! addresses have nothing to do with that floor, so [`layer_task`] re-points
//! the guard at THIS stack before every frame ([`Engine::set_native_stack_limit`]).
//!
//! Built only under `cfg(layer_core0)` = `multi_core` + the `layer-core0`
//! cargo feature (build.rs). Not in any board's default set until the
//! panel has measured it: `EXTRA_FEATURES=layer-core0 BOARD=board-seengreat-hub75
//! firmware/build-esp32.sh` is the A/B build.

use core::sync::atomic::{AtomicI32, AtomicPtr, AtomicU32, AtomicUsize, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;

use crate::scenes::Slot;

/// Engines one job can carry — every pattern layer of a scene but the base.
pub const MAX: usize = luxel_core::caps::MAX_LAYERS as usize;
/// Pixels per `frame_step` between yields. 256 px is ~1.1 ms at
/// `_Fairies`' 4.3 µs/px on the S3, which is under the output task's own
/// compose time; a cheaper pattern yields more often than it needs to.
pub const CHUNK: u32 = 256;
/// How long the render task waits for a job before calling it a stall.
pub const JOIN_TIMEOUT: Duration = Duration::from_millis(2_000);

static ENGINES: [AtomicPtr<Engine>; MAX] = [const { AtomicPtr::new(core::ptr::null_mut()) }; MAX];
static COUNT: AtomicUsize = AtomicUsize::new(0);
static DELTA: AtomicI32 = AtomicI32::new(0);
/// The last job posted. `SEQ != DONE` ⇔ a job is outstanding.
static SEQ: AtomicU32 = AtomicU32::new(0);
/// The last job completed.
static DONE: AtomicU32 = AtomicU32::new(0);
static WAKE: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// Jobs completed / joins that timed out, for `/api/status`.
static FRAMES: AtomicU32 = AtomicU32::new(0);
static STALLS: AtomicU32 = AtomicU32::new(0);

/// A job is posted and not yet complete.
pub fn pending() -> bool {
    SEQ.load(Ordering::Acquire) != DONE.load(Ordering::Acquire)
}

/// Wait for job `seq` to complete. `false` = it did not within
/// [`JOIN_TIMEOUT`] (counted as a stall); the caller must then treat every
/// engine of that job as still in use.
pub fn join(seq: u32) -> bool {
    if DONE.load(Ordering::Acquire) == seq {
        return true;
    }
    let t0 = Instant::now();
    loop {
        if DONE.load(Ordering::Acquire) == seq {
            return true;
        }
        if t0.elapsed() > JOIN_TIMEOUT {
            STALLS.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        core::hint::spin_loop();
    }
}

/// Wait for whatever job is outstanding, if any. `false` = a stalled job
/// still holds its engines.
pub fn quiesce() -> bool {
    join(SEQ.load(Ordering::Acquire))
}

/// `(frames rendered here, joins that timed out)`.
pub fn stats() -> (u32, u32) {
    (FRAMES.load(Ordering::Relaxed), STALLS.load(Ordering::Relaxed))
}

/// Hand this frame's chunkable pattern layers to the ProCpu. Returns the
/// bitmask of slot indices the job covers (bit `i` = `slots[i]`) and the
/// job's sequence number. `(mask, None)` with a non-zero mask means those
/// slots belong to a job that has NOT completed (a stall from an earlier
/// frame): the caller must draw nothing for them and touch no engine.
///
/// Called by the render task only, once per `Runtime::render`.
pub fn post_slots(slots: &mut [Slot], delta: Fx) -> (u32, Option<u32>) {
    let mut all: u32 = 0;
    let mut mask: u32 = 0;
    let mut ptrs: [*mut Engine; MAX] = [core::ptr::null_mut(); MAX];
    let mut n = 0usize;
    for (i, s) in slots.iter_mut().enumerate().take(32) {
        if let Slot::Pattern(e) = s {
            all |= 1 << i;
            if n < MAX && e.frame_chunkable() {
                ptrs[n] = e as *mut Engine;
                n += 1;
                mask |= 1 << i;
            }
        }
    }
    // A job from an earlier frame that never completed still owns its
    // engines: nothing of this stack may run inline until it does.
    if pending() && !quiesce() {
        return (all, None);
    }
    if n == 0 {
        return (0, None);
    }
    for (slot, p) in ENGINES.iter().zip(ptrs.iter()) {
        slot.store(*p, Ordering::Relaxed);
    }
    COUNT.store(n, Ordering::Relaxed);
    DELTA.store(delta.raw(), Ordering::Relaxed);
    let seq = SEQ.load(Ordering::Relaxed).wrapping_add(1);
    SEQ.store(seq, Ordering::Release);
    WAKE.signal(());
    (mask, Some(seq))
}

/// Where the JIT's depth guard should fire on THIS stack: the same
/// budget-below-the-stack-pointer bound `jit::stack_limit` uses, taken on
/// the layer task rather than the render task.
#[cfg(feature = "jit")]
fn stack_limit() -> usize {
    let probe = 0u32;
    let here = &probe as *const u32 as usize;
    here.saturating_sub(crate::jit::STACK_BUDGET)
}

/// The ProCpu half: one job at a time, each engine's frame in [`CHUNK`]-pixel
/// slices with a yield between them.
#[embassy_executor::task]
pub async fn layer_task() -> ! {
    loop {
        WAKE.wait().await;
        let seq = SEQ.load(Ordering::Acquire);
        if DONE.load(Ordering::Relaxed) == seq {
            continue;
        }
        let n = COUNT.load(Ordering::Relaxed).min(MAX);
        let delta = Fx::from_raw(DELTA.load(Ordering::Relaxed));
        for slot in ENGINES.iter().take(n) {
            let p = slot.load(Ordering::Relaxed);
            if p.is_null() {
                continue;
            }
            // SAFETY: the render task published `p` with this job (module
            // doc, protocol step 1) and touches neither the engine nor the
            // `Vec` it lives in until it observes `DONE == seq`, which is
            // the last store below. The engine outlives the job: a
            // `Runtime` whose job has not completed leaks its engines
            // instead of dropping them (scenes.rs).
            let e = unsafe { &mut *p };
            #[cfg(feature = "jit")]
            e.set_native_stack_limit(stack_limit());
            if e.frame_begin(delta) {
                while !e.frame_step(CHUNK) {
                    embassy_futures::yield_now().await;
                }
            }
        }
        FRAMES.fetch_add(1, Ordering::Relaxed);
        DONE.store(seq, Ordering::Release);
    }
}
