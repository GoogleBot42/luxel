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

/// Entries in one framebuffer, as the bulk packer counts them.
const FB_WORDS: usize = luxel_hub75::words(NROWS, PANEL_COLS, PLANES);

/// The bulk packer (Gitea #329) treats the framebuffer as a flat `[u16]` laid
/// out plane -> row -> column. That is exactly `DmaFrameBuffer`'s `#[repr(C)]`
/// shape today, and this catches the one way it could silently stop being so:
/// a `hub75-framebuffer` feature that adds words (`inter-row-blank-*`,
/// `tail-closes-latch`) would change the size. Column ORDER can't be caught
/// this way, so `probe_layout` checks that on the real buffer at boot.
const _: () = assert!(
    core::mem::size_of::<Fb>() == FB_WORDS * core::mem::size_of::<u16>(),
    "framebuffer is not a flat entry array — the bulk packer's layout assumption broke"
);

/// The framebuffer as the flat entry array the packer writes.
fn fb_words(fb: &mut Fb) -> &mut [u16] {
    // SAFETY: `DmaFrameBuffer` is `#[repr(C)]` over `[PlaneData; PLANES]`,
    // `PlaneData` over `[Row; NROWS]`, `Row` over `[Entry; COLS]` (plus
    // zero-length arrays for the disabled blank/tail features), and `Entry`
    // is `#[repr(transparent)] u16`. So the whole struct is `FB_WORDS`
    // contiguous `u16`s with no padding — asserted above — and `u16`'s
    // alignment is the struct's own.
    unsafe { core::slice::from_raw_parts_mut((fb as *mut Fb).cast::<u16>(), FB_WORDS) }
}

/// Heap-allocate the packer's spread tables (2 KiB), leaked like the
/// framebuffers. `Tables::zeroed()` is all-zero bytes, so `alloc_zeroed`
/// produces a valid value without a 2 KiB stack temporary.
fn alloc_tables() -> Option<&'static mut luxel_hub75::Tables> {
    let layout = core::alloc::Layout::new::<luxel_hub75::Tables>();
    let p = unsafe { alloc::alloc::alloc_zeroed(layout) }.cast::<luxel_hub75::Tables>();
    if p.is_null() {
        return None;
    }
    Some(unsafe { &mut *p })
}

/// The channel LUT the packer folds into its tables: what the panel is
/// actually driven with for each source value. Exactly the arithmetic the
/// per-pixel path does inline, so the two are byte-identical by construction
/// (asserted in `luxel-hub75`'s tests against the real framebuffer type).
fn brightness_lut(brightness5: u8) -> [u8; 256] {
    let mut lut = [0u8; 256];
    let full = brightness5 >= 31;
    for (c, v) in lut.iter_mut().enumerate() {
        let c = c as u8;
        *v = if full { c } else { scale5(c, brightness5) };
    }
    lut
}

/// Does the real framebuffer agree with the packer's index arithmetic?
///
/// The size assert above pins the word COUNT but not the column ordering
/// (`hub75-framebuffer`'s `esp32-ordering` feature XORs adjacent columns) or
/// the plane/half bit assignment. Rather than encode "the S3 doesn't enable
/// that feature" as a comment, write three pixels through the crate's own
/// `set_pixel` and check they land where `pack` would have put them. Runs
/// once, on the still-unused back buffer, and leaves it erased.
fn probe_layout(fb: &mut Fb) -> bool {
    const R1: u16 = 1 << 9;
    const G1: u16 = 1 << 10;
    const B2: u16 = 1 << 14;
    const STRIDE: usize = NROWS * PANEL_COLS;
    // A green value whose only set bit belongs to the LAST plane.
    let g: u8 = 1 << (8 - PLANES);

    fb.erase();
    fb.set_pixel(Point::new(1, 0), Color::new(255, 0, 0));
    fb.set_pixel(Point::new(2, NROWS as i32), Color::new(0, 0, 255));
    fb.set_pixel(Point::new(3, 1), Color::new(0, g, 0));

    let mut ok = true;
    {
        let w = fb_words(fb);
        // 255 lights every plane for its channel; `g` lights exactly one.
        let lit = w.iter().filter(|e| **e & luxel_hub75::COLOR_MASK != 0).count();
        ok &= lit == 2 * PLANES + 1;
        for p in 0..PLANES {
            ok &= w[p * STRIDE + 1] & luxel_hub75::COLOR_MASK == R1;
            ok &= w[p * STRIDE + 2] & luxel_hub75::COLOR_MASK == B2;
        }
        ok &= w[(PLANES - 1) * STRIDE + PANEL_COLS + 3] & luxel_hub75::COLOR_MASK == G1;
    }
    fb.erase();
    ok
}

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
    /// Brightness-folded spread tables for the bulk packer (Gitea #329).
    /// `None` = allocation failed or the layout probe disagreed; the
    /// per-pixel path still works, it is just five times slower.
    tables: Option<&'static mut luxel_hub75::Tables>,
    /// Brightness the tables were built for; `u8::MAX` = never built.
    tables_b5: u8,
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
            tables: None,
            tables_b5: u8::MAX,
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
        // Bulk packer (Gitea #329): used only if its layout assumption holds
        // on the buffer we actually got, and if its 2 KiB of tables fit.
        let tables = if probe_layout(back) {
            match alloc_tables() {
                Some(t) => {
                    println!(
                        "hub75: bulk bitplane packer active ({} B of tables)",
                        core::mem::size_of::<luxel_hub75::Tables>()
                    );
                    Some(t)
                }
                None => {
                    println!("hub75: packer table alloc failed — per-pixel compose");
                    None
                }
            }
        } else {
            println!("hub75: framebuffer layout probe FAILED — per-pixel compose");
            None
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
                    tables,
                    tables_b5: u8::MAX,
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
        // brightness5: no APA102-style hardware field on HUB75 — scale
        // channels in software exactly like the WS2812 path.
        //
        // The bulk packer (Gitea #329) walks the frame per ROW PAIR, building
        // all PLANES entry words for a pixel pair from one pair of table
        // lookups per channel, with brightness folded into those tables. It
        // writes every colour bit of every entry, so it subsumes the erase.
        // The per-pixel path below is the fallback for a framebuffer whose
        // layout the boot probe did not recognise.
        if let Some(t) = self.tables.as_deref_mut() {
            if self.tables_b5 != brightness5 {
                t.build(&brightness_lut(brightness5));
                self.tables_b5 = brightness5;
            }
        }
        match self.tables.as_deref() {
            Some(tables) => {
                luxel_hub75::pack::<NROWS, PANEL_COLS, PLANES>(fb_words(back), rgb, tables);
            }
            None => {
                back.erase();
                // At 0 the erase above already produced the all-black frame.
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
