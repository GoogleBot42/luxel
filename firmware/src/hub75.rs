//! HUB75 matrix-panel output over the ESP32-S3 LCD_CAM peripheral
//! (Gitea #72; feature `hub75`, S3 only — C3/S2 have no parallel-output
//! peripheral).
//!
//! Unlike the SPI strips there are no latching pixels: a circular DMA
//! chain (esp-hub75 `circular-dma`) autonomously rescans a BCM
//! framebuffer, so panel refresh costs ~zero CPU and is fully decoupled
//! from the engine frame rate. Per engine frame `write_frame` composes
//! the post-outpipe RGB888 frame into the back framebuffer's bitplanes
//! and queues a buffer swap.
//!
//! That swap is frame-atomic — but only because of a local patch to
//! esp-hub75 (`firmware/patches/esp-hub75-0.14.0-atomic-swap.patch`,
//! Gitea #376). The driver keeps ONE DESCRIPTOR RING PER FRAMEBUFFER and
//! swaps by rewriting the running ring's tail `next` to the other ring's
//! head, a single aligned store the DMA reads only when it wraps. So the
//! panel switches images exactly between rescans and no displayed frame
//! ever mixes two images' bitplanes. Stock esp-hub75 0.14 instead
//! rewrites every descriptor's `buffer` pointer the instant `swap()` is
//! called, mid-pass, and mixes the high bitplanes of one frame with the
//! low bitplanes of the next (visible on the panel; filmed 2026-09-07).
//!
//! Framebuffers are DMA targets and must live in internal SRAM. Two
//! `PLANES`-deep bitplane buffers (~28 KB each at 64x64/7-plane) don't
//! fit the S3's leftover `.stack` region as statics, so they're
//! heap-leaked at construction time instead — main() wiring runs before
//! the WiFi blob's boot mallocs, when the heap is fresh enough that two
//! contiguous 28 KB blocks are a certainty. Allocation failure disables
//! output (render keeps ticking) rather than panicking.

use core::sync::atomic::Ordering;

use embedded_graphics::geometry::Point;
use esp_hal::peripherals::{DMA_CH0, LCD_CAM};
use esp_hal::time::Rate;
use esp_hal::Blocking;
use esp_hub75::framebuffer::bitplane::plain::DmaFrameBuffer;
use esp_hub75::framebuffer::compute_rows;
use esp_hub75::{Color, Hub75, Hub75Pins16, Hub75Swap};
use esp_println::println;

use crate::leds::{scale5, Protocol};
use crate::output::OutputDriver;

/// Panel geometry. Compile-time on purpose: the framebuffer type is
/// const-generic and DMA-static, so runtime width/height would mean
/// carrying every monomorphization in flash. Chained panels / other
/// geometries get their own board consts when a board needs them (#73).
pub const PANEL_COLS: usize = 64;
pub const PANEL_ROWS: usize = 64;
const NROWS: usize = compute_rows(PANEL_ROWS);

/// BCM bit depth. Refresh rate halves per extra plane (the MSB plane is
/// rescanned 2^(PLANES-1) times per frame). Measured on the bench panel at
/// [CLOCK] = 30 MHz: **115 Hz at 7 planes**, so ~58 Hz at 8 and ~231 Hz at
/// 6. 7 matches the esp-hub75 author's own 64x64 S3 example. Drop to 6 for
/// tight heaps (4 KB less per buffer) or a faster rescan.
const PLANES: usize = 7;

/// LCD_CAM pixel-clock rate.
///
/// The FM6124 datasheet (v1.1) puts the ceiling at **FCLK max 30 MHz**, and
/// its minimum clock high/low of 20 ns each implies 25 MHz on pulse width
/// alone — so 30 MHz is the datasheet limit with no margin, and the
/// 74HCT245 buffers on this board add another 22–28 ns of worst-case tpd.
///
/// Measured on the bench panel (64x64 FM6124EJ, 7 planes, 2026-09-07,
/// Gitea #255) — the rescan rate is exactly linear in the clock:
///
/// | clock  | rescans/s | verdict                                    |
/// |--------|-----------|--------------------------------------------|
/// | 20 MHz | 77        | clean (the esp-hub75 example's value)      |
/// | 30 MHz | 115       | clean — chosen                             |
/// | 40 MHz | 154       | **fails**: mid-panel split, colours wrong  |
///
/// 40 MHz is out of spec and looks it: the two 32-row halves mis-sample and
/// the colours distort. Note the firmware sees **nothing** wrong when that
/// happens — no swap error, no DMA error, `vmerr` null, and the composed
/// frame is still byte-identical to a host render. Only the panel's own
/// sampling fails, so this class of regression needs an eyeball, not a test.
///
/// A faster clock buys no frame throughput (every pattern here is
/// render-bound, not rescan-bound; fps was identical at all three rates).
/// What it buys is headroom: 8 bitplanes become usable (~58 Hz rather than
/// ~38), and chained panels get the bandwidth they need (#255).
const CLOCK: Rate = Rate::from_mhz(30);

type Fb = DmaFrameBuffer<NROWS, PANEL_COLS, PLANES>;

/// Fallibly heap-allocate a framebuffer, leaked to the `'static` the DMA
/// driver requires. Zeroed-alloc + `format()` is exactly `Fb::new()`
/// (zeroed color bits + row-address/control formatting) without a ~28 KB
/// stack temporary (frame budget is 12 KB — tools/stack-check.sh).
fn alloc_fb() -> Option<&'static mut Fb> {
    let layout = core::alloc::Layout::new::<Fb>();
    let p = unsafe { alloc::alloc::alloc_zeroed(layout) }.cast::<Fb>();
    if p.is_null() {
        return None;
    }
    let fb = unsafe { &mut *p };
    fb.format();
    Some(fb)
}

/// The panel driver behind `output::BoardOutput` on `hub75` builds.
pub struct Hub75Output {
    /// `None` = init failed (framebuffer alloc or LCD_CAM setup); output
    /// stays disabled while the engine keeps running.
    hub75: Option<Hub75<Blocking, Fb>>,
    /// The compose target while no swap is in flight.
    back: Option<&'static mut Fb>,
    /// The previous frame's swap; waited (instant by then) at the start
    /// of the next `write_frame` to reclaim the displaced buffer.
    pending: Option<Hub75Swap<Fb>>,
    /// Panel rescan count when the last frame was handed to the DMA, for
    /// `pass_per_frame_max` (Gitea #395).
    last_shown_rescan: u32,
    /// Sequence number of the next frame to compose.
    next_seq: u32,
    /// How far through the driver's shown-log we have audited.
    shown_cursor: u32,
    /// Last displayed sequence seen by the audit.
    last_shown_seq: u32,
}

impl Hub75Output {
    pub fn new(lcd_cam: LCD_CAM<'static>, pins: Hub75Pins16<'static>, channel: DMA_CH0<'static>) -> Self {
        let dead = Self {
            hub75: None,
            back: None,
            pending: None,
            last_shown_rescan: 0,
            next_seq: 1,
            shown_cursor: 0,
            last_shown_seq: 0,
        };
        // esp-hub75's macro expands to `StaticCell::uninit().write([EMPTY; N])`
        // — the descriptor array is written straight into the static, but the
        // literal is a value expression so clippy counts it as a stack array.
        // It lives in a third-party macro we only carry as a patch file, and
        // DMA descriptors must be in a fixed static anyway (the peripheral
        // walks them), so heap is not an option. tools/stack-check.sh measures
        // the real linked frames.
        #[allow(clippy::large_stack_arrays)]
        let tx_descriptors = esp_hub75::hub75_dma_descriptors!(Fb);
        let (front, back) = match (alloc_fb(), alloc_fb()) {
            (Some(f), Some(b)) => (f, b),
            _ => {
                println!("hub75: framebuffer alloc failed — panel output disabled");
                return dead;
            }
        };
        match Hub75::new(lcd_cam, pins, channel, tx_descriptors, CLOCK, &*front) {
            Ok(h) => {
                // Descriptor cost is worth printing once: the frame-atomic
                // swap (#376) doubles it, and it is static DMA-capable RAM.
                const DESCS: usize =
                    esp_hub75::dma_descriptor_count(Fb::bcm_chunk_count(), Fb::bcm_chunk_bytes());
                println!(
                    "hub75: {}x{} panel, {} bitplanes, LCD_CAM @ {} MHz, circular DMA, \
                     {} descriptors x {} rings = {} B",
                    PANEL_COLS,
                    PANEL_ROWS,
                    PLANES,
                    CLOCK.as_mhz(),
                    DESCS,
                    esp_hub75::DESCRIPTOR_RINGS,
                    DESCS
                        * esp_hub75::DESCRIPTOR_RINGS
                        * core::mem::size_of::<esp_hal::dma::DmaDescriptor>(),
                );
                Self {
                    hub75: Some(h),
                    back: Some(back),
                    pending: None,
                    last_shown_rescan: 0,
                    next_seq: 1,
                    shown_cursor: 0,
                    last_shown_seq: 0,
                }
            }
            Err(e) => {
                println!("hub75: LCD_CAM init failed: {:?} — panel output disabled", e);
                dead
            }
        }
    }
}

impl Hub75Output {
    /// Replay the driver's log of framebuffers the panel actually scanned out,
    /// translate it back into frame sequence numbers, and count skips and
    /// repeats (Gitea #395).
    ///
    /// A skip is a composed frame whose framebuffer never appears in the log:
    /// the panel went straight from frame N to frame N+2. That is invisible to
    /// `dropped` and `out_fps`, because `write_frame` succeeded — the frame was
    /// composed and its swap was armed, the swap just never took effect. It is
    /// exactly what the camera showed, without needing a camera.
    fn audit_shown(&mut self, shown: (u32, [u32; esp_hub75::SHOWN_LOG_LEN], u32, u32)) {
        let (n, log, arm, arm_max) = shown;
        crate::shared::SHOWN_ARM_IDX.store(arm, Ordering::Relaxed);
        crate::shared::SHOWN_ARM_IDX_MAX.store(arm_max, Ordering::Relaxed);
        let len = log.len() as u32;
        // The panel produces EOFs slightly faster than we audit them, so the
        // ring WILL lap. Entries lost to that are not skips — count the lapse
        // and forget the previous sequence, so nothing is attributed across a
        // hole we cannot see into. Conflating the two is what made the first
        // version of this counter report phantom skips.
        let oldest = n.saturating_sub(len);
        if self.shown_cursor < oldest {
            crate::shared::SHOWN_LAPSED.fetch_add(oldest - self.shown_cursor, Ordering::Relaxed);
            self.last_shown_seq = 0;
        }
        let from = self.shown_cursor.max(oldest);
        for k in from..n {
            let seq = log[(k % len) as usize];
            if seq == 0 {
                continue; // pass predating the first tagged swap
            }
            crate::shared::push_tag(seq);
            if self.last_shown_seq != 0 && seq != self.last_shown_seq {
                let gap = seq.wrapping_sub(self.last_shown_seq);
                if gap == 0 || gap > 0x8000_0000 {
                    // out of order: ignore rather than mis-blame
                } else if gap > 1 {
                    // Counted in the ISR now; here only to freeze a window for
                    // eyeballing. The snapshot this reads is racy, so it must
                    // not drive any counter.
                    crate::shared::SHOWN_SKIP_ARM_IDX.store(arm, Ordering::Relaxed);
                    crate::shared::freeze_skip_tags();
                }
            }
            if seq != 0 {
                self.last_shown_seq = seq;
            }

        }
        self.shown_cursor = n;
    }
}

impl OutputDriver for Hub75Output {
    type Error = &'static str;

    fn set_protocol(&mut self, _p: Protocol) -> Result<(), Self::Error> {
        // Fixed wire format — the render task keeps the previous protocol
        // on Err (the output.rs contract for exactly this driver).
        Err("hub75 panel: wire format is fixed")
    }

    fn resize(&mut self, _pixels: usize) -> bool {
        // Fixed panel geometry, buffers allocated at construction: any
        // count "fits" (write_frame ignores pixels past the panel area,
        // and pixels short of it leave the tail black).
        self.hub75.is_some()
    }

    /// Can the panel take a frame right now — i.e. has the previous swap
    /// landed? The pipelined output task waits on this instead of composing
    /// into a buffer the panel is about to overwrite (Gitea #387).
    fn ready_for_frame(&self) -> bool {
        self.pending.as_ref().is_none_or(Hub75Swap::is_done)
    }

    /// The panel rescans on its own clock, so the render loop paces on it.
    fn paces_frames(&self) -> bool {
        self.hub75.is_some()
    }

    fn write_frame(&mut self, rgb: &[[u8; 3]], brightness5: u8) -> bool {
        // Audit which frames the panel actually scanned out since last time.
        // Copy the log out first so the driver borrow ends before the &mut
        // self call (Gitea #395).
        let shown = self.hub75.as_ref().map(Hub75::shown_log);
        if let Some(shown) = shown {
            self.audit_shown(shown);
        }
        let Some(hub75) = self.hub75.as_ref() else { return false };
        // The panel's own BCM frame counter, for `rescan_hz`. Free: the ISR
        // that feeds it is always armed in circular-DMA mode.
        crate::shared::RESCANS.store(hub75.frame_count(), Ordering::Relaxed);
        // Swap diagnostics (Gitea #387): how often a swap armed inside the
        // unserviced-EOF window that the pending-EOF check closes, and how
        // often it had to take the two-EOF fallback. The first is the rate at
        // which the pre-fix driver would have handed back a framebuffer still
        // being scanned out — a glitch no frame accounting can see.
        let (sk, rp, ps) = hub75.shown_counts();
        crate::shared::SHOWN_SKIPS.store(sk, Ordering::Relaxed);
        crate::shared::SHOWN_REPEATS.store(rp, Ordering::Relaxed);
        crate::shared::SHOWN_AUDITED.store(ps, Ordering::Relaxed);
        let (mismatch, double_arm) = hub75.landing_stats();
        crate::shared::LANDING_MISMATCH.store(mismatch, Ordering::Relaxed);
        crate::shared::DOUBLE_ARM.store(double_arm, Ordering::Relaxed);
        let (race, slow) = hub75.swap_stats();
        crate::shared::SWAP_EOF_RACE.store(race, Ordering::Relaxed);
        crate::shared::SWAP_SLOW_PATH.store(slow, Ordering::Relaxed);
        // Pass-length forensics (Gitea #395): a pass shorter than a ring means
        // the DMA entered a ring off its head, which is the only mechanism
        // that can lose a displayed frame without `write_frame` failing.
        let (pc, pmin, pmax, pnom, pshort, plong, plat) = hub75.pass_stats();
        crate::shared::PASS_COUNT.store(pc, Ordering::Relaxed);
        crate::shared::PASS_MIN_US.store(pmin, Ordering::Relaxed);
        crate::shared::PASS_MAX_US.store(pmax, Ordering::Relaxed);
        crate::shared::PASS_NOMINAL_US.store(pnom, Ordering::Relaxed);
        crate::shared::PASS_SHORT.store(pshort, Ordering::Relaxed);
        crate::shared::PASS_LONG.store(plong, Ordering::Relaxed);
        crate::shared::PASS_ISR_LAT_MAX.store(plat, Ordering::Relaxed);
        // The log itself only moves when a short pass happens, which should be
        // never — mirror it only then rather than every frame.
        if pshort != 0 {
            let (n, log) = hub75.pass_shorts();
            crate::shared::set_pass_shorts(n, &log);
        }

        // Reclaim the displaced buffer from the previous frame's swap.
        // With the patched driver a swap lands when the DMA wraps onto the
        // new descriptor ring — the next rescan boundary, ~8.7 ms at 7
        // planes / 30 MHz — and `is_done()` is the ISR's *observation* of
        // that, not a guess. If it hasn't landed yet, skip this frame
        // rather than spin: output is best-effort (the trait contract),
        // this self-throttles compose to the panel's rescan rate, and a
        // stalled DMA can never hang the render task.
        let back = match self.pending.take() {
            Some(swap) => {
                if !swap.is_done() {
                    self.pending = Some(swap);
                    return false;
                }
                match swap.wait() {
                    Ok(fb) => fb,
                    Err((e, fb)) => {
                        println!("hub75: swap error: {:?}", e);
                        fb
                    }
                }
            }
            None => match self.back.take() {
                Some(fb) => fb,
                None => return false,
            },
        };
        back.erase();
        // brightness5: no APA102-style hardware field on HUB75 — scale
        // channels in software exactly like the WS2812 path. At 0 the
        // erase above already produced the all-black frame.
        if brightness5 > 0 {
            let full = brightness5 >= 31;
            for (i, px) in rgb.iter().enumerate().take(PANEL_COLS * PANEL_ROWS) {
                let [r, g, b] = *px;
                let (r, g, b) = if full {
                    (r, g, b)
                } else {
                    (scale5(r, brightness5), scale5(g, brightness5), scale5(b, brightness5))
                };
                let p = Point::new((i % PANEL_COLS) as i32, (i / PANEL_COLS) as i32);
                back.set_pixel(p, Color::new(r, g, b));
            }
        }
        // Rescans between consecutive DISPLAYED frames, sampled at the only
        // point that knows a frame is going out: 1 = every rescan showed
        // something new, 2 = one repeat (Gitea #395).
        let rescans = hub75.frame_count();
        if self.last_shown_rescan != 0 {
            let d = rescans.wrapping_sub(self.last_shown_rescan);
            crate::shared::PASS_PER_FRAME_MAX.fetch_max(d, Ordering::Relaxed);
            crate::shared::PASS_PER_FRAME_MIN.fetch_min(d, Ordering::Relaxed);
            if d == 0 {
                // Two frames handed over inside one rescan: the first was
                // never scanned out. This is the skip, caught at the source.
                crate::shared::PASS_ZERO_RESCAN.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.last_shown_rescan = rescans;
        // Tag this frame so the driver's log of DISPLAYED passes reads back
        // as our own sequence — no pointer mapping to go stale (Gitea #395).
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1).max(1);
        hub75.set_swap_tag(seq);
        self.pending = Some(hub75.swap(back));
        true
    }
}
