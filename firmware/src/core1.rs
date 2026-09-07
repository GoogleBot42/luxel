//! Second-core bring-up and the cross-core flash fence (Gitea #259, #260).
//!
//! On dual-core chips (classic ESP32, ESP32-S3) the render task runs on the
//! AppCpu under its own esp-rtos scheduler and a thread-mode embassy
//! executor of its own, while WiFi, the network stack, the web pool and
//! every other task stay on the ProCpu. Single-core chips (C3/C6/S2/C2)
//! compile this module down to no-ops: [`fenced`] is the identity and
//! [`start`] does not exist. The `multi_core` cfg comes from build.rs
//! (features `esp32` / `esp32s3`) and mirrors esp-hal's private cfg of the
//! same name.
//!
//! ## Why the flash fence exists
//!
//! SPI flash is shared between the cache (SPI0 — every instruction fetch
//! from flash-resident code, on EITHER core) and the flash driver (SPI1 —
//! esp-storage's ROM calls). While an SPI1 op runs the other core must not
//! fetch from flash: during an erase/program the chip serves garbage, and
//! even a plain SPI1 read contends for the bus. ESP-IDF stalls the other
//! CPU for every op, reads included (`spi_flash_disable_interrupts_caches_
//! and_other_cpu`). esp-storage knows this too: its default multicore
//! strategy makes every write FAIL while the other core runs, and its
//! `multicore_auto_park` alternative hard-stalls the other core at a random
//! instruction — which can be inside a spinlock (scheduler, heap, any
//! critical section); the flash-writing core's next interrupt then spins on
//! that lock forever. That deadlock is why the hard park was not used.
//!
//! The fence is the cooperative version of ESP-IDF's stall. `fenced(op)`
//! asks the other core to park itself by raising that core's park software
//! interrupt (SWI2 parks the ProCpu, SWI3 the AppCpu — SWI0/1 belong to
//! esp-rtos). The park handler runs at the highest vectored priority, lives
//! in IRAM, and spins on a DRAM flag. Because every esp-sync critical
//! section masks interrupts, the handler can only run once its core holds
//! no spinlock — so the parked core holds nothing the flash op could need.
//! The fence itself is taken OUTSIDE the flash driver's critical section,
//! so two cores fencing at once resolve through the park handler (the
//! loser gets parked mid-spin, then proceeds) instead of deadlocking. The
//! contended path runs with interrupts still on until the fence lock is
//! won; from there — before the other core is even asked to park — the
//! fencing core masks its own interrupts until after the release (Gitea
//! #292), and `park_if_asked` in the spin-waits stands in for the park
//! interrupt it can no longer take.
//!
//! Every flash op goes through one of four fenced doors: `ota::with_flash`
//! (borrow-per-op, the normal path), `patterns::AsyncFlash` (the
//! sequential-storage adapter over a leased driver), `ota::begin`'s
//! partition-table reads on the taken driver, and `flashmap::quiesced`
//! (cache-MMU table programming + the whole-cache flush — Gitea #272: the
//! ESP32's DPORT MMU registers must not be read while the other core runs,
//! and a flush must not race a core executing from flash or reading a
//! mapping). A new flash path must use one of them or wrap itself in
//! [`fenced`] — .claude/rules/firmware.md. Mapped READS need nothing: a
//! task-context load on the other core either completed before the park
//! interrupt was taken or happens after the release.
//!
//! A fence *timeout* (the other core did not park within 100 ms; the op
//! proceeds anyway, `FENCE_TIMEOUTS`) risks a garbage code fetch AND a
//! garbage mapped read on that core — `/api/status` `core1.fence_timeouts`
//! is the tell for both, and must stay 0.
//!
//! What it took to make the park safe on the classic ESP32 (2026-09-05,
//! black-boxed hangs): the park handler runs at Priority1, not 3 (a
//! level-3 park landing inside a level-1 handler's DPORT/APB register
//! reads wedged the bus); the AppCpu never touches RTC memory inside the
//! park (its RTC access wedged the ProCpu's next one); and the fence waits
//! for the strip's SPI2 DMA transfer to finish before the SPI1 op (the SPI
//! hosts share the DMA engine — a flash op during an in-flight transfer
//! hangs the ProCpu, which single-core builds could never do because the
//! blocking DMA write held the only core). An RTC watchdog fed from the
//! ProCpu executor plus the RTC-memory black box (`core1.last` in
//! `/api/status`) turn any future wedge into a reboot with a diagnosis.

#[cfg(multi_core)]
mod imp {
    use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

    use embassy_executor::Spawner;
    use esp_hal::interrupt::software::SoftwareInterrupt;
    use esp_hal::interrupt::{InterruptHandler, Priority};
    use esp_hal::peripherals::CPU_CTRL;
    use esp_hal::system::Cpu;
    use esp_hal::time::{Duration, Instant};
    use esp_println::println;
    use static_cell::StaticCell;

    const PRO: usize = 0;
    const APP: usize = 1;

    /// Per-core "please park" request and "I am parked" acknowledgement,
    /// indexed by `Cpu as usize` (0 = ProCpu, 1 = AppCpu). Plain DRAM
    /// atomics: the park handler spins on them from IRAM.
    static PARK_REQ: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];
    static PARKED: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];
    /// That core's park handler is installed (set by the core itself).
    static PARKER_READY: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];
    /// Serializes fences from both cores.
    static FENCE: AtomicBool = AtomicBool::new(false);
    /// Times the other core failed to acknowledge a park within
    /// [`PARK_TIMEOUT`] (the op proceeded anyway). Nonzero = investigate.
    pub static FENCE_TIMEOUTS: AtomicU32 = AtomicU32::new(0);
    /// Longest a fence has waited for the park acknowledgement, in µs —
    /// the render-side latency cost of a flash op.
    pub static FENCE_MAX_WAIT_US: AtomicU32 = AtomicU32::new(0);

    pub use super::tag;

    /// Black box in RTC SLOW memory (both CPUs can reach it; RTC fast is
    /// ProCpu-only on the ESP32): persists across software and watchdog
    /// resets, so when the watchdog below catches a wedge, the boot after
    /// it can report what the fence was doing. Layout: [0] magic,
    /// [1] ProCpu fence phase, [2] AppCpu fence phase, [3] fences begun,
    /// [4] ProCpu parks, [5] AppCpu parks, [6] park-ack timeouts,
    /// [7] fences completed, [8] call-site tag of the fence in flight
    /// ([`tag`]), [9] the AppCpu park count the ProCpu saw when it entered
    /// that flash op (behind [3] means the AppCpu was NOT parked for it).
    /// Phases: 0 idle, 1 waiting for the fence lock, 2 waiting for the
    /// park ack, 3 inside the flash op, 4 waiting for the release ack,
    /// 6 inside the driver call (esp-storage and the ROM SPI1 routine),
    /// 7 that call returned. 6 and 7 refine 3; which side a wedge lands
    /// on is the whole diagnosis.
    #[esp_hal::ram(unstable(rtc_slow, persistent))]
    static mut BLACKBOX: [u32; 10] = [0; 10];
    const BB_MAGIC: u32 = 0x5EED_C0DE;

    #[inline(always)]
    fn bb_write(i: usize, v: u32) {
        unsafe { core::ptr::write_volatile(core::ptr::addr_of_mut!(BLACKBOX).cast::<u32>().add(i), v) }
    }
    #[inline(always)]
    fn bb_read(i: usize) -> u32 {
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!(BLACKBOX).cast::<u32>().add(i)) }
    }
    #[inline(always)]
    fn bb_bump(i: usize) {
        bb_write(i, bb_read(i).wrapping_add(1));
    }

    /// What the black box held at boot (copied out before it is re-armed)
    /// plus the reset reason — `/api/status` `core1.last`.
    static LAST: [AtomicU32; 10] = [const { AtomicU32::new(0) }; 10];
    /// Times the AppCpu entered [`park`] — DRAM (the AppCpu must not touch
    /// RTC memory inside the park), copied into the black box by the
    /// ProCpu at the moment it enters a flash op.
    static APP_PARKS: AtomicU32 = AtomicU32::new(0);
    static LAST_RESET: static_cell::StaticCell<alloc::string::String> = static_cell::StaticCell::new();
    static LAST_RESET_STR: core::sync::atomic::AtomicPtr<alloc::string::String> =
        core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

    /// Snapshot the black box from the previous run and re-arm it. Call
    /// once, early, on the ProCpu (heap up).
    pub fn boot_blackbox() {
        let reason = alloc::format!("{:?}", esp_hal::system::reset_reason());
        let valid = bb_read(0) == BB_MAGIC;
        for i in 0..10 {
            LAST[i].store(if valid { bb_read(i) } else { 0 }, Ordering::Relaxed);
        }
        for i in 1..10 {
            bb_write(i, 0);
        }
        bb_write(0, BB_MAGIC);
        let s = LAST_RESET.init(reason);
        LAST_RESET_STR.store(s as *mut _, Ordering::Release);
        println!(
            "core1: last reset {} — fence phases pro {} app {}, fences {}/{}, parks {}/{}, timeouts {}",
            unsafe { &*LAST_RESET_STR.load(Ordering::Acquire) },
            LAST[1].load(Ordering::Relaxed),
            LAST[2].load(Ordering::Relaxed),
            LAST[7].load(Ordering::Relaxed),
            LAST[3].load(Ordering::Relaxed),
            LAST[4].load(Ordering::Relaxed),
            LAST[5].load(Ordering::Relaxed),
            LAST[6].load(Ordering::Relaxed),
        );
    }

    /// `(reset reason, black box of the previous run)` for `/api/status`.
    pub fn last_run() -> (&'static str, [u32; 10]) {
        let p = LAST_RESET_STR.load(Ordering::Acquire);
        let reason = if p.is_null() { "?" } else { unsafe { (*p).as_str() } };
        let mut bb = [0u32; 10];
        for (i, v) in bb.iter_mut().enumerate() {
            *v = LAST[i].load(Ordering::Relaxed);
        }
        (reason, bb)
    }

    /// The live black box (this run): `(fences begun, completed)` as of
    /// now, for `/api/status`. ProCpu only (RTC memory).
    pub fn live_counters() -> (u32, u32) {
        (bb_read(3), bb_read(7))
    }

    /// RTC watchdog fed from the ProCpu executor: a wedged ProCpu (a fence
    /// that never releases, a web task spinning) resets the system instead
    /// of stranding the device, and the black box says what it was doing.
    pub const WATCHDOG_SECS: u64 = 20;
    pub fn arm_watchdog(rtc_timer: esp_hal::peripherals::RTC_TIMER<'static>) {
        use esp_hal::rtc_cntl::{Rtc, RwdtStage, RwdtStageAction};
        let mut rtc = Rtc::new(rtc_timer);
        rtc.rwdt.set_timeout(RwdtStage::Stage0, Duration::from_secs(WATCHDOG_SECS));
        rtc.rwdt.set_stage_action(RwdtStage::Stage0, RwdtStageAction::ResetSystem);
        rtc.rwdt.enable();
        // Leaked so both the watchdog task and the fence can feed it (the
        // Rwdt handle itself is not public). Feeds are register writes, so
        // the two paths cannot corrupt each other's state.
        RTC.store(alloc::boxed::Box::leak(alloc::boxed::Box::new(rtc)), Ordering::Release);
    }

    /// The armed watchdog (leaked in [`arm_watchdog`]), or null before it.
    static RTC: core::sync::atomic::AtomicPtr<esp_hal::rtc_cntl::Rtc<'static>> =
        core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

    /// Feed the RTC watchdog. No-op before [`arm_watchdog`].
    pub fn feed_watchdog() {
        let p = RTC.load(Ordering::Acquire);
        if !p.is_null() {
            // SAFETY: `p` points at the leaked Rtc; `feed` only writes the
            // RWDT feed/protect registers, so a racing feed from the other
            // path is the same write, not a torn structure.
            unsafe { (*p).rwdt.feed() };
        }
    }

    /// Fences since the last watchdog feed from the fence.
    static SINCE_FEED: AtomicU32 = AtomicU32::new(0);
    /// Feed every this many fences — ~10 ms of flash ops at the worst
    /// observed per-fence cost, so the window stays far under
    /// [`WATCHDOG_SECS`] while costing three register writes per 64 ops.
    const FEED_EVERY: u32 = 64;

    /// Feed the RTC watchdog from inside the fence.
    ///
    /// The watchdog task feeds every 3 s from the ProCpu executor, but a
    /// long flash burst BLOCKS that executor: a 728 KB `POST /api/assets`
    /// is ~15 s of sector erases, and a pattern save that garbage-collects
    /// the store was measured at 18.5 s (Gitea #292) — both close enough
    /// to the 20 s timeout to reboot the board mid-write for making slow
    /// progress rather than for being wedged. Taking a fence IS progress
    /// (a wedged core stops taking them), so feeding here keeps the
    /// watchdog's real job — catching a core that stopped — while it stops
    /// punishing work that is merely long.
    #[inline(always)]
    fn feed_from_fence() {
        if SINCE_FEED.fetch_add(1, Ordering::Relaxed) + 1 < FEED_EVERY {
            return;
        }
        SINCE_FEED.store(0, Ordering::Relaxed);
        feed_watchdog();
    }

    #[embassy_executor::task]
    pub async fn watchdog_task() -> ! {
        loop {
            feed_watchdog();
            embassy_time::Timer::after_secs(3).await;
        }
    }

    /// A core that cannot park within this window is presumed wedged; the
    /// flash op proceeds rather than hanging the requesting core forever.
    /// A healthy core answers in microseconds (interrupt latency plus at
    /// most one critical section).
    const PARK_TIMEOUT: Duration = Duration::from_millis(100);

    /// Spin, parked, until the request clears. IRAM (`#[ram]` implies
    /// `#[inline(never)]`, so a flash-resident caller can't absorb it):
    /// nothing may fetch from flash while parked.
    #[esp_hal::ram]
    fn park(core: usize) {
        // Mask the vectored levels while parked (the handler itself runs at
        // level 1 — see install_*_parker for why not 3) so nothing on this
        // core can run flash-resident code until the fence is released.
        let mask = esp_hal::xtensa_lx::interrupt::disable();
        // The black box lives in RTC memory, which the AppCpu must not
        // touch here: an AppCpu RTC-memory access inside the park wedged the
        // ProCpu's next RTC access (black-boxed, mode-isolated 2026-09-05).
        if core == PRO {
            bb_bump(4);
        } else {
            APP_PARKS.fetch_add(1, Ordering::Relaxed);
        }
        PARKED[core].store(true, Ordering::SeqCst);
        while PARK_REQ[core].load(Ordering::SeqCst) {}
        PARKED[core].store(false, Ordering::SeqCst);
        unsafe { esp_hal::xtensa_lx::interrupt::set_mask(mask) };
    }

    #[esp_hal::ram]
    extern "C" fn park_pro_handler() {
        unsafe { SoftwareInterrupt::<'static, 2>::steal() }.reset();
        park(PRO);
    }

    #[esp_hal::ram]
    extern "C" fn park_app_handler() {
        unsafe { SoftwareInterrupt::<'static, 3>::steal() }.reset();
        park(APP);
    }

    /// Install the ProCpu's park handler. Call from the ProCpu, after
    /// `esp_rtos::start`, before the second core starts.
    pub fn install_pro_parker(mut irq: SoftwareInterrupt<'static, 2>) {
        debug_assert_eq!(Cpu::current(), Cpu::ProCpu);
        // Priority1, not 3: a level-3 park can land inside a level-1
        // handler mid-way through its DPORT/APB register reads (esp-rtos's
        // task switch, esp-hal's interrupt dispatcher), and the ESP32's
        // interrupted-peripheral-read errata then wedge the bus bridge
        // for BOTH cores (measured: a hang within a minute under a heavy
        // pattern, black-boxed as "ProCpu inside the op"). At level 1 the
        // park can only preempt thread-mode code; park() raises the mask
        // itself once inside.
        irq.set_interrupt_handler(InterruptHandler::new(park_pro_handler, Priority::Priority1));
        PARKER_READY[PRO].store(true, Ordering::SeqCst);
    }

    fn install_app_parker(mut irq: SoftwareInterrupt<'static, 3>) {
        debug_assert_eq!(Cpu::current(), Cpu::AppCpu);
        irq.set_interrupt_handler(InterruptHandler::new(park_app_handler, Priority::Priority1));
        PARKER_READY[APP].store(true, Ordering::SeqCst);
    }

    #[inline(always)]
    fn raise_park(core: usize) {
        if core == PRO {
            unsafe { SoftwareInterrupt::<'static, 2>::steal() }.raise();
        } else {
            unsafe { SoftwareInterrupt::<'static, 3>::steal() }.raise();
        }
    }

    /// Cooperative park while we wait inside a fence: if the other core's
    /// fence asked us to park and our interrupts are masked (a caller
    /// fencing from inside a critical section), the handler can't run — so
    /// honor the request here instead of deadlocking on it.
    #[inline(always)]
    fn park_if_asked(me: usize) {
        if PARK_REQ[me].load(Ordering::SeqCst) {
            park(me);
        }
    }

    /// A held fence: the other core is parked (or was found not running).
    /// Out-of-line on purpose — `fenced` is instantiated at every
    /// `with_flash` call site, and inlining the spin-waits there cost
    /// ~1 KB of image per site (measured: +25 KB on the Athom).
    struct Fence {
        /// Index of the core we parked; `None` = nothing to release.
        parked: Option<usize>,
        /// This core's interrupt mask, saved by [`Fence::acquire`] and
        /// restored by `drop` — held across the op AND the release. Kept
        /// in the guard rather than in `fenced_as` on purpose: `fenced_as`
        /// is `#[inline(always)]` and instantiated at every flash call
        /// site, so anything living there is paid for ~20 times over
        /// (measured: 2.6 KB of image).
        mask: u32,
    }

    impl Fence {
        #[inline(never)]
        fn acquire(tag: usize) -> Fence {
            feed_from_fence();
            let me = Cpu::current() as usize;
            let other = 1 - me;
            // No `is_running()` check here on purpose: it is a DPORT read on
            // every fence, and the ESP32's DPORT-read erratum makes a racing
            // read from the other core return garbage — a wrong "not
            // running" answer would silently skip the park. The parker
            // flag is our own DRAM state and the second core never stops.
            if !PARKER_READY[other].load(Ordering::Acquire) {
                return Fence { parked: None, mask: 0 };
            }
            bb_write(1 + me, 1);
            while FENCE
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                park_if_asked(me);
            }
            bb_bump(3);
            // Interrupts OFF on this core from HERE — before the other
            // core is asked to park — until after it is released again.
            // See the note in `drop`. `park_if_asked` in the spin-waits
            // below is what keeps that safe: with the mask up we cannot
            // take our own park interrupt, so we honour the request
            // inline instead.
            let mask = esp_hal::xtensa_lx::interrupt::disable();
            bb_write(1 + me, 2);
            PARK_REQ[other].store(true, Ordering::SeqCst);
            raise_park(other);
            let t0 = Instant::now();
            let mut acked = true;
            while !PARKED[other].load(Ordering::SeqCst) {
                park_if_asked(me);
                if t0.elapsed() > PARK_TIMEOUT {
                    acked = false;
                    break;
                }
            }
            let waited = t0.elapsed().as_micros() as u32;
            if acked {
                FENCE_MAX_WAIT_US.fetch_max(waited, Ordering::Relaxed);
            } else {
                FENCE_TIMEOUTS.fetch_add(1, Ordering::Relaxed);
                bb_bump(6);
            }
            // The parked core executes nothing, but its cache controller can
            // still have a fill in flight from the instruction it was
            // interrupted on; ESP-IDF waits for that cache to go idle and
            // disables it before touching SPI1 (spi_flash_disable_cache),
            // and so do we, via the ROM helper. Re-enabled (with a flush) in
            // Drop before the core is released.
            // The parked core may have been stopped mid-frame with the
            // strip's SPI2 DMA transfer still running. On the classic ESP32
            // the SPI hosts share the SPI DMA engine, and a ROM SPI1 flash op
            // issued while that transfer is in flight wedges the ProCpu (hard
            // hang, black-boxed 6/6 as "ProCpu inside the op"; a fence that
            // only parks — no flash op — ran clean, and this wait made the
            // same trigger run clean). Single-core builds never hit it because
            // the blocking DMA write held the only core. Wait for the output
            // driver's transfer to finish (bounded — a frame at most).
            //
            // Asked of EITHER core, not just the AppCpu: which core runs the
            // output driver is a build-time question (pipeline.rs moves it to
            // the ProCpu where the board pipelines), and the check is one
            // register read that reads `false` on every board whose driver
            // cannot have a transfer in flight.
            if acked {
                let t = Instant::now();
                while crate::output::transfer_busy() {
                    if t.elapsed() > PARK_TIMEOUT {
                        FENCE_TIMEOUTS.fetch_add(1, Ordering::Relaxed);
                        bb_bump(6);
                        break;
                    }
                }
            }
            bb_write(8, tag as u32);
            bb_write(9, APP_PARKS.load(Ordering::Relaxed));
            bb_write(1 + me, 3);
            Fence {
                parked: Some(other),
                mask,
            }
        }
    }

    impl Drop for Fence {
        #[inline(never)]
        fn drop(&mut self) {
            let Some(other) = self.parked else { return };
            // This core's interrupts have been masked since `acquire` took
            // the fence lock, and stay masked until the other core is
            // running again — the half of ESP-IDF's
            // spi_flash_disable_interrupts_caches_and_other_cpu() the
            // fence was missing (Gitea #292). Not "so the handler can't
            // fetch from flash": the two measured wedges land where this
            // core RE-ENABLES interrupts while the other core is still
            // parked — once on the instruction where esp-storage's own
            // critical section drops back to level 0 after a 45 ms sector
            // erase (black-box phase 7), once while waiting for the park
            // acknowledgement (phase 2). Everything that queued up behind
            // the op fires at once, and one of those handlers never
            // returns; the fence is never released and the RTC watchdog
            // reboots the board 20 s later. Deferring them all past the
            // release turned a 5/5 wedging 728 KB `POST /api/assets` into
            // 5/5 clean and a 6/10 pattern-save soak into 10/10.
            //
            // Cost: this core's interrupt latency for the length of one
            // flash op. esp-storage already masks levels 1-5 for the ROM
            // call itself, so the extra exposure is level 6+ across the op
            // plus the park round-trip. The OTHER core pays nothing new —
            // it is parked for exactly the same window.
            let mask = self.mask;
            let me = 1 - other;
            bb_write(1 + me, 4);
            PARK_REQ[other].store(false, Ordering::SeqCst);
            // Wait for the release: the next fence's raise must not land
            // while this handler is still on its way out (its reset() would
            // eat it).
            let t1 = Instant::now();
            while PARKED[other].load(Ordering::SeqCst) {
                if t1.elapsed() > PARK_TIMEOUT {
                    break;
                }
            }
            FENCE.store(false, Ordering::Release);
            bb_write(1 + me, 0);
            bb_bump(7);
            unsafe { esp_hal::xtensa_lx::interrupt::set_mask(mask) };
        }
    }

    /// Run `f` (a flash driver operation) with the other core parked. No-op
    /// until the second core runs its park handler, so boot-time flash ops
    /// before [`start`] need nothing.
    #[inline(always)]
    pub fn fenced<R>(f: impl FnOnce() -> R) -> R {
        fenced_as(tag::OTHER, f)
    }

    /// [`fenced`], attributing the fence to a call site ([`tag`]).
    #[inline(always)]
    pub fn fenced_as<R>(tag: usize, f: impl FnOnce() -> R) -> R {
        let fence = Fence::acquire(tag);
        let r = f();
        drop(fence);
        r
    }

    /// Record a fence sub-phase for this core in the black box. Phases 6
    /// and 7 live inside phase 3 ("inside the flash op") and split it at
    /// the driver call: 6 = inside esp-storage and the ROM SPI1 routine,
    /// 7 = it returned and the fence is winding down. Which side a wedge
    /// lands on is the whole diagnosis (Gitea #292 was 7 — the op had
    /// finished).
    #[inline(never)]
    pub fn bb_phase(p: u32) {
        bb_write(1 + Cpu::current() as usize, p);
    }

    /// The AppCpu main-thread stack. Heap-leaked at boot rather than a
    /// static: a static would come straight out of the leftover-DRAM main
    /// stack (docs/firmware.md "Stack & heap invariants"), and every board
    /// would need its own heap arithmetic to compensate. The render path
    /// it carries measures ~11 KB deep at worst (render_task frame 4.1 KB +
    /// esp-storage's 4 KB read bounce under a pattern-store load, per
    /// tools/stack-check.sh) plus interrupt frames; the AppCpu takes no
    /// WiFi NMIs. `/api/status` reports the high-water mark (`core1_stack`).
    pub const STACK_BYTES: usize = 20 * 1024;
    type CoreStack = esp_hal::system::Stack<STACK_BYTES>;
    /// Fill pattern for the high-water scan.
    const FILL: u8 = 0xA5;
    /// Bytes at the bottom the scan skips: esp-hal's stack guard word and
    /// the entry closure it copies just above it.
    const SKIP: usize = 256;
    static STACK_BASE: AtomicUsize = AtomicUsize::new(0);

    /// Start the scheduler on the AppCpu and run `init` on a thread-mode
    /// executor pinned there. `Err(init)` hands the closure back if the
    /// stack can't be allocated, so the caller can spawn on the ProCpu
    /// instead. Returns once the second core's park handler is armed, so
    /// no flash op on the ProCpu can race the core's flash-resident startup.
    pub fn start<F>(
        cpu_ctrl: CPU_CTRL<'static>,
        rtos_irq: SoftwareInterrupt<'static, 1>,
        park_irq: SoftwareInterrupt<'static, 3>,
        init: F,
    ) -> Result<(), F>
    where
        F: FnOnce(Spawner) + Send + 'static,
    {
        // alloc + cast, never Box::new(Stack::new()): that would build the
        // 20 KB value on the (main) stack first.
        let layout = core::alloc::Layout::new::<CoreStack>();
        let p = unsafe { alloc::alloc::alloc(layout) }.cast::<CoreStack>();
        if p.is_null() {
            return Err(init);
        }
        unsafe { core::ptr::write_bytes(p.cast::<u8>(), FILL, STACK_BYTES) };
        STACK_BASE.store(p as usize, Ordering::Release);
        let stack: &'static mut CoreStack = unsafe { &mut *p };

        esp_rtos::start_second_core(cpu_ctrl, rtos_irq, stack, move || {
            install_app_parker(park_irq);
            static EXECUTOR: StaticCell<esp_rtos::embassy::Executor> = StaticCell::new();
            let executor = EXECUTOR.init(esp_rtos::embassy::Executor::new());
            executor.run(init)
        });
        while !PARKER_READY[APP].load(Ordering::Acquire) {}
        println!("core1: AppCpu scheduler up, {} B stack, flash fence armed", STACK_BYTES);
        Ok(())
    }

    /// `(park-ack timeouts, longest park wait in µs)` for `/api/status`.
    pub fn fence_stats() -> (u32, u32) {
        (
            FENCE_TIMEOUTS.load(Ordering::Relaxed),
            FENCE_MAX_WAIT_US.load(Ordering::Relaxed),
        )
    }

    /// `(used, total)` bytes of the AppCpu stack, from the fill-pattern
    /// high-water mark (a lower bound: the bottom [`SKIP`] bytes are not
    /// scanned). `None` before [`start`] / on a single-core build.
    pub fn stack_high_water() -> Option<(u32, u32)> {
        let base = STACK_BASE.load(Ordering::Acquire);
        if base == 0 {
            return None;
        }
        let s = unsafe { core::slice::from_raw_parts(base as *const u8, STACK_BYTES) };
        let untouched = s[SKIP..].iter().take_while(|&&b| b == FILL).count();
        Some(((STACK_BYTES - SKIP - untouched) as u32, STACK_BYTES as u32))
    }
}

#[cfg(multi_core)]
pub use imp::*;

/// Call-site tags for [`fenced_as`] / [`crate::ota::with_flash_as`], so
/// `/api/status` can say WHOSE fences these are. Append-only: the status
/// `core1.last.bb[8]` reports it as a number; docs/api.md names them.
pub mod tag {
    pub const OTHER: usize = 0;
    pub const ASSET_ERASE: usize = 1;
    pub const ASSET_WRITE: usize = 2;
    pub const ASSET_READ: usize = 3;
    pub const OTA_ERASE: usize = 4;
    pub const OTA_WRITE: usize = 5;
    pub const STORE_READ: usize = 6;
    pub const STORE_ERASE: usize = 7;
    pub const STORE_WRITE: usize = 8;
    pub const RAW_ERASE: usize = 9;
    pub const RAW_WRITE: usize = 10;
    pub const MAP: usize = 11;
    pub const COUNT: usize = 12;
}

/// Single-core: no second core, nothing to fence.
#[cfg(not(multi_core))]
#[inline(always)]
pub fn fenced<R>(f: impl FnOnce() -> R) -> R {
    f()
}

#[cfg(not(multi_core))]
pub fn stack_high_water() -> Option<(u32, u32)> {
    None
}

#[cfg(not(multi_core))]
pub fn fence_stats() -> (u32, u32) {
    (0, 0)
}

#[cfg(not(multi_core))]
pub fn last_run() -> (&'static str, [u32; 10]) {
    ("", [0; 10])
}

/// Single-core: no fence, so nothing to attribute.
#[cfg(not(multi_core))]
#[inline(always)]
pub fn fenced_as<R>(_tag: usize, f: impl FnOnce() -> R) -> R {
    f()
}

#[cfg(not(multi_core))]
pub fn live_counters() -> (u32, u32) {
    (0, 0)
}

#[cfg(not(multi_core))]
#[inline(always)]
pub fn bb_phase(_p: u32) {}
