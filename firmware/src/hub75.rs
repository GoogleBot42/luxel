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
//!
//! ## Spare-plane swap (`hub75-spare-plane`, Gitea #610)
//!
//! The same frame-atomic guarantee as the two-buffer swap — nothing is ever
//! written where the DMA can read it — for one framebuffer plus ONE extra
//! plane instead of two framebuffers. The circular ring is plane-major and
//! plane 0 is the MSB, repeated `2^(PLANES-1)` times, so the first half of
//! every pass reads nothing but plane 0; planes `1..PLANES` are idle then.
//! The driver is handed two *views* ([`spare::PlaneView`]) over the same
//! internal buffer that differ only in which block plane 0 names: the
//! buffer's own, or a spare one. Per frame: `write_frame` composes into a
//! full-size staging framebuffer (the PSRAM arena where the board has one)
//! whenever the render task delivers; `flush`, polled by the output task,
//! waits until the DMA is inside the MSB run with room to spare, arms the
//! ring flip to the other view FIRST (the atomic-swap patch's single tail
//! store, which lands at the wrap), then copies planes `1..` into the live
//! buffer and the new MSB into the idle spare block. The next pass reads the
//! new frame in full. Each copy has a deadline — plane 1 before the DMA
//! leaves the MSB run, plane k before it reaches plane k, the spare before
//! the wrap — and the copies are in that order, so plane 1's deadline is the
//! only tight one and every later plane has exponentially more slack. The
//! window check sizes itself from the measured per-plane copy time and the
//! ISR's nominal pass length; a pass without room defers the frame (counted,
//! `spare.deferred`); a copy that still overruns is counted as
//! `spare.torn_p1`/`spare.torn_wrap` — the one way this mode can tear, and a
//! bug if it ever moves off 0.

use core::sync::atomic::Ordering;

use embedded_graphics::geometry::Point;
use esp_hal::peripherals::{DMA_CH0, LCD_CAM};
use esp_hal::time::Rate;
use esp_hal::Blocking;
use esp_hub75::framebuffer::bitplane::plain::DmaFrameBuffer;
#[cfg(feature = "hub75-spare-plane")]
use esp_hub75::framebuffer::FrameBuffer;
use esp_hub75::framebuffer::compute_rows;
use esp_hub75::{Color, Hub75, Hub75Pins16, Hub75Swap};
use esp_println::println;

use luxel_core::layout::{Matrix, PanelView};
use luxel_hub75::arrange;

use crate::leds::{scale5, Protocol};
use crate::output::OutputDriver;

/// The area the DMA drives, in panel pixels. Compile-time on purpose: the
/// framebuffer type is const-generic and DMA-static, so runtime width/height
/// would mean carrying every monomorphization in flash (#401 is that work).
///
/// This is a CHAIN extent, not one panel: a HUB75 chain shifts as one ribbon
/// `pw` wide per tile, so two 32-wide tiles fit here exactly as one 64-wide
/// tile does. What the tiles are, where they hang and which way the chain
/// threads them is the runtime arrangement (`Layout.matrix`, #475); the
/// firmware turns it into a remap table at boot and drives the leading tiles
/// that fit in here (`arrange::driven_panels`).
pub const PANEL_COLS: usize = 64;
pub const PANEL_ROWS: usize = 64;
const NROWS: usize = compute_rows(PANEL_ROWS);
/// Driver pixels the framebuffer covers — also the remap table's length.
const FB_PIXELS: usize = PANEL_COLS * PANEL_ROWS;
const _: () = assert!(NROWS * 2 == PANEL_ROWS, "the remap assumes a half-height dual-RGB scan");

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

/// Descriptors in one ring: `descs_per_plane * (2^PLANES - 1)`.
const DESCS: usize = esp_hub75::dma_descriptor_count(Fb::bcm_chunk_count(), Fb::bcm_chunk_bytes());

/// What the DMA driver is built against. Normally the framebuffer itself;
/// in spare-plane mode a view over it (Gitea #610).
#[cfg(feature = "hub75-spare-plane")]
type DmaFb = spare::PlaneView;
#[cfg(not(feature = "hub75-spare-plane"))]
type DmaFb = Fb;

/// Spare-plane swap (Gitea #610): the views, the buffers, the window maths.
#[cfg(feature = "hub75-spare-plane")]
mod spare {
    use core::alloc::Layout;

    use esp_hub75::framebuffer::FrameBuffer;

    use super::{Fb, DESCS, PLANES};

    /// Bytes in one bit-plane of `Fb` (all rows, all columns).
    pub const PLANE_BYTES: usize = Fb::bcm_chunk_bytes();
    /// Descriptors covering one plane once.
    pub const DESCS_PER_PLANE: usize = DESCS / ((1 << PLANES) - 1);
    /// Descriptors from the ring head to the end of the MSB run: plane 0
    /// emitted `2^(PLANES-1)` times.
    pub const MSB_DESCS: usize = DESCS_PER_PLANE << (PLANES - 1);
    /// Margin the window check keeps beyond the measured copy cost.
    pub const SLACK_NS: u64 = 300_000;
    /// A staged frame no window opened for within this long is abandoned
    /// rather than freezing the engine behind it — the same liveness floor
    /// as the output task's vsync hold (a dead DMA must not stop the world).
    pub const HOLD_US: u64 = 50_000;
    /// Per-plane copy cost assumed until measured: 16 B/us, well under what
    /// a cached PSRAM read achieves, so the first window check is the most
    /// conservative one.
    pub const PLANE_US_GUESS: u32 = (PLANE_BYTES / 16) as u32;
    const _: () = assert!(DESCS % ((1 << PLANES) - 1) == 0);

    /// `PLANES` plane spans. Both views share planes `1..PLANES` (the live
    /// framebuffer's own) and differ only in plane 0, the MSB: one names the
    /// framebuffer's, the other the spare block.
    pub struct PlaneView {
        planes: [(*const u8, usize); PLANES],
    }

    // SAFETY: raw pointers into leaked 'static DMA memory that only the
    // driver and `Hub75Output` ever address; the view is never aliased
    // mutably (it carries no data of its own).
    unsafe impl Send for PlaneView {}
    unsafe impl Sync for PlaneView {}

    impl FrameBuffer for PlaneView {
        type Word = u16;

        fn plane_count(&self) -> usize {
            PLANES
        }

        fn plane_ptr_len(&self, plane_idx: usize) -> (*const u8, usize) {
            self.planes[plane_idx]
        }
    }

    impl PlaneView {
        /// The plane spans, as copy destinations.
        pub fn planes(&self) -> [(*mut u8, usize); PLANES] {
            self.planes.map(|(p, n)| (p.cast_mut(), n))
        }
    }

    /// One internal framebuffer, one internal spare MSB block, one staging
    /// framebuffer (arena where there is one), and the two views. `None`
    /// disables output exactly like a framebuffer allocation failure. Where
    /// the staging buffer landed is the third element.
    #[allow(clippy::type_complexity)]
    pub fn alloc_all() -> Option<(&'static mut PlaneView, &'static mut PlaneView, &'static mut Fb, &'static str)> {
        let fb: &'static Fb = super::alloc_fb()?;
        let layout = Layout::from_size_align(PLANE_BYTES, 2).ok()?;
        // SAFETY: non-zero size; checked for null below.
        let spare = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if spare.is_null() {
            return None;
        }
        // Control bits (row address, OE/LAT placement) are the same in every
        // plane, so plane 0 of a formatted buffer is the spare's template.
        // SAFETY: both spans are `PLANE_BYTES` long, disjoint, and live.
        unsafe { core::ptr::copy_nonoverlapping(fb.plane_ptr_len(0).0, spare, PLANE_BYTES) };
        let (staging, place) = alloc_staging()?;
        let mut a = [(core::ptr::null::<u8>(), 0usize); PLANES];
        for (i, slot) in a.iter_mut().enumerate() {
            *slot = fb.plane_ptr_len(i);
        }
        let mut b = a;
        b[0] = (spare.cast_const(), PLANE_BYTES);
        let va = alloc::boxed::Box::leak(alloc::boxed::Box::new(PlaneView { planes: a }));
        let vb = alloc::boxed::Box::leak(alloc::boxed::Box::new(PlaneView { planes: b }));
        Some((va, vb, staging, place))
    }

    /// The compose target: a whole `Fb`, formatted, never a DMA source. Only
    /// task context touches it, so the PSRAM arena is the right home on a
    /// `psram-arena` board (`psram.rs`'s fence argument); elsewhere the heap.
    fn alloc_staging() -> Option<(&'static mut Fb, &'static str)> {
        let layout = Layout::new::<Fb>();
        #[cfg(feature = "psram-arena")]
        let (p, place) = (crate::psram::alloc_bulk_zeroed(layout), "arena");
        #[cfg(not(feature = "psram-arena"))]
        let (p, place) = (unsafe { alloc::alloc::alloc_zeroed(layout) }, "heap");
        if p.is_null() {
            return None;
        }
        // SAFETY: zeroed, correctly laid out, leaked.
        let fb = unsafe { &mut *p.cast::<Fb>() };
        fb.format();
        Some((fb, place))
    }

    /// Is there room in this pass for the copy? The arithmetic lives in
    /// `luxel_hub75::spare_window_fits`, where the host tests it against
    /// this board's ring geometry.
    pub fn window_fits(idx: usize, eof_pending: bool, nominal_us: u32, plane_us: u32) -> bool {
        luxel_hub75::spare_window_fits(idx, eof_pending, nominal_us, plane_us, DESCS, MSB_DESCS, PLANES, SLACK_NS)
    }
}

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

/// Build the boot-time panel→pixel remap for `m` (Gitea #475), leaked like
/// the framebuffers. `None` = the arrangement is already what the compose
/// path does natively (one upright tile, or any chain that comes out
/// row-major) or the 8 KiB table would not fit — either way the frame is
/// packed exactly as it was before this existed, with no per-pixel cost.
fn build_remap(m: &Matrix) -> Option<&'static [u16]> {
    let layout = core::alloc::Layout::array::<u16>(FB_PIXELS).ok()?;
    // zeroed: `build_lut` overwrites every entry, but a `&mut [u16]` may not
    // be made from uninitialised memory.
    let p = unsafe { alloc::alloc::alloc_zeroed(layout) }.cast::<u16>();
    if p.is_null() {
        println!("hub75: remap alloc failed — arrangement ignored");
        return None;
    }
    // SAFETY: `FB_PIXELS` zeroed, aligned `u16`s, never freed while borrowed.
    let lut: &'static mut [u16] = unsafe { core::slice::from_raw_parts_mut(p, FB_PIXELS) };
    arrange::build_lut(lut, m, PANEL_COLS, PANEL_ROWS);
    if arrange::is_identity(lut) {
        // SAFETY: same pointer and layout the allocation used; the only
        // reference to it dies here.
        unsafe { alloc::alloc::dealloc(p.cast::<u8>(), layout) };
        return None;
    }
    Some(lut)
}

/// What `GET /api/layout` reports about a matrix arrangement on this board:
/// the refresh the chain would rescan at, and how much of it this
/// framebuffer can drive.
pub fn panel_view(m: &Matrix) -> PanelView {
    PanelView {
        est_hz: arrange::est_hz(m, PLANES as u32, CLOCK.as_hz()),
        drive: arrange::driven_panels(m, PANEL_COLS, PANEL_ROWS) as u32,
    }
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
    hub75: Option<Hub75<Blocking, DmaFb>>,
    /// The compose target while no swap is in flight (spare-plane mode: the
    /// view whose MSB block is idle).
    back: Option<&'static mut DmaFb>,
    /// The previous frame's swap; waited (instant by then) at the start
    /// of the next `write_frame` to reclaim the displaced buffer.
    pending: Option<Hub75Swap<DmaFb>>,
    /// Spare-plane mode (Gitea #610): the compose target. The live DMA
    /// framebuffer is written only by `flush`, inside the window.
    #[cfg(feature = "hub75-spare-plane")]
    staging: Option<&'static mut Fb>,
    /// A composed frame is in `staging`, waiting for its window.
    #[cfg(feature = "hub75-spare-plane")]
    staged: bool,
    /// When that frame was composed — the liveness floor's clock.
    #[cfg(feature = "hub75-spare-plane")]
    staged_at: esp_hal::time::Instant,
    /// Slowest single-plane copy seen, microseconds; sizes the window check.
    #[cfg(feature = "hub75-spare-plane")]
    plane_us: u32,
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
    /// The boot-time panel→pixel remap (Gitea #475). `None` = the configured
    /// arrangement IS the driver's own row-major order, so nothing is
    /// gathered and the compose path is byte-for-byte what it always was.
    remap: Option<&'static [u16]>,
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
            remap: None,
            #[cfg(feature = "hub75-spare-plane")]
            staging: None,
            #[cfg(feature = "hub75-spare-plane")]
            staged: false,
            #[cfg(feature = "hub75-spare-plane")]
            staged_at: esp_hal::time::Instant::EPOCH,
            #[cfg(feature = "hub75-spare-plane")]
            plane_us: spare::PLANE_US_GUESS,
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
        // Buffers. Two full framebuffers the DMA alternates between (#376),
        // or — spare-plane mode (#610) — one framebuffer plus a spare MSB
        // block, both internal, and a staging framebuffer only the compose
        // writes.
        #[cfg(not(feature = "hub75-spare-plane"))]
        let (front, back) = match (alloc_fb(), alloc_fb()) {
            (Some(f), Some(b)) => (f, b),
            _ => {
                println!("hub75: framebuffer alloc failed — panel output disabled");
                return dead;
            }
        };
        #[cfg(feature = "hub75-spare-plane")]
        let (front, back, mut staging) = match spare::alloc_all() {
            Some((a, b, st, place)) => {
                println!(
                    "hub75: spare-plane swap — framebuffer {} B + spare MSB plane {} B internal, \
                     staging {} B in the {}",
                    core::mem::size_of::<Fb>(),
                    spare::PLANE_BYTES,
                    core::mem::size_of::<Fb>(),
                    place,
                );
                (a, b, Some(st))
            }
            None => {
                println!("hub75: framebuffer alloc failed — panel output disabled");
                return dead;
            }
        };
        #[cfg(not(feature = "hub75-spare-plane"))]
        let layout_ok = probe_layout(back);
        #[cfg(feature = "hub75-spare-plane")]
        let layout_ok = staging.as_deref_mut().is_some_and(probe_layout);
        // Bulk packer (Gitea #329): used only if its layout assumption holds
        // on the buffer we actually got, and if its 2 KiB of tables fit.
        let tables = if layout_ok {
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
        // The configured arrangement (#475). `layout::init()` has already run
        // (main.rs wires the panel after it), so this is the stored Layout.
        let m = crate::layout::matrix();
        let remap = build_remap(&m);
        let view = panel_view(&m);
        println!(
            "hub75: {}x{} tiles of {}x{} from {} {}{}{}, driving {}, remap {}, est {} Hz",
            m.cols,
            m.rows,
            m.pw,
            m.ph,
            m.start.as_str(),
            m.dir.as_str(),
            if m.snake { " snake" } else { "" },
            if m.rot180 { " rot180" } else { "" },
            view.drive,
            if remap.is_some() { "on" } else { "off (row-major)" },
            view.est_hz,
        );
        match Hub75::new(lcd_cam, pins, channel, tx_descriptors, CLOCK, &*front) {
            Ok(h) => {
                // Descriptor cost is worth printing once: the frame-atomic
                // swap (#376) doubles it, and it is static DMA-capable RAM.
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
                    remap,
                    #[cfg(feature = "hub75-spare-plane")]
                    staging,
                    #[cfg(feature = "hub75-spare-plane")]
                    staged: false,
                    #[cfg(feature = "hub75-spare-plane")]
                    staged_at: esp_hal::time::Instant::EPOCH,
                    #[cfg(feature = "hub75-spare-plane")]
                    plane_us: spare::PLANE_US_GUESS,
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
        #[cfg(feature = "hub75-spare-plane")]
        {
            // The staging buffer is the only thing `write_frame` touches; the
            // swap's landing is `flush`'s business (Gitea #610).
            !self.staged
        }
        #[cfg(not(feature = "hub75-spare-plane"))]
        {
            self.pending.as_ref().is_none_or(Hub75Swap::is_done)
        }
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
        // Spare-plane mode (Gitea #610): compose into the staging buffer and
        // stop — `flush` copies it into the live buffer inside the window.
        #[cfg(feature = "hub75-spare-plane")]
        {
            if self.staged {
                return false;
            }
            let Some(staging) = self.staging.take() else { return false };
            compose_into(&mut self.tables, &mut self.tables_b5, self.remap, staging, rgb, brightness5);
            self.staging = Some(staging);
            self.staged = true;
            self.staged_at = esp_hal::time::Instant::now();
            true
        }
        #[cfg(not(feature = "hub75-spare-plane"))]
        {
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
            compose_into(&mut self.tables, &mut self.tables_b5, self.remap, back, rgb, brightness5);
            note_handoff(&mut self.last_shown_rescan, &mut self.next_seq, hub75);
            self.pending = Some(hub75.swap(back));
            true
        }
    }

    /// Spare-plane mode (Gitea #610): copy the staged frame into the live
    /// buffer if this pass has room for it. See the module docs for the
    /// window and the deadlines.
    #[cfg(feature = "hub75-spare-plane")]
    fn flush(&mut self) -> bool {
        use crate::shared;
        if !self.staged {
            return true;
        }
        let Some(hub75) = self.hub75.as_ref() else {
            self.staged = false;
            return true;
        };
        // Liveness floor: no window within HOLD_US means the panel stopped
        // passing (a dead DMA). Drop the frame; never freeze the engine.
        if self.staged_at.elapsed().as_micros() >= spare::HOLD_US {
            self.staged = false;
            shared::SPARE_ABANDONED.fetch_add(1, Ordering::Relaxed);
            return true;
        }
        // The previous flip must have landed: only then is the DMA on the
        // other ring and the displaced view's MSB block provably idle.
        let back = match self.pending.take() {
            Some(swap) => {
                if !swap.is_done() {
                    self.pending = Some(swap);
                    return false;
                }
                match swap.wait() {
                    Ok(v) => v,
                    Err((e, v)) => {
                        println!("hub75: swap error: {:?}", e);
                        v
                    }
                }
            }
            None => match self.back.take() {
                Some(v) => v,
                None => {
                    self.staged = false;
                    return true;
                }
            },
        };
        // Is the DMA inside the MSB run of this pass, with room?
        let Some((ring, idx, eof_pending)) = hub75.dma_position() else {
            self.back = Some(back);
            shared::SPARE_DEFERRED.fetch_add(1, Ordering::Relaxed);
            return false;
        };
        let nominal_us = hub75.pass_stats().3;
        if !spare::window_fits(idx, eof_pending, nominal_us, self.plane_us) {
            self.back = Some(back);
            shared::SPARE_DEFERRED.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let Some(staging) = self.staging.as_deref() else {
            self.staged = false;
            return true;
        };
        // Destinations: planes 1.. are the live buffer's own (both views
        // name the same memory), plane 0 is the idle spare block.
        let dst = back.planes();
        // Arm the flip FIRST. It lands at the wrap — the patch's single tail
        // store, taken while the DMA is provably short of the tail — so the
        // next pass reads the other view whatever happens below. Arming after
        // the copy could only make things worse: the shared planes would
        // already be new against the old MSB.
        note_handoff(&mut self.last_shown_rescan, &mut self.next_seq, hub75);
        let swap = hub75.swap(back);
        let t0 = esp_hal::time::Instant::now();
        let mut worst: u32 = 0;
        for (p, &(d, len)) in dst.iter().enumerate().skip(1) {
            let (src, slen) = staging.plane_ptr_len(p);
            debug_assert_eq!(len, slen);
            let t = esp_hal::time::Instant::now();
            // SAFETY: both spans are `len` bytes, live, and disjoint — the
            // staging buffer is never a DMA source and the live plane is idle
            // for the rest of this pass (window check above).
            unsafe { core::ptr::copy_nonoverlapping(src, d, len) };
            worst = worst.max(t.elapsed().as_micros() as u32);
            if p == 1 {
                // The tight deadline: plane 1 is the first thing the DMA reads
                // after the MSB run. Still in the run on the same ring?
                let ok = hub75.dma_position().is_some_and(|(r, i, _)| r == ring && i < spare::MSB_DESCS);
                if !ok {
                    shared::SPARE_TORN_P1.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        {
            let (src, slen) = staging.plane_ptr_len(0);
            let (d, len) = dst[0];
            debug_assert_eq!(len, slen);
            let t = esp_hal::time::Instant::now();
            // SAFETY: as above; the spare block is in neither descriptor
            // ring the DMA can reach before the flip lands.
            unsafe { core::ptr::copy_nonoverlapping(src, d, len) };
            worst = worst.max(t.elapsed().as_micros() as u32);
        }
        // The last deadline: the spare MSB must be in place before the wrap
        // the flip lands on. A ring change here means it was not.
        if hub75.dma_position().is_none_or(|(r, _, _)| r != ring) {
            shared::SPARE_TORN_WRAP.fetch_add(1, Ordering::Relaxed);
        }
        let total = t0.elapsed().as_micros() as u32;
        self.plane_us = self.plane_us.max(worst);
        shared::SPARE_PLANE_US.store(self.plane_us, Ordering::Relaxed);
        shared::SPARE_COPY_US.store(total, Ordering::Relaxed);
        shared::SPARE_COPY_US_MAX.fetch_max(total, Ordering::Relaxed);
        shared::SPARE_FLUSHES.fetch_add(1, Ordering::Relaxed);
        self.pending = Some(swap);
        self.staged = false;
        true
    }
}

/// Bookkeeping at the one point that knows a frame is going out: the rescans
/// since the previous hand-off (Gitea #395) and the tag that makes the
/// driver's shown-log read back as our sequence. Takes the two counters
/// rather than `&mut self` so the caller can keep its driver borrow.
fn note_handoff(last_shown_rescan: &mut u32, next_seq: &mut u32, hub75: &Hub75<Blocking, DmaFb>) {
    // Rescans between consecutive DISPLAYED frames: 1 = every rescan showed
    // something new, 2 = one repeat.
    let rescans = hub75.frame_count();
    if *last_shown_rescan != 0 {
        let d = rescans.wrapping_sub(*last_shown_rescan);
        crate::shared::PASS_PER_FRAME_MAX.fetch_max(d, Ordering::Relaxed);
        crate::shared::PASS_PER_FRAME_MIN.fetch_min(d, Ordering::Relaxed);
        if d == 0 {
            // Two frames handed over inside one rescan: the first was never
            // scanned out. This is the skip, caught at the source.
            crate::shared::PASS_ZERO_RESCAN.fetch_add(1, Ordering::Relaxed);
        }
    }
    *last_shown_rescan = rescans;
    // Tag this frame so the driver's log of DISPLAYED passes reads back as
    // our own sequence — no pointer mapping to go stale.
    let seq = *next_seq;
    *next_seq = next_seq.wrapping_add(1).max(1);
    hub75.set_swap_tag(seq);
}

/// Compose `rgb` at `brightness5` into `target`.
///
/// brightness5: no APA102-style hardware field on HUB75 — scale channels in
/// software exactly like the WS2812 path.
///
/// The bulk packer (Gitea #329) walks the frame per ROW PAIR, building all
/// PLANES entry words for a pixel pair from one pair of table lookups per
/// channel, with brightness folded into those tables. It writes every colour
/// bit of every entry, so it subsumes the erase. The per-pixel path is the
/// fallback for a framebuffer whose layout the boot probe did not recognise.
fn compose_into(
    tables: &mut Option<&'static mut luxel_hub75::Tables>,
    tables_b5: &mut u8,
    remap: Option<&'static [u16]>,
    target: &mut Fb,
    rgb: &[[u8; 3]],
    brightness5: u8,
) {
    if let Some(t) = tables.as_deref_mut() {
        if *tables_b5 != brightness5 {
            t.build(&brightness_lut(brightness5));
            *tables_b5 = brightness5;
        }
    }
    match (tables.as_deref(), remap) {
        (Some(tables), None) => {
            luxel_hub75::pack::<NROWS, PANEL_COLS, PLANES>(fb_words(target), rgb, tables);
        }
        (Some(tables), Some(lut)) => {
            let fb = fb_words(target);
            luxel_hub75::pack_remap::<NROWS, PANEL_COLS, PLANES>(fb, rgb, lut, tables);
        }
        (None, remap) => {
            target.erase();
            // At 0 the erase above already produced the all-black frame.
            if brightness5 > 0 {
                let full = brightness5 >= 31;
                for i in 0..FB_PIXELS {
                    // Driver pixel i takes its colour from engine pixel
                    // `lut[i]`; unmapped and short frames stay erased.
                    let src = match remap {
                        Some(l) => usize::from(l[i]),
                        None => i,
                    };
                    let Some([r, g, b]) = rgb.get(src).copied() else { continue };
                    let (r, g, b) = if full {
                        (r, g, b)
                    } else {
                        (scale5(r, brightness5), scale5(g, brightness5), scale5(b, brightness5))
                    };
                    let p = Point::new((i % PANEL_COLS) as i32, (i / PANEL_COLS) as i32);
                    target.set_pixel(p, Color::new(r, g, b));
                }
            }
        }
    }
}
