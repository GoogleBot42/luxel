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
//! ## Everything is a setting now (Gitea #401 + #525)
//!
//! The panel used to be compile-time: one const geometry, one bit depth, one
//! clock, one control template. All of it is read from the stored Layout at
//! boot instead — the `matrix` line for the arrangement and the `panel` line
//! for how it is DRIVEN (`planes`, `clock_mhz`, `chip`, `blank`) — so one
//! image drives any panel the RAM fits. Consequences:
//!
//! * the framebuffer is [`DynFb`], a heap-leaked `[u16]` with a runtime
//!   [`Geometry`], not a const-generic `DmaFrameBuffer`. The control template
//!   (row address, latch, output-enable) is written by
//!   [`luxel_hub75::format`], which is byte-identical to
//!   `DmaFrameBuffer::new()`'s at `Control::default()` — a `cargo test`
//!   assertion in `luxel-hub75`, not something only the panel can tell us;
//! * the DMA descriptor ring is heap-leaked too, sized from the geometry
//!   (`hub75_dma_descriptors!` is a compile-time static for a const type);
//! * a panel with a driver CHIP gets its register-init sequence bit-banged on
//!   the real pins before the LCD_CAM takes them over
//!   ([`luxel_hub75::chip`]);
//! * anything that does not fit, or does not initialise, FALLS BACK once to
//!   the board default (64x64, 7 planes, 30 MHz, shiftreg, blank 1) and says
//!   so in [`panel_view`]'s `live.fallback`. Only if that fails too does
//!   output stay disabled (the pre-#401 behaviour), with the render loop
//!   still ticking.
//!
//! What actually booted is recorded in [`LIVE`] and reported as the `live`
//! half of `GET /api/layout`'s `driver` block, so a UI can say "reboot to
//! apply" by comparing it against the stored Layout.
//!
//! ## Frame-atomic swap
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
//! Framebuffers are DMA targets and must live in internal SRAM — which is
//! what the GLOBAL heap is on every board here (the PSRAM arena is a second,
//! separate heap; see `psram.rs`). Two `planes`-deep bitplane buffers
//! (~28 KB each at 64x64/7-plane) don't fit the S3's leftover `.stack`
//! region as statics, so they're heap-leaked at construction time instead —
//! main() wiring runs before the WiFi blob's boot mallocs, when the heap is
//! fresh enough that two contiguous 28 KB blocks are a certainty.
//! Allocation failure falls back, then disables output (render keeps
//! ticking) rather than panicking.
//!
//! ## Spare-plane swap (`hub75-spare-plane`, Gitea #610)
//!
//! The same frame-atomic guarantee as the two-buffer swap — nothing is ever
//! written where the DMA can read it — for one framebuffer plus ONE extra
//! plane instead of two framebuffers. The circular ring is plane-major and
//! plane 0 is the MSB, repeated `2^(planes-1)` times, so the first half of
//! every pass reads nothing but plane 0; planes `1..planes` are idle then.
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

use core::alloc::Layout as AllocLayout;
use core::cell::Cell;
use core::sync::atomic::{AtomicU16, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_hal::dma::DmaDescriptor;
use esp_hal::peripherals::{DMA_CH0, LCD_CAM};
use esp_hal::time::Rate;
use esp_hal::Blocking;
use esp_hub75::framebuffer::FrameBuffer;
use esp_hub75::{Hub75, Hub75Pins16, Hub75Swap};
use esp_println::println;

use luxel_core::layout::{Chip, LiveDriver, Matrix, PanelDriver, PanelView};
use luxel_hub75::chip::ChipInit;
use luxel_hub75::{arrange, Control, Geometry, Scratch, Tables};

use crate::leds::{scale5, Protocol};
use crate::output::OutputDriver;

/// The panel a HUB75 board comes up on with nothing stored — and the shape it
/// falls back to when the configured one does not fit. Every other geometry
/// is a runtime setting (the `matrix` line); this is only the default, and
/// `board::PANEL_PIXELS` (the board's default pixel count) is its area.
pub const DEFAULT_PANEL_W: u16 = 64;
/// See [`DEFAULT_PANEL_W`].
pub const DEFAULT_PANEL_H: u16 = 64;

/// HUB75 drives two half-height rows at once through R1G1B1/R2G2B2 and has
/// five address lines (A..E), so no arrangement can scan deeper than this.
const MAX_SCAN: usize = 32;

/// The arrangement a board with nothing stored is, and the one a boot falls
/// back to.
pub fn board_default_matrix() -> Matrix {
    Matrix::single(DEFAULT_PANEL_W, DEFAULT_PANEL_H)
}

/// What the running DMA was actually built with (#525) — the `live` half of
/// `GET /api/layout`'s `driver` block. Written once, at boot, before the HTTP
/// server exists; `None` = no panel output at all.
static LIVE: BlockingMutex<CriticalSectionRawMutex, Cell<Option<LiveDriver>>> =
    BlockingMutex::new(Cell::new(None));

/// The live scan depth, duplicated out of [`LIVE`] as a plain atomic because
/// the power model reads it on the per-frame path (`crate::power_model`) and a
/// critical section there would be paid 100+ times a second for a value that
/// never changes after boot. 0 = no panel output.
static LIVE_SCAN: AtomicU16 = AtomicU16::new(0);

/// LCD_CAM pixel-clock rate — the `clock_mhz` setting of the `panel` line.
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
/// | 30 MHz | 115       | clean — the default                        |
/// | 40 MHz | 154       | **fails**: mid-panel split, colours wrong  |
///
/// 40 MHz is out of spec and looks it: the two 32-row halves mis-sample and
/// the colours distort. Note the firmware sees **nothing** wrong when that
/// happens — no swap error, no DMA error, `vmerr` null, and the composed
/// frame is still byte-identical to a host render. Only the panel's own
/// sampling fails, so this class of regression needs an eyeball, not a test.
/// That is why the setting's range is the generous 2..=40 the wire accepts
/// and the WARNING above 30 lives in the UI: which panel is plugged in is
/// not something the firmware can know.
///
/// A faster clock buys no frame throughput (every pattern here is
/// render-bound, not rescan-bound; fps was identical at all three rates).
/// What it buys is headroom: 8 bitplanes become usable (~58 Hz rather than
/// ~38), and chained panels get the bandwidth they need (#255).
fn clock_rate(d: &PanelDriver) -> Rate {
    Rate::from_mhz(u32::from(d.clock_mhz))
}

/// The control template the stored driver implies.
fn control_of(d: &PanelDriver) -> Control {
    Control { blank: d.blank, latch_clocks: d.latch_clocks() }
}

/// A raw internal-SRAM block owned until it is [`Block::leak`]ed.
///
/// A boot attempt allocates several of these (framebuffers, descriptors,
/// tables) and any of them can fail. Whatever succeeded has to go BACK on the
/// heap before the fallback attempt runs, or the fallback is asked to fit a
/// smaller panel into a heap the bigger one is still holding — so ownership,
/// not `leak()` at the allocation site.
struct Block {
    ptr: *mut u8,
    layout: AllocLayout,
}

impl Block {
    /// Zeroed, from the GLOBAL heap — internal SRAM on every board here, and
    /// that is a requirement, not a preference: the DMA reads it.
    fn zeroed(layout: AllocLayout) -> Option<Self> {
        if layout.size() == 0 {
            return None;
        }
        // SAFETY: non-zero size; the null return is the failure signal.
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr, layout })
        }
    }

    /// Give the block up to `'static`. Nothing frees it after this.
    fn leak(self) -> *mut u8 {
        let p = self.ptr;
        core::mem::forget(self);
        p
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        // SAFETY: same pointer and layout the allocation used, and the only
        // reference to it dies with `self`.
        unsafe { alloc::alloc::dealloc(self.ptr, self.layout) };
    }
}

/// A HUB75 BCM framebuffer whose shape is a runtime [`Geometry`] (#401).
///
/// `plane -> row -> column` 16-bit LCD_CAM bus words, one flat allocation, in
/// internal SRAM. This is what `hub75-framebuffer`'s const-generic
/// `DmaFrameBuffer<NROWS, COLS, PLANES>` was: the same layout, the same
/// control template ([`luxel_hub75::format`]), the same descriptor chunking —
/// only the dimensions moved to run time.
pub struct DynFb {
    g: Geometry,
    words: *mut u16,
}

// SAFETY: the pointer addresses leaked 'static DMA memory that only the
// driver and `Hub75Output` ever reach, exactly as the const-generic
// framebuffer's `&'static mut` did.
unsafe impl Send for DynFb {}
unsafe impl Sync for DynFb {}

impl FrameBuffer for DynFb {
    type Word = u16;

    fn plane_count(&self) -> usize {
        self.g.planes
    }

    fn plane_ptr_len(&self, plane_idx: usize) -> (*const u8, usize) {
        let bytes = self.g.plane_bytes();
        // SAFETY: `plane_idx < planes` (the trait's contract) and the
        // allocation is `planes * bytes` long.
        (unsafe { self.words.cast::<u8>().add(plane_idx * bytes) }, bytes)
    }
}

impl DynFb {
    /// Allocate, zero and [`luxel_hub75::format`] one framebuffer.
    ///
    /// The 16-byte `DynFb` header is leaked (it is the `&'static` the DMA
    /// driver's signature demands); the BUFFER comes back as an owned
    /// [`Block`] so a boot attempt that fails further along hands the heap
    /// back for the fallback attempt. Call [`Block::leak`] on it once the
    /// driver is running.
    fn alloc(g: Geometry, c: Control) -> Option<(Block, &'static mut DynFb)> {
        let block = Block::zeroed(Self::buffer_layout(g)?)?;
        let words = Self::format_words(block.ptr, g, c);
        Some((block, alloc::boxed::Box::leak(alloc::boxed::Box::new(DynFb { g, words }))))
    }

    /// Allocate a framebuffer through someone else's allocator and leak it —
    /// spare-plane mode's staging buffer, which comes from the PSRAM arena on
    /// a `psram-arena` board (it is never a DMA source, and the arena has no
    /// `free`).
    #[cfg(feature = "hub75-spare-plane")]
    fn alloc_words_leaked(
        g: Geometry,
        c: Control,
        alloc_zeroed: impl FnOnce(AllocLayout) -> Option<*mut u8>,
    ) -> Option<*mut u16> {
        let p = alloc_zeroed(Self::buffer_layout(g)?)?;
        Some(Self::format_words(p, g, c))
    }

    /// The layout one framebuffer's buffer needs. Align 4: the LCD_CAM's GDMA
    /// wants word-aligned sources, and `u16` alignment alone would let an
    /// odd-sized geometry land on a halfword.
    fn buffer_layout(g: Geometry) -> Option<AllocLayout> {
        AllocLayout::from_size_align(g.bytes(), 4).ok()
    }

    /// Write the control template into a freshly zeroed buffer.
    fn format_words(p: *mut u8, g: Geometry, c: Control) -> *mut u16 {
        let words = p.cast::<u16>();
        // SAFETY: the caller allocated `g.bytes()` zeroed, 4-byte-aligned
        // bytes at `p` and holds the only reference to them.
        let flat = unsafe { core::slice::from_raw_parts_mut(words, g.words()) };
        luxel_hub75::format(flat, g, c);
        words
    }

    /// The framebuffer as the flat entry array the packer writes.
    fn fb_words(&mut self) -> &mut [u16] {
        // SAFETY: the allocation is `g.words()` contiguous `u16`s and `self`
        // is its only owner.
        unsafe { core::slice::from_raw_parts_mut(self.words, self.g.words()) }
    }
}

/// Does the template [`luxel_hub75::format`] just wrote actually light the
/// panel, and is it the shape the packer preserves?
///
/// The const-generic era checked this by writing three pixels through
/// `hub75-framebuffer`'s own `set_pixel` and seeing where they landed — the
/// question was whether a third-party feature flag (`esp32-ordering`,
/// `inter-row-blank-*`) had changed the layout under us. There is no
/// third-party framebuffer any more: `format` and `pack` are both
/// `luxel-hub75`, tested against each other on the host, so that question is
/// answered there.
///
/// What CANNOT be answered there is whether the configured `blank` and the
/// chip's latch width leave any room in a row block this narrow. With
/// `cols = 16`, `blank = 8` and a DP3246's 3 latch clocks the OE window is
/// empty and the panel is simply black — a legal `panel` line that cannot
/// drive, so it must trip the fallback rather than ship a dark panel. Also
/// asserts the colour bits came out clear, which is what makes the first
/// composed frame exact.
fn template_lights(words: &[u16], g: Geometry, c: Control) -> bool {
    if words.len() != g.words() || g.rows == 0 || g.cols == 0 || g.planes == 0 {
        return false;
    }
    let block = &words[..g.cols];
    let lit = block.iter().any(|w| w & luxel_hub75::OE_ACTIVE != 0);
    let latched = block.iter().any(|w| w & luxel_hub75::LATCH != 0);
    let clean = words.iter().all(|w| w & luxel_hub75::COLOR_MASK == 0);
    if !lit || !latched {
        println!(
            "hub75: blank {} + {} latch clocks leave no {} in a {}-word row block",
            c.blank,
            c.latch_clocks,
            if lit { "latch" } else { "lit clocks" },
            g.cols,
        );
    }
    lit && latched && clean
}

/// Heap-allocate the packer's spread tables (2 KiB). `Tables::zeroed()` is
/// all-zero bytes, so a zeroed block is a valid value without a 2 KiB stack
/// temporary.
fn alloc_tables() -> Option<(Block, &'static mut Tables)> {
    let layout = AllocLayout::new::<Tables>();
    let block = Block::zeroed(layout)?;
    let p = block.ptr.cast::<Tables>();
    // SAFETY: zeroed is a valid `Tables`, correctly laid out and aligned; the
    // borrow lives as long as the block, which the caller leaks on success.
    Some((block, unsafe { &mut *p }))
}

/// One ring's worth of DMA descriptors for `g`, as esp-hub75 counts them.
fn descs_per_ring(g: Geometry) -> usize {
    esp_hub75::dma_descriptor_count(g.planes, g.plane_bytes())
}

/// Heap-allocate the descriptor rings. `hub75_dma_descriptors!` is a
/// compile-time static sized for a const framebuffer type, so a runtime
/// geometry has to bring its own — [`esp_hub75::DESCRIPTOR_RINGS`] of them,
/// because the frame-atomic swap (#376) needs one ring per framebuffer and
/// `CircularBcmBuf::ring_count` derives that from the slice length.
/// Descriptors must be in internal RAM (the peripheral walks them) and
/// 4-byte aligned, both of which `Layout::array` over the type gives.
fn alloc_descriptors(g: Geometry) -> Option<(Block, &'static mut [DmaDescriptor])> {
    let n = esp_hub75::DESCRIPTOR_RINGS * descs_per_ring(g);
    let layout = AllocLayout::array::<DmaDescriptor>(n).ok()?;
    let block = Block::zeroed(layout)?;
    let p = block.ptr.cast::<DmaDescriptor>();
    for i in 0..n {
        // SAFETY: `i < n` and the block is `n` correctly-aligned
        // descriptors; `DmaDescriptor` has no Drop, so a plain write over
        // zeroed memory is a complete initialisation.
        unsafe { p.add(i).write(DmaDescriptor::EMPTY) };
    }
    // SAFETY: `n` initialised, aligned descriptors, leaked on success.
    Some((block, unsafe { core::slice::from_raw_parts_mut(p, n) }))
}

/// Spare-plane swap (Gitea #610): the views and the window maths, at runtime
/// dimensions. Everything that was a `const` derived from the framebuffer
/// TYPE is now a field of [`Window`] derived from the [`Geometry`].
#[cfg(feature = "hub75-spare-plane")]
mod spare {
    use esp_hub75::framebuffer::FrameBuffer;
    use luxel_hub75::{Geometry, MAX_PLANES};

    /// Margin the window check keeps beyond the measured copy cost.
    pub const SLACK_NS: u64 = 300_000;
    /// A staged frame no window opened for within this long is abandoned
    /// rather than freezing the engine behind it — the same liveness floor
    /// as the output task's vsync hold (a dead DMA must not stop the world).
    pub const HOLD_US: u64 = 50_000;

    /// The ring geometry the window check needs, computed once at boot.
    #[derive(Clone, Copy)]
    pub struct Window {
        /// Descriptors in one ring.
        pub descs: usize,
        /// Descriptors from the ring head to the end of the MSB run: plane 0
        /// emitted `2^(planes-1)` times.
        pub msb_descs: usize,
        pub planes: usize,
    }

    impl Window {
        pub fn new(g: Geometry, descs: usize) -> Self {
            let per_plane = descs / ((1 << g.planes) - 1);
            Self { descs, msb_descs: per_plane << (g.planes - 1), planes: g.planes }
        }

        /// Is there room in this pass for the copy? The arithmetic lives in
        /// `luxel_hub75::spare_window_fits`, where the host tests it against
        /// this board's ring geometry.
        pub fn fits(&self, idx: usize, eof_pending: bool, nominal_us: u32, plane_us: u32) -> bool {
            luxel_hub75::spare_window_fits(
                idx,
                eof_pending,
                nominal_us,
                plane_us,
                self.descs,
                self.msb_descs,
                self.planes,
                SLACK_NS,
            )
        }
    }

    /// Per-plane copy cost assumed until measured: 16 B/us, well under what
    /// a cached PSRAM read achieves, so the first window check is the most
    /// conservative one.
    pub fn plane_us_guess(g: Geometry) -> u32 {
        (g.plane_bytes() / 16) as u32
    }

    /// `g.planes` plane spans. Both views share planes `1..planes` (the live
    /// framebuffer's own) and differ only in plane 0, the MSB: one names the
    /// framebuffer's, the other the spare block.
    pub struct PlaneView {
        planes: [(*const u8, usize); MAX_PLANES],
        n: usize,
    }

    // SAFETY: raw pointers into leaked 'static DMA memory that only the
    // driver and `Hub75Output` ever address; the view is never aliased
    // mutably (it carries no data of its own).
    unsafe impl Send for PlaneView {}
    unsafe impl Sync for PlaneView {}

    impl FrameBuffer for PlaneView {
        type Word = u16;

        fn plane_count(&self) -> usize {
            self.n
        }

        fn plane_ptr_len(&self, plane_idx: usize) -> (*const u8, usize) {
            self.planes[plane_idx]
        }
    }

    impl PlaneView {
        /// Build the pair of views over `fb`'s planes, the second with `spare`
        /// standing in for plane 0.
        pub fn pair(
            fb: &dyn FrameBuffer<Word = u16>,
            spare: *const u8,
            g: Geometry,
        ) -> (PlaneView, PlaneView) {
            let mut a = [(core::ptr::null::<u8>(), 0usize); MAX_PLANES];
            for (i, slot) in a.iter_mut().enumerate().take(g.planes) {
                *slot = fb.plane_ptr_len(i);
            }
            let mut b = a;
            b[0] = (spare, g.plane_bytes());
            (PlaneView { planes: a, n: g.planes }, PlaneView { planes: b, n: g.planes })
        }

        /// The plane spans, as copy destinations. Only the first
        /// [`FrameBuffer::plane_count`] entries are meaningful.
        pub fn planes(&self) -> [(*mut u8, usize); MAX_PLANES] {
            self.planes.map(|(p, n)| (p.cast_mut(), n))
        }
    }
}

/// What the DMA driver is built against. Normally the framebuffer itself;
/// in spare-plane mode a view over it (Gitea #610).
#[cfg(feature = "hub75-spare-plane")]
type DmaFb = spare::PlaneView;
#[cfg(not(feature = "hub75-spare-plane"))]
type DmaFb = DynFb;

/// Build the boot-time panel→pixel remap for `m` (Gitea #475), leaked like
/// the framebuffers. `None` = the arrangement is already what the compose
/// path does natively (one upright tile, or any chain that comes out
/// row-major) or the table would not fit — either way the frame is packed
/// exactly as it was before this existed, with no per-pixel cost.
///
/// `fb_w`/`fb_h` are the framebuffer's extent in PANEL pixels, which for a
/// 1/N-scan panel is not the driver's own `cols`/`rows`: the driver array is
/// `stripes` times wider and `stripes` times shallower, and `build_lut` folds
/// that back itself.
fn build_remap(m: &Matrix, g: Geometry) -> Option<&'static [u16]> {
    let stripes = arrange::stripes(m);
    let (fb_w, fb_h) = (g.cols / stripes, 2 * g.rows * stripes);
    let layout = AllocLayout::array::<u16>(g.pixels()).ok()?;
    // zeroed: `build_lut` overwrites every entry, but a `&mut [u16]` may not
    // be made from uninitialised memory.
    let block = Block::zeroed(layout)?;
    let p = block.ptr.cast::<u16>();
    // SAFETY: `g.pixels()` zeroed, aligned `u16`s; the block is leaked below
    // if the table is kept, and freed by `Block::drop` if it is not.
    let lut: &mut [u16] = unsafe { core::slice::from_raw_parts_mut(p, g.pixels()) };
    arrange::build_lut(lut, m, fb_w, fb_h);
    if arrange::is_identity(lut) {
        return None;
    }
    let p = block.leak().cast::<u16>();
    // SAFETY: as above, now `'static`.
    Some(unsafe { core::slice::from_raw_parts(p, g.pixels()) })
}

/// What the running driver was built with — the `live` block of
/// `GET /api/layout`'s `driver`. `None` = no panel output.
pub fn live_driver() -> Option<LiveDriver> {
    LIVE.lock(Cell::get)
}

/// The live scan depth for the power model (`PowerModel::Hub75`), or the
/// board default's while there is no panel output — a HUB75 board's power
/// model must describe a time-multiplexed panel either way.
pub fn live_scan() -> u16 {
    match LIVE_SCAN.load(Ordering::Relaxed) {
        0 => DEFAULT_PANEL_H / 2,
        n => n,
    }
}

/// What `GET /api/layout` reports about a matrix arrangement on this board:
/// the refresh the CONFIGURED chain would rescan at, how much of it the
/// RUNNING framebuffer can drive, and what that framebuffer actually is.
pub fn panel_view(m: &Matrix) -> PanelView {
    let d = crate::layout::driver();
    let live = live_driver();
    // `drive` is about the framebuffer that exists, not the one configured:
    // with no panel output nothing is driven at all.
    let (fb_w, fb_h) = live.map_or((0, 0), |l| (l.w as usize, l.h as usize));
    PanelView {
        est_hz: arrange::est_hz(m, u32::from(d.planes), d.clock_hz()),
        drive: arrange::driven_panels(m, fb_w, fb_h) as u32,
        driver_live: live,
    }
}

/// The channel LUT the packer folds into its tables: what the panel is
/// actually driven with for each source value. Exactly the arithmetic the
/// per-pixel path did inline, so the two are byte-identical by construction
/// (asserted in `luxel-hub75`'s tests against `hub75-framebuffer` itself).
fn brightness_lut(brightness5: u8) -> [u8; 256] {
    let mut lut = [0u8; 256];
    let full = brightness5 >= 31;
    for (c, v) in lut.iter_mut().enumerate() {
        let c = c as u8;
        *v = if full { c } else { scale5(c, brightness5) };
    }
    lut
}

/// Walk a driver chip's register-init sequence on the real pins, before the
/// LCD_CAM takes them over (Gitea #525).
///
/// One [`luxel_hub75::chip::Step`] is "establish these levels, then pulse the
/// clock", so this is a direct transcription: six colour lines to one level,
/// LAT, OE (`oe: true` = pin HIGH = display DISABLED), then CLK high/low. The
/// pins are REBORROWED from the `Hub75Pins16` the caller still owns —
/// `AnyPin::reborrow` hands out a shorter-lived handle, the nine `Output`s
/// are dropped at the end of this function and the pins go on to
/// `Hub75::new` untouched. Nothing is stolen and no pad has two owners.
///
/// The C++ reference (`ESP32-HUB75-MatrixPanel-I2S-DMA`'s `fm6124init`) uses
/// no delays at all, but it pays an Arduino `digitalWrite` per edge. A direct
/// `set_level` here is a single register store, which would put CLK's rising
/// and falling edges ~10 ns apart on a 240 MHz S3 — under the 20 ns minimum
/// clock high/low an FM6124-class part wants. Hence the 100 ns holds: still
/// only ~0.1 ms for a 64-wide chain's 194 clocks, once, at boot.
///
/// Returns the number of clocks emitted.
fn chip_init(pins: &mut Hub75Pins16<'static>, chip: Chip, cols: usize) -> usize {
    use esp_hal::delay::Delay;
    use esp_hal::gpio::{Level, Output, OutputConfig};

    /// Clock high and low time. See the note above.
    const HOLD_NS: u32 = 100;

    let lv = |b: bool| if b { Level::High } else { Level::Low };
    let mut rgb = [
        Output::new(pins.red1.reborrow(), Level::Low, OutputConfig::default()),
        Output::new(pins.grn1.reborrow(), Level::Low, OutputConfig::default()),
        Output::new(pins.blu1.reborrow(), Level::Low, OutputConfig::default()),
        Output::new(pins.red2.reborrow(), Level::Low, OutputConfig::default()),
        Output::new(pins.grn2.reborrow(), Level::Low, OutputConfig::default()),
        Output::new(pins.blu2.reborrow(), Level::Low, OutputConfig::default()),
    ];
    let mut lat = Output::new(pins.latch.reborrow(), Level::Low, OutputConfig::default());
    // OE starts HIGH = display off, which is where every step but the last
    // one holds it.
    let mut oe = Output::new(pins.blank.reborrow(), Level::High, OutputConfig::default());
    let mut clk = Output::new(pins.clock.reborrow(), Level::Low, OutputConfig::default());
    let delay = Delay::new();

    let mut clocks = 0usize;
    luxel_hub75::chip::init_steps(chip, cols, &mut |s| {
        let d = lv(s.data);
        for p in rgb.iter_mut() {
            p.set_level(d);
        }
        lat.set_level(lv(s.latch));
        oe.set_level(lv(s.oe));
        clk.set_high();
        delay.delay_nanos(HOLD_NS);
        clk.set_low();
        delay.delay_nanos(HOLD_NS);
        clocks += 1;
    });
    clocks
}

/// The panel driver behind `output::BoardOutput` on `hub75` builds.
pub struct Hub75Output {
    /// `None` = init failed (framebuffer alloc or LCD_CAM setup) even at the
    /// board default; output stays disabled while the engine keeps running.
    hub75: Option<Hub75<Blocking, DmaFb>>,
    /// The framebuffer shape that booted — what the packer is called with.
    g: Geometry,
    /// The compose target while no swap is in flight (spare-plane mode: the
    /// view whose MSB block is idle).
    back: Option<&'static mut DmaFb>,
    /// The previous frame's swap; waited (instant by then) at the start
    /// of the next `write_frame` to reclaim the displaced buffer.
    pending: Option<Hub75Swap<DmaFb>>,
    /// Spare-plane mode (Gitea #610): the compose target. The live DMA
    /// framebuffer is written only by `flush`, inside the window.
    #[cfg(feature = "hub75-spare-plane")]
    staging: Option<&'static mut DynFb>,
    /// A composed frame is in `staging`, waiting for its window.
    #[cfg(feature = "hub75-spare-plane")]
    staged: bool,
    /// When that frame was composed — the liveness floor's clock.
    #[cfg(feature = "hub75-spare-plane")]
    staged_at: esp_hal::time::Instant,
    /// Slowest single-plane copy seen, microseconds; sizes the window check.
    #[cfg(feature = "hub75-spare-plane")]
    plane_us: u32,
    /// The ring geometry the window check is against.
    #[cfg(feature = "hub75-spare-plane")]
    window: spare::Window,
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
    /// `None` only on a dead driver: the packer is the ONLY compose path
    /// since #401 (there is no `set_pixel` on a `DynFb` to fall back to, and
    /// nothing third-party left for a per-pixel path to disagree with), so
    /// failing to find its 2 KiB is a boot failure like any other.
    tables: Option<&'static mut Tables>,
    /// Brightness the tables were built for; `u8::MAX` = never built.
    tables_b5: u8,
    /// The packer's per-row pads, allocated once at boot — what keeps
    /// composition allocation-free with `cols` no longer a const.
    scratch: Scratch,
    /// The boot-time panel→pixel remap (Gitea #475). `None` = the configured
    /// arrangement IS the driver's own row-major order, so nothing is
    /// gathered and the compose path is byte-for-byte what it always was.
    remap: Option<&'static [u16]>,
}

impl Hub75Output {
    /// Build the panel from the STORED layout, falling back once to the board
    /// default if the configured shape cannot be built.
    pub fn new(lcd_cam: LCD_CAM<'static>, pins: Hub75Pins16<'static>, channel: DMA_CH0<'static>) -> Self {
        let m = crate::layout::matrix();
        let d = crate::layout::driver();
        match Self::try_boot(lcd_cam, pins, channel, m, d, false) {
            Ok(me) => me,
            Err(why) => {
                let (dm, dd) = (board_default_matrix(), PanelDriver::default());
                if (m, d) == (dm, dd) {
                    println!("hub75: {why} at the board default — panel output disabled");
                    return Self::dead();
                }
                println!("hub75: {why} — falling back to the board default panel");
                // SAFETY: the attempt above consumed all three — a failed
                // `Hub75::new` drops the pins and the LCD_CAM/DMA handles it
                // was given — so these steals are the only live owners. The
                // pin NUMBERS come from the same board table `hub75_pins!`
                // names by type (`board::HUB75_PINS`, asserted reserved).
                let (lcd_cam, channel, pins) =
                    unsafe { (LCD_CAM::steal(), DMA_CH0::steal(), crate::board::hub75_pins_stolen()) };
                match Self::try_boot(lcd_cam, pins, channel, dm, dd, true) {
                    Ok(me) => me,
                    Err(why) => {
                        println!("hub75: {why} at the board default — panel output disabled");
                        Self::dead()
                    }
                }
            }
        }
    }

    /// Output disabled: the engine keeps rendering, nothing reaches a panel.
    fn dead() -> Self {
        Self {
            hub75: None,
            g: Geometry::new(0, 0, 0),
            back: None,
            pending: None,
            last_shown_rescan: 0,
            next_seq: 1,
            shown_cursor: 0,
            last_shown_seq: 0,
            tables: None,
            tables_b5: u8::MAX,
            scratch: Scratch::new(0),
            remap: None,
            #[cfg(feature = "hub75-spare-plane")]
            staging: None,
            #[cfg(feature = "hub75-spare-plane")]
            staged: false,
            #[cfg(feature = "hub75-spare-plane")]
            staged_at: esp_hal::time::Instant::EPOCH,
            #[cfg(feature = "hub75-spare-plane")]
            plane_us: 0,
            #[cfg(feature = "hub75-spare-plane")]
            window: spare::Window { descs: 0, msb_descs: 0, planes: 0 },
        }
    }

    /// One boot attempt at `(m, d)`.
    ///
    /// Nothing is running on `Err`, and — for every failure up to and
    /// including the last allocation — nothing stays allocated either: the
    /// framebuffers, the descriptor ring, the packer tables and the row pads
    /// are all owned [`Block`]s / `Vec`s until the driver actually starts, so
    /// the fallback attempt gets the heap it started with. The two exceptions
    /// are documented where they happen: spare-plane mode's staging buffer
    /// (the PSRAM arena cannot free) and everything the DMA may already point
    /// at once `Hub75::new` itself fails.
    fn try_boot(
        lcd_cam: LCD_CAM<'static>,
        mut pins: Hub75Pins16<'static>,
        channel: DMA_CH0<'static>,
        m: Matrix,
        d: PanelDriver,
        fallback: bool,
    ) -> Result<Self, &'static str> {
        let Some(g) = arrange::fb_geometry(&m, usize::from(d.planes)) else {
            return Err("the arrangement has no framebuffer (odd ph, or scan does not divide ph/2)");
        };
        if g.planes == 0 || g.planes > luxel_hub75::MAX_PLANES {
            return Err("planes out of range");
        }
        if g.rows > MAX_SCAN {
            return Err("scan deeper than the five HUB75 address lines can reach");
        }
        if g.pixels() as u32 > crate::board::MAX_PIXELS {
            return Err("the panel is larger than this board's pixel cap");
        }
        let c = control_of(&d);
        let stripes = arrange::stripes(&m);
        let (w, h) = (g.cols / stripes, 2 * g.rows * stripes);

        // ---- buffers. Nothing is leaked until every allocation landed. ----
        let per_ring = descs_per_ring(g);
        let (desc_block, descriptors) =
            alloc_descriptors(g).ok_or("DMA descriptor alloc failed")?;
        let (tables_block, tables) = alloc_tables().ok_or("packer table alloc failed")?;

        #[cfg(not(feature = "hub75-spare-plane"))]
        let (fb_blocks, front, back, live_bytes) = {
            let (fb0, front) = DynFb::alloc(g, c).ok_or("framebuffer alloc failed")?;
            if !template_lights(front.fb_words(), g, c) {
                return Err("this blank/latch template cannot light the panel");
            }
            let (fb1, back) = DynFb::alloc(g, c).ok_or("second framebuffer alloc failed")?;
            ([fb0, fb1], front, back, g.bytes())
        };
        // Spare-plane mode (#610): one framebuffer plus a spare MSB block,
        // both internal, and a staging framebuffer only the compose writes.
        #[cfg(feature = "hub75-spare-plane")]
        let (fb_blocks, front, back, mut staging, live_bytes) = {
            let (fb_block, fb) = DynFb::alloc(g, c).ok_or("framebuffer alloc failed")?;
            if !template_lights(fb.fb_words(), g, c) {
                return Err("this blank/latch template cannot light the panel");
            }
            let plane_bytes = g.plane_bytes();
            let spare_layout =
                AllocLayout::from_size_align(plane_bytes, 4).map_err(|_| "bad plane layout")?;
            let spare_block = Block::zeroed(spare_layout).ok_or("spare MSB plane alloc failed")?;
            // Control bits (row address, OE/LAT placement) are the same in
            // every plane, so plane 0 of a formatted buffer is the template.
            // SAFETY: both spans are `plane_bytes` long, disjoint, and live.
            unsafe {
                core::ptr::copy_nonoverlapping(fb.plane_ptr_len(0).0, spare_block.ptr, plane_bytes)
            };
            let spare_ptr = spare_block.ptr;
            let (va, vb) = spare::PlaneView::pair(fb, spare_ptr.cast_const(), g);
            // The compose target: a whole framebuffer, formatted, never a DMA
            // source. Only task context touches it, so the PSRAM arena is the
            // right home on a `psram-arena` board (psram.rs's fence
            // argument); elsewhere the heap. Leaked either way — the arena has
            // no `free`, so this is the ONE allocation a failed attempt cannot
            // give back (and it is the last one, so nothing after it can fail).
            #[cfg(feature = "psram-arena")]
            let (words, place) = (
                DynFb::alloc_words_leaked(g, c, |l| {
                    let p = crate::psram::alloc_bulk_zeroed(l);
                    (!p.is_null()).then_some(p)
                }),
                "arena",
            );
            #[cfg(not(feature = "psram-arena"))]
            let (words, place) =
                (DynFb::alloc_words_leaked(g, c, |l| Block::zeroed(l).map(Block::leak)), "heap");
            let words = words.ok_or("staging framebuffer alloc failed")?;
            let staging = alloc::boxed::Box::leak(alloc::boxed::Box::new(DynFb { g, words }));
            println!(
                "hub75: spare-plane swap — framebuffer {} B + spare MSB plane {} B internal, \
                 staging {} B in the {}",
                g.bytes(),
                plane_bytes,
                g.bytes(),
                place,
            );
            (
                [fb_block, spare_block],
                alloc::boxed::Box::leak(alloc::boxed::Box::new(va)),
                alloc::boxed::Box::leak(alloc::boxed::Box::new(vb)),
                Some(staging),
                g.bytes(),
            )
        };

        // The packer's per-row pads. `Scratch` is a `Vec` inside, and the
        // global allocator ABORTS rather than returning on OOM, so check the
        // heap can take them — 14 B per column — now that the framebuffers
        // have had their share. Owned, not leaked: an `Err` below frees it
        // again for the fallback attempt.
        if esp_alloc::HEAP.free() < g.cols * 14 + 4096 {
            return Err("no room for the packer's row pads");
        }
        let scratch = Scratch::for_geometry(g);

        // The configured arrangement (#475). `layout::init()` has already run
        // (main.rs wires the panel after it), so `m` is the stored Layout's.
        let remap = build_remap(&m, g);
        println!(
            "hub75: {}x{} tiles of {}x{} from {} {}{}{}, framebuffer {}x{} scan 1/{}, \
             remap {}, est {} Hz",
            m.cols,
            m.rows,
            m.pw,
            m.ph,
            m.start.as_str(),
            m.dir.as_str(),
            if m.snake { " snake" } else { "" },
            if m.rot180 { " rot180" } else { "" },
            w,
            h,
            g.rows,
            if remap.is_some() { "on" } else { "off (row-major)" },
            arrange::est_hz(&m, u32::from(d.planes), d.clock_hz()),
        );

        // ---- the chip's register init, on the real pins, before the DMA ----
        if d.chip.needs_init() {
            let n = chip_init(&mut pins, d.chip, g.cols);
            println!("hub75: {} init sequence sent ({} clocks)", d.chip.as_str(), n);
        }

        match Hub75::new(lcd_cam, pins, channel, descriptors, clock_rate(&d), &*front) {
            Ok(hub75) => {
                // Descriptor cost is worth printing once: the frame-atomic
                // swap (#376) doubles it, and it is internal DMA-capable RAM.
                println!(
                    "hub75: {}x{} panel, scan 1/{}, {} bitplanes, LCD_CAM @ {} MHz, chip {}, \
                     blank {}, circular DMA, {} descriptors x {} rings = {} B, framebuffer {} B{}",
                    w,
                    h,
                    g.rows,
                    g.planes,
                    d.clock_mhz,
                    d.chip.as_str(),
                    d.blank,
                    per_ring,
                    esp_hub75::DESCRIPTOR_RINGS,
                    per_ring * esp_hub75::DESCRIPTOR_RINGS * core::mem::size_of::<DmaDescriptor>(),
                    live_bytes,
                    if fallback { " — FALLBACK, the configured panel would not build" } else { "" },
                );
                let live = LiveDriver {
                    planes: d.planes,
                    clock_mhz: d.clock_mhz,
                    chip: d.chip,
                    blank: d.blank,
                    w: w as u16,
                    h: h as u16,
                    scan: g.rows as u16,
                    fb_bytes: live_bytes as u32,
                    fallback,
                };
                LIVE.lock(|c| c.set(Some(live)));
                LIVE_SCAN.store(g.rows as u16, Ordering::Relaxed);
                // Past the point of no return: the driver owns the
                // descriptors and the DMA is running out of `front`.
                let _ = desc_block.leak();
                let _ = tables_block.leak();
                for b in fb_blocks {
                    let _ = b.leak();
                }
                Ok(Self {
                    hub75: Some(hub75),
                    g,
                    back: Some(back),
                    pending: None,
                    last_shown_rescan: 0,
                    next_seq: 1,
                    shown_cursor: 0,
                    last_shown_seq: 0,
                    tables: Some(tables),
                    tables_b5: u8::MAX,
                    scratch,
                    remap,
                    #[cfg(feature = "hub75-spare-plane")]
                    staging: staging.take(),
                    #[cfg(feature = "hub75-spare-plane")]
                    staged: false,
                    #[cfg(feature = "hub75-spare-plane")]
                    staged_at: esp_hal::time::Instant::EPOCH,
                    #[cfg(feature = "hub75-spare-plane")]
                    plane_us: spare::plane_us_guess(g),
                    #[cfg(feature = "hub75-spare-plane")]
                    window: spare::Window::new(g, per_ring),
                })
            }
            Err(e) => {
                println!("hub75: LCD_CAM init failed at {} MHz: {:?}", d.clock_mhz, e);
                // The DESCRIPTOR ring and the FRAMEBUFFERS stay leaked on this
                // one path: a half-started GDMA may still hold pointers into
                // both (`Hub75::new` is past `CircularBcmBuf::new` by the time
                // the transfer can fail), and handing that memory back to the
                // allocator would be a use-after-free the moment the
                // peripheral twitched. So does the remap, which is leaked at
                // its own allocation site. The packer tables and the row pads
                // go back. Every allocation FAILURE above — the likely reason
                // to fall back at all — gives everything back.
                let _ = desc_block.leak();
                for b in fb_blocks {
                    let _ = b.leak();
                }
                Err("LCD_CAM init failed")
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
        // Panel geometry is fixed for the life of a boot (changing it is
        // `reboot_required`), and the buffers were allocated from it: any
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
            compose_into(
                &mut self.tables,
                &mut self.tables_b5,
                &mut self.scratch,
                self.g,
                self.remap,
                staging,
                rgb,
                brightness5,
            );
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
            compose_into(
                &mut self.tables,
                &mut self.tables_b5,
                &mut self.scratch,
                self.g,
                self.remap,
                back,
                rgb,
                brightness5,
            );
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
        if !self.window.fits(idx, eof_pending, nominal_us, self.plane_us) {
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
        let planes = back.plane_count();
        let dst = back.planes();
        let msb_descs = self.window.msb_descs;
        // Arm the flip FIRST. It lands at the wrap — the patch's single tail
        // store, taken while the DMA is provably short of the tail — so the
        // next pass reads the other view whatever happens below. Arming after
        // the copy could only make things worse: the shared planes would
        // already be new against the old MSB.
        note_handoff(&mut self.last_shown_rescan, &mut self.next_seq, hub75);
        let swap = hub75.swap(back);
        let t0 = esp_hal::time::Instant::now();
        let mut worst: u32 = 0;
        for (p, &(d, len)) in dst.iter().enumerate().take(planes).skip(1) {
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
                let ok = hub75.dma_position().is_some_and(|(r, i, _)| r == ring && i < msb_descs);
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
/// software exactly like the WS2812 path, folded into the packer's tables so
/// it costs nothing in the inner loop.
///
/// The bulk packer (Gitea #329) walks the frame per ROW PAIR, building all
/// `g.planes` entry words for a pixel pair from one pair of table lookups per
/// channel. It writes every colour bit of every entry, so it subsumes the
/// erase, and it preserves every control bit `format` wrote. Since #401 it is
/// the ONLY compose path: a `DynFb` has no `set_pixel`, and the per-pixel
/// fallback that used to exist was there to cover a third-party framebuffer
/// whose layout the boot probe did not recognise — there is no longer a
/// third-party framebuffer to disagree with.
#[allow(clippy::too_many_arguments)]
fn compose_into(
    tables: &mut Option<&'static mut Tables>,
    tables_b5: &mut u8,
    scratch: &mut Scratch,
    g: Geometry,
    remap: Option<&'static [u16]>,
    target: &mut DynFb,
    rgb: &[[u8; 3]],
    brightness5: u8,
) {
    let Some(t) = tables.as_deref_mut() else { return };
    if *tables_b5 != brightness5 {
        t.build(&brightness_lut(brightness5));
        *tables_b5 = brightness5;
    }
    let dst = target.fb_words();
    match remap {
        None => luxel_hub75::pack(dst, g, rgb, t, scratch),
        Some(lut) => luxel_hub75::pack_remap(dst, g, rgb, lut, t, scratch),
    }
}
