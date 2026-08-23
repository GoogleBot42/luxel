//! HUB75 matrix-panel output over the ESP32-S3 LCD_CAM peripheral
//! (Gitea #72; feature `hub75`, S3 only — C3/S2 have no parallel-output
//! peripheral).
//!
//! Unlike the SPI strips there are no latching pixels: a circular DMA
//! chain (esp-hub75 `circular-dma`) autonomously rescans a BCM
//! framebuffer, so panel refresh costs ~zero CPU and is fully decoupled
//! from the engine frame rate. Per engine frame `write_frame` composes
//! the post-outpipe RGB888 frame into the back framebuffer's bitplanes
//! and queues an atomic buffer swap at the next rescan boundary.
//!
//! Framebuffers are DMA targets and must live in internal SRAM. Two
//! `PLANES`-deep bitplane buffers (~28 KB each at 64x64/7-plane) don't
//! fit the S3's leftover `.stack` region as statics, so they're
//! heap-leaked at construction time instead — main() wiring runs before
//! the WiFi blob's boot mallocs, when the heap is fresh enough that two
//! contiguous 28 KB blocks are a certainty. Allocation failure disables
//! output (render keeps ticking) rather than panicking.

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
/// rescanned 2^(PLANES-1) times per frame): at a 20 MHz LCD_CAM clock a
/// 64x64 panel does ~77 Hz at 7 planes, ~38 Hz at 8, ~155 Hz at 6 — so 8
/// is unusable without a faster clock, and 7 matches the esp-hub75
/// author's own 64x64 S3 example. Drop to 6 for tight heaps (4 KB less
/// per buffer) or a faster rescan; tune on metal in #75.
const PLANES: usize = 7;

/// LCD_CAM pixel-clock rate (the esp-hub75 S3 example's value).
const CLOCK: Rate = Rate::from_mhz(20);

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
}

impl Hub75Output {
    pub fn new(lcd_cam: LCD_CAM<'static>, pins: Hub75Pins16<'static>, channel: DMA_CH0<'static>) -> Self {
        let dead = Self { hub75: None, back: None, pending: None };
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
                println!(
                    "hub75: {}x{} panel, {} bitplanes, LCD_CAM @ {} MHz, circular DMA",
                    PANEL_COLS,
                    PANEL_ROWS,
                    PLANES,
                    CLOCK.as_mhz()
                );
                Self { hub75: Some(h), back: Some(back), pending: None }
            }
            Err(e) => {
                println!("hub75: LCD_CAM init failed: {:?} — panel output disabled", e);
                dead
            }
        }
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

    fn write_frame(&mut self, rgb: &[[u8; 3]], brightness5: u8) {
        let Some(hub75) = self.hub75.as_ref() else { return };
        // Reclaim the displaced buffer from the previous frame's swap. A
        // swap lands at a rescan boundary (~13 ms at 7 planes / 20 MHz);
        // if it hasn't landed yet, skip this frame rather than spin —
        // output is best-effort (the trait contract), this self-throttles
        // compose to the panel's rescan rate, and a stalled DMA can never
        // hang the render task.
        let back = match self.pending.take() {
            Some(swap) => {
                if !swap.is_done() {
                    self.pending = Some(swap);
                    return;
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
                None => return,
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
        self.pending = Some(hub75.swap(back));
    }
}
