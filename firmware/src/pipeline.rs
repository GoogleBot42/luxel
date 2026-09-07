//! The frame sink: where a rendered frame goes after the VM produced it
//! (Gitea #306).
//!
//! Two shapes live here, chosen by the `pipelined` cfg (build.rs: `hub75`
//! + `multi_core`), and the render task holds one of them:
//!
//! - [`DirectSink`] — the classic path, unchanged. The render task does
//!   the preview copy, the output pipeline and `write_frame` itself,
//!   inline, on whichever core it runs on. Every strip board and every
//!   single-core chip uses this.
//! - [`RenderSide`] + [`output_task`] — the pipelined path. The render
//!   task copies the finished RGB frame into a hand-off buffer and goes
//!   straight back to the VM; an `output_task` on the **ProCpu** does the
//!   preview copy, the output pipeline and the panel compose while the
//!   AppCpu is already rendering the next frame.
//!
//! ## Why only HUB75
//!
//! On the 64x64 panel the compose is a flat ~6 ms per frame at 4096 px
//! whatever the pattern does — it walks every pixel and sets seven
//! bitplane bits for it — while the AppCpu that runs it has nothing else
//! to do. Serially that is `vm + out` per frame; pipelined it is
//! `max(vm, out)`, which is ~24 % off a rainbow frame and more on the
//! cheap patterns. A strip is the opposite case: `write_frame` hands a
//! buffer to SPI DMA and the cost is on the wire, not the CPU, so a
//! second task would buy nothing and cost a frame buffer.
//!
//! ## The hand-off
//!
//! ONE frame buffer travels between the cores, in an embassy
//! `CriticalSectionRawMutex` cell (multicore-aware under esp-rtos),
//! announced by a `Signal`. The render task borrows it only for the copy:
//! it takes the buffer, fills it, publishes it, and holds nothing between
//! frames. Holding nothing is what keeps the count at one buffer instead
//! of two.
//!
//! ## Why it costs no RAM
//!
//! A pipeline needs one more live frame than a serial loop does, and at
//! 4096 px that is 12 KB the panel does not have to spare (the 2D snake
//! sat 8 KB above `RUNTIME_FLOOR` before this landed). It is paid for by
//! deleting the frame it replaces: `shared::PIXELS`, the snapshot
//! `GET /api/pixels` serves, was a second full copy of the frame that had
//! JUST been composed. The travelling buffer holds exactly that frame
//! whenever it is parked in the slot, so [`preview`] reads it there and
//! the pipelined build never calls `shared::set_pixels` at all. Net RAM:
//! zero, and one 12 KB memcpy per frame less.
//!
//! The render task never blocks on output. If the output task still holds
//! the buffer when a frame is ready, that frame is dropped — the same
//! best-effort contract `Hub75Output::write_frame` already has with the
//! DMA swap, and the same reason: the panel rescans autonomously, so a
//! dropped frame costs nothing but motion smoothness the panel could not
//! have shown anyway. If a frame is published while an earlier one is
//! still unclaimed, the newest wins.
//!
//! ## The flash fence
//!
//! Nothing here takes a flash op, and the two critical sections are a
//! handful of pointer moves, so the fence (core1.rs) is unaffected: a
//! park request lands between them at worst. The output task waits on a
//! `Signal`, never on a spin — so a fence held on the ProCpu (where flash
//! ops run) parks the AppCpu exactly as before, with the output task
//! simply not scheduled while the fencing code runs. The reverse — the
//! render task fencing from the AppCpu — parks the ProCpu, which may be
//! mid-compose; the compose is pure CPU + DRAM work with no peripheral
//! transaction in flight (HUB75's DMA is circular and autonomous), so
//! `output::transfer_busy()` stays `false` for it and the fence needs no
//! new wait.

use alloc::boxed::Box;
use alloc::vec::Vec;

use embassy_time::Instant;
use luxel_core::outpipe::GridMap;

use crate::output::{BoardOutput, OutputDriver};
use crate::shared;

/// The output pipeline's per-frame scratch and caches. Lives with whoever
/// runs `apply_outpipe` — the render task on the direct path, the output
/// task on the pipelined one.
struct PipeState {
    /// Scratch copy the outpipe stages work in (stays empty while every
    /// knob is off, which is the common case).
    buf: Vec<[u8; 3]>,
    gamma: (u8, Option<Box<[u8; 256]>>),
    /// (cooked-for epoch, luma -> color table); `u32::MAX` = never cooked.
    palette: (u32, Option<Box<[[u8; 3]; 256]>>),
}

impl PipeState {
    const fn new() -> Self {
        Self {
            buf: Vec::new(),
            gamma: (0, None),
            palette: (u32::MAX, None),
        }
    }

    /// Preview copy + output pipeline + driver write, timed.
    /// Returns `(pipe_us, out_us)`.
    fn run(
        &mut self,
        out: &mut BoardOutput,
        frame: &[[u8; 3]],
        grid: Option<GridMap>,
    ) -> (u32, u32) {
        let t0 = Instant::now();
        // The browser/e2e preview snapshot. On the pipelined path there is
        // no separate snapshot at all: `preview()` reads the parked hand-off
        // buffer, which already holds exactly this frame — that is what
        // keeps the pipeline's extra frame buffer NET FREE (Gitea #306).
        #[cfg(not(pipelined))]
        shared::set_pixels(frame);
        let b5 = crate::out_brightness();
        let wire = crate::apply_outpipe(frame, &mut self.buf, &mut self.gamma, &mut self.palette, b5, grid);
        let t1 = Instant::now();
        out.write_frame(wire, b5);
        let t2 = Instant::now();
        ((t1 - t0).as_micros() as u32, (t2 - t1).as_micros() as u32)
    }
}

// ---------------------------------------------------------------- direct

/// The whole frame sink in the render task: it owns the driver and runs
/// every post-VM stage inline. Byte-for-byte the pre-#306 behaviour.
#[cfg(not(pipelined))]
pub struct DirectSink {
    out: BoardOutput,
    pipe: PipeState,
    /// Crossfade blend / live-input assembly buffer — see [`stage`].
    stage: Vec<[u8; 3]>,
}

#[cfg(not(pipelined))]
impl DirectSink {
    pub fn new(out: BoardOutput) -> Self {
        Self { out, pipe: PipeState::new(), stage: Vec::new() }
    }

    /// Reconfigure the wire for `p`. Errors are the driver's (a fixed
    /// wire format rejects the switch); the render task keeps the previous
    /// protocol on `Err`.
    pub fn set_protocol(&mut self, p: crate::Protocol) -> Result<(), &'static str> {
        self.out.set_protocol(p).map_err(|_| "driver rejected the protocol")
    }

    /// (Re)size the driver's buffers. `false` = allocation failed, output
    /// paused until a later resize or the driver's own lazy retry.
    pub fn resize(&mut self, pixels: usize) -> bool {
        self.out.resize(pixels)
    }

    /// The render task's frame-assembly buffer: the crossfade blend target
    /// and the live-input (DDP/E1.31) assembly area. Emitted with
    /// [`emit_staged`](Self::emit_staged).
    pub fn stage(&mut self) -> &mut Vec<[u8; 3]> {
        &mut self.stage
    }

    /// Emit a frame the caller owns (the engine's own pixel buffer).
    pub fn emit(&mut self, frame: &[[u8; 3]], grid: Option<GridMap>) -> (u32, u32) {
        let Self { out, pipe, .. } = self;
        pipe.run(out, frame, grid)
    }

    /// Emit whatever [`stage`](Self::stage) currently holds.
    pub fn emit_staged(&mut self, grid: Option<GridMap>) -> (u32, u32) {
        let Self { out, pipe, stage } = self;
        pipe.run(out, stage, grid)
    }
}

// ------------------------------------------------------------- pipelined

#[cfg(pipelined)]
mod pipe {
    use super::*;
    use core::cell::RefCell;
    use core::sync::atomic::Ordering;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
    use embassy_sync::signal::Signal;
    use embassy_time::{Duration, Timer};
    use esp_println::println;

    /// A frame on its way to the output task. `grid` is the engine's view
    /// of the installed map at the moment the frame was rendered — it
    /// travels WITH the frame so the outpipe's spatial stages can never
    /// be applied with a map that belongs to a different pixel count.
    struct Frame {
        buf: Vec<[u8; 3]>,
        grid: Option<GridMap>,
    }

    /// The single travelling frame buffer, in exactly one of three places:
    /// `free` (waiting for the render task), `ready` (filled, waiting for
    /// the output task), or moved out into the task that is using it.
    #[derive(Default)]
    struct Slot {
        free: Option<Vec<[u8; 3]>>,
        ready: Option<Frame>,
    }

    static SLOT: BlockingMutex<CriticalSectionRawMutex, RefCell<Slot>> =
        BlockingMutex::new(RefCell::new(Slot { free: None, ready: None }));
    /// "A frame is ready." Coalescing by design: the output task always
    /// takes the newest frame, never a queue of stale ones.
    static READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();

    /// Frames the render task produced but could not hand over because the
    /// output task still held the buffer. Expected to be nonzero whenever
    /// the VM is faster than the compose (`/api/status` `out_fps` is the
    /// number that matters); reported in the boot log's fps line.
    pub static DROPPED: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

    /// The render task's half of the pipeline: no driver, no outpipe, no
    /// preview copy — just the hand-off, plus the crossfade/live-input
    /// assembly buffer the render task needs anyway.
    pub struct RenderSide {
        stage: Vec<[u8; 3]>,
    }

    impl RenderSide {
        /// Seeds the travelling buffer (empty; it grows to the frame size
        /// on the first hand-off and is never shrunk).
        pub fn new() -> Self {
            SLOT.lock(|c| c.borrow_mut().free = Some(Vec::new()));
            Self { stage: Vec::new() }
        }

        /// Fixed wire format: the pipelined path is HUB75-only, whose
        /// driver rejects protocol switches anyway. Same answer, without
        /// a round trip to the other core.
        pub fn set_protocol(&mut self, _p: crate::Protocol) -> Result<(), &'static str> {
            Err("hub75 panel: wire format is fixed")
        }

        /// The panel's framebuffers are allocated once at construction for
        /// a compile-time geometry, so there is nothing to size per pixel
        /// count; the output task still calls the driver's `resize` when
        /// the count changes, on the core that owns it.
        pub fn resize(&mut self, _pixels: usize) -> bool {
            true
        }

        pub fn stage(&mut self) -> &mut Vec<[u8; 3]> {
            &mut self.stage
        }

        /// Copy `frame` into the travelling buffer and hand it over. One
        /// 12 KB memcpy at 4096 px (~50 us against a 5-77 ms frame): the
        /// engine renders into a buffer it owns and `Engine::frame` may
        /// legitimately return the PREVIOUS frame (a pattern under its own
        /// frame-rate cap), so alternating the engine's buffer would show
        /// a stale frame on those. Copying keeps the engine API intact.
        pub fn emit(&mut self, frame: &[[u8; 3]], grid: Option<GridMap>) -> (u32, u32) {
            let Some(mut buf) = claim() else {
                DROPPED.fetch_add(1, Ordering::Relaxed);
                return (0, 0);
            };
            buf.clear();
            if buf.try_reserve(frame.len()).is_err() {
                // heap too tight for the frame buffer: give it back rather
                // than publishing a short frame (a black tail on the panel)
                SLOT.lock(|c| c.borrow_mut().free = Some(buf));
                return (0, 0);
            }
            buf.extend_from_slice(frame);
            publish(buf, grid);
            (0, 0)
        }

        /// Hand over whatever [`stage`](Self::stage) holds. The buffers are
        /// swapped rather than copied, so the crossfade blend and the
        /// live-input assembly cost the pipeline nothing extra.
        pub fn emit_staged(&mut self, grid: Option<GridMap>) -> (u32, u32) {
            let Some(buf) = claim() else {
                DROPPED.fetch_add(1, Ordering::Relaxed);
                return (0, 0);
            };
            publish(core::mem::replace(&mut self.stage, buf), grid);
            (0, 0)
        }
    }

    /// Take the travelling buffer: the free one, or an earlier frame the
    /// output task has not claimed yet (newest frame wins). `None` = the
    /// output task is composing and this frame is dropped.
    fn claim() -> Option<Vec<[u8; 3]>> {
        SLOT.lock(|c| {
            let s = &mut *c.borrow_mut();
            s.free.take().or_else(|| s.ready.take().map(|f| f.buf))
        })
    }

    fn publish(buf: Vec<[u8; 3]>, grid: Option<GridMap>) {
        SLOT.lock(|c| c.borrow_mut().ready = Some(Frame { buf, grid }));
        READY.signal(());
    }

    /// The last complete frame, flattened to RGB bytes — `GET /api/pixels`.
    ///
    /// The hand-off buffer IS the preview snapshot on this path: whenever it
    /// is parked in the slot (free or ready) it holds a whole frame, so
    /// there is nothing to keep a second 12 KB copy of. It is out of the slot
    /// only while one of the two tasks is filling or composing it — a ~50 us
    /// memcpy or a ~6 ms compose against a slot that is parked the rest of
    /// the time — so retry briefly rather than answer with nothing. An empty
    /// answer means no frame has been rendered yet (or the heap could not
    /// hold the response), exactly as the pre-#306 snapshot did before the
    /// first frame.
    ///
    /// ONE allocation, made OUTSIDE the critical section. This runs on a heap
    /// that a 4096 px pattern can leave under 30 KB free, so a second 12 KB
    /// temporary is the difference between serving the preview and an OOM
    /// panic; and allocating with interrupts masked would stall both cores on
    /// the allocator's own lock.
    pub async fn preview() -> Vec<u8> {
        let mut v: Vec<u8> = Vec::new();
        for _ in 0..16 {
            let need = shared::PIXEL_COUNT.load(Ordering::Relaxed) as usize * 3;
            if v.capacity() < need && v.try_reserve_exact(need).is_err() {
                return Vec::new();
            }
            if SLOT.lock(|c| {
                let s = c.borrow();
                let Some(buf) = s.free.as_ref().or(s.ready.as_ref().map(|f| &f.buf)) else {
                    return false;
                };
                let bytes = buf.as_flattened();
                // never grow inside the critical section
                if bytes.len() > v.capacity() {
                    return false;
                }
                v.clear();
                v.extend_from_slice(bytes);
                true
            }) {
                return v;
            }
            Timer::after(Duration::from_millis(2)).await;
        }
        Vec::new()
    }

    /// The ProCpu half: preview copy, output pipeline, panel compose.
    ///
    /// Runs on the main executor alongside WiFi, the network stack and the
    /// web pool. At 4096 px it costs ~6 ms per rendered frame — ~24 % of
    /// the ProCpu at a 39 fps rainbow, far from the ~95 % that starved the
    /// web task before the render loop moved off this core (Gitea #259).
    #[embassy_executor::task]
    pub async fn output_task(mut out: BoardOutput) -> ! {
        let mut pipe = PipeState::new();
        // The seeded protocol (flash may name one other than the boot
        // default the driver was built with) and the initial buffer size:
        // both are the driver's, so both happen here now.
        if let Err(e) = out.set_protocol(crate::cur_protocol()) {
            println!("output: protocol config not applied: {:?}", e);
        }
        let mut sized = shared::PIXEL_COUNT.load(Ordering::Relaxed);
        if !out.resize(sized as usize) {
            println!("encode buffer alloc failed at boot — output paused");
        }
        let mut frames: u32 = 0;
        let mut pipe_sum: u64 = 0;
        let mut out_sum: u64 = 0;
        let mut mark = Instant::now();
        let mut last_rescans: u32 = 0;
        loop {
            // The timer half keeps the once-a-second publish running while
            // nothing renders, so the counters fall to 0 instead of
            // freezing at the last busy window's average.
            embassy_futures::select::select(READY.wait(), Timer::after(Duration::from_millis(250)))
                .await;
            if let Some(f) = SLOT.lock(|c| c.borrow_mut().ready.take()) {
                // A live pixel-count change resizes the driver here, on the
                // core that owns it. (No-op on the panel: its geometry is
                // compile-time and its framebuffers are allocated once.)
                let px = shared::PIXEL_COUNT.load(Ordering::Relaxed);
                if px != sized {
                    sized = px;
                    if !out.resize(px as usize) {
                        println!("encode buffer alloc failed ({} px) — output paused", px);
                    }
                }
                let (p, o) = pipe.run(&mut out, &f.buf, f.grid);
                // back to the render task only AFTER write_frame: the wire
                // slice borrows this buffer whenever the outpipe is a no-op
                SLOT.lock(|c| c.borrow_mut().free = Some(f.buf));
                pipe_sum += p as u64;
                out_sum += o as u64;
                frames += 1;
            }
            if mark.elapsed().as_millis() >= 1000 {
                let n = frames.max(1) as u64;
                shared::OUT_FPS.store(frames, Ordering::Relaxed);
                // The panel's real refresh rate, from the driver's BCM
                // frame counter. `elapsed` is >= 1000 ms but not exactly
                // that, so scale rather than assuming a 1 s window.
                let now = shared::RESCANS.load(Ordering::Relaxed);
                let ms = mark.elapsed().as_millis().max(1);
                let hz = (u64::from(now.wrapping_sub(last_rescans)) * 1000 / ms) as u32;
                shared::RESCAN_HZ.store(hz, Ordering::Relaxed);
                last_rescans = now;
                shared::PIPE_US.store(if frames == 0 { 0 } else { (pipe_sum / n) as u32 }, Ordering::Relaxed);
                shared::OUT_US.store(if frames == 0 { 0 } else { (out_sum / n) as u32 }, Ordering::Relaxed);
                frames = 0;
                pipe_sum = 0;
                out_sum = 0;
                mark = Instant::now();
            }
        }
    }
}

#[cfg(pipelined)]
pub use pipe::{output_task, preview, RenderSide, DROPPED};

/// The frame sink the render task holds on this board.
#[cfg(pipelined)]
pub type RenderSink = RenderSide;
#[cfg(not(pipelined))]
pub type RenderSink = DirectSink;
