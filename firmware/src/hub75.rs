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
//! for how it is DRIVEN (`planes`, `clock_mhz`, `chip`, `blank`, and since
//! Gitea #460/#789 `lsb`, the brighter ↔ faster trade — see
//! [`luxel_hub75::Schedule`]) — so one image drives any panel the RAM fits.
//! Consequences:
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
//! CHASES THE BEAM (Gitea #829): at the top of a pass it arms the ring flip
//! to the other view FIRST (the atomic-swap patch's single tail store, which
//! lands at the wrap), writes the new MSB into the idle spare block at once
//! (no ring the DMA can reach before the wrap names it), and then writes
//! each shared plane `1..` the moment the DMA has finished reading it for
//! this pass — from then on nothing reads it until the next pass, which
//! reads the other view. The three lowest planes are written without
//! waiting (weights 1/127..4/127; a torn pass of them is invisible), which
//! removes the only tight deadline. A poll spins for at most one pass and
//! resumes on the next poll otherwise. So a frame goes out within one pass
//! at every schedule with no window and no deferral; the one way to tear is
//! the wrap arriving before the last copies did — the output task held off
//! the core for longer than the rest of the pass — counted as
//! `spare.torn_wrap` (one pass with a stale low plane). An earlier design
//! waited for a window inside the MSB run and froze the panel when none
//! opened (#620); that is why nothing here waits for room.

use core::alloc::Layout as AllocLayout;
use core::cell::Cell;
use core::sync::atomic::{AtomicU16, AtomicU32, AtomicU8, Ordering};

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
use luxel_hub75::{arrange, Control, Geometry, Schedule, Scratch, Tables};

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

/// Heap that must still be free once the panel's buffers are allocated, or
/// the boot falls back to the board default (Gitea #768).
///
/// Measured basis, Seengreat, from the `hub75: panel took … left …` boot line
/// (2026-09-27, Gitea #822): the heap this runs against is ~216 KB (the
/// 152 KB region plus the 64 KB reclaimed one). A 64x64 pair takes 61,760 B
/// and leaves 154,652; the board then reads 79,892 B free once WiFi is up and
/// idles at ~60 KB with a pattern — so WiFi + embassy-net + the web slots
/// cost ~75 KB, and a two-layer scene at 4096 px asks the resume for another
/// ~65 KB. A 2x1 chain double-buffered takes 120,944 B and leaves 95,452 —
/// which PASSED the old 64 KB floor and then starved the board at 19.8 KB
/// free, every route 503, until the self-heal reverted it. The same chain
/// under `hub75-spare-plane` (~59 + 8 KB internal) leaves ~146 KB. 100 KB
/// sits between the two: WiFi's 75 KB plus `RUNTIME_FLOOR` (20 KB) with a
/// little slack, and no engine at all — the self-heal
/// (`crate::layout::heal_if_starved`) is what judges the engine's share.
const BOOT_HEAP_FLOOR: usize = 100 * 1024;

/// Free internal heap at the very top of the FIRST [`Hub75Output::try_boot`]
/// attempt — before a single panel byte is allocated (Gitea #822).
///
/// This is the one number `POST /api/layout` needs to answer "would the panel
/// this body asks for leave room for the rest of a boot?", and it can only be
/// read at boot: by the time an HTTP handler runs, WiFi, embassy-net, the web
/// slots and the engine have all taken their share. Written once (the fallback
/// attempt does NOT overwrite it — it runs with the first attempt's
/// allocations already given back, so the two agree, and "written once" is
/// easier to reason about than "written twice with the same value"). 0 = never
/// ran, which is the pre-boot / `LUXEL_NO_OTA` case and means "no prediction".
static BOOT_HEAP_BEFORE: AtomicU32 = AtomicU32::new(0);

/// [`BOOT_HEAP_BEFORE`] — free internal heap before the panel was built.
/// 0 = unknown (the panel has not booted), in which case nothing may be
/// predicted from it.
pub fn heap_before_panel() -> usize {
    BOOT_HEAP_BEFORE.load(Ordering::Relaxed) as usize
}

/// [`BOOT_HEAP_FLOOR`] — the heap a boot must still have once the panel's
/// buffers are allocated.
pub fn boot_heap_floor() -> usize {
    BOOT_HEAP_FLOOR
}

/// Internal-SRAM bytes booting the arrangement `m` under driver `d` would
/// take for the PANEL — the prediction `POST /api/layout` refuses a layout on
/// (Gitea #822).
///
/// The arithmetic is [`luxel_hub75::boot_cost`], shared with the host tests;
/// everything board-specific comes from here. `None` when the arrangement has
/// no framebuffer at all (odd `ph`, or a `scan` that does not divide `ph/2`)
/// — the core parser has its own error for that and this must not shadow it.
///
/// **Panel side only.** What the engine and the compositor then cost at the
/// same pixel count is not modelled anywhere, so a layout that passes this
/// check can still starve the board; the boot self-heal
/// (`crate::layout::heal_if_starved`) is the guard that catches those.
pub fn boot_cost(m: &Matrix, d: &PanelDriver) -> Option<usize> {
    let g = arrange::fb_geometry(m, usize::from(d.planes))?;
    let s = schedule_of(g, d);
    // The spare-plane swap (#610) allocates ONE live framebuffer plus one
    // spare MSB plane, and composes into a staging framebuffer that lives in
    // the PSRAM arena where there is one — internal heap where there is not.
    #[cfg(not(feature = "hub75-spare-plane"))]
    let (buffers, spare_planes) = (2, 0);
    #[cfg(all(feature = "hub75-spare-plane", feature = "psram-arena"))]
    let (buffers, spare_planes) = (1, 1);
    #[cfg(all(feature = "hub75-spare-plane", not(feature = "psram-arena")))]
    let (buffers, spare_planes) = (2, 1);
    Some(
        luxel_hub75::boot_cost(
            g.with_trail(s.needs_trail()),
            &s,
            &luxel_hub75::BootAlloc {
                buffers,
                spare_planes,
                rings: esp_hub75::DESCRIPTOR_RINGS,
                desc_bytes: core::mem::size_of::<DmaDescriptor>(),
                max_chunk: esp_hub75::max_dma_chunk_size(),
            },
        )
        .total(),
    )
}

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

/// The live framebuffer's row block in bus words — [`Geometry::cols`] of the
/// running DMA, i.e. `pw · panels · stripes`. 0 = no panel output.
///
/// Kept beside [`LIVE_SCAN`] rather than derived from [`LIVE`]'s `w`/`h`/`scan`
/// because it is what bounds the latch-blanking window, and re-deriving the
/// stripe count in a second place is exactly how the two would drift.
static LIVE_COLS: AtomicU16 = AtomicU16::new(0);

/// The latch blanking the stored Layout wants — the ONE panel field that
/// applies without a reboot (Gitea #778).
///
/// Written by `POST /api/layout` (`crate::layout::set_from_wire`) and read by
/// [`Hub75Output::write_frame`]. It is not a boot parameter: `blank` is
/// nothing but control bits in the framebuffer words — the OE window and the
/// latch tail [`luxel_hub75::format`] writes — and the packer rewrites only
/// colour bits, so the output task can re-`format` a buffer in place between
/// frames. The panel it is tuned against (ghosting between address rows) is
/// the reason: a reboot per attempt makes that knob unusable.
///
/// [`BLANK_NONE`] until the driver boots, so a strip board and a pre-boot
/// POST cost nothing.
static WANT_BLANK: AtomicU8 = AtomicU8::new(BLANK_NONE);

/// [`WANT_BLANK`]: nothing has asked for a blanking yet. Outside the 0..=8
/// the parser accepts, so it can never be mistaken for one.
const BLANK_NONE: u8 = u8::MAX;

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
///
/// **Which is why the setting is a FIXED LIST, not a range** — Gitea #771.
/// Jeremy set 40 MHz "to see what happens" on 2026-09-26 and got exactly the
/// row above; a UI warning is not a guard. The offered values are
/// [`PanelDriver::CLOCKS`] = 8, 10, 12, 15, 20, 24, 30, which is every rate
/// esp-hal can reach with an **integer** LCD_CAM divider (`source / (2·N)`
/// from XTAL 40 MHz or PLL_D2 240 MHz — the i8080 driver doubles the request
/// for the S3's PCLK-divider errata) capped at the FM6124 datasheet's 30 and
/// floored at 8. Everything in between — 16 and 25 MHz among them — is
/// synthesised by esp-hal's fractional divider, which dithers the period
/// rather than dividing evenly; and 13/17/39 MHz are worse than that, because
/// `calculate_clkm` scores its candidate sources with a numerator/denominator
/// swap and picks the XTAL "too fast" fallback, silently clocking at 10 MHz.
/// The 40 MHz row above stays here as the reason 40 is not on the list.
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

/// The BCM emission schedule the stored driver implies at geometry `g` —
/// the brighter ↔ faster trade (Gitea #460 / #789): its `lsb` clamped to the
/// row block's lit width, and from that how many low planes are emitted once
/// with OE cut early instead of `2^k` times. `lsb 0` is the stock schedule.
fn schedule_of(g: Geometry, d: &PanelDriver) -> Schedule {
    Schedule::plan(g, control_of(d), d.lsb)
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
    /// Which control template these words carry — [`Hub75Output::fmt_gen`] at
    /// the last [`luxel_hub75::format`] of this buffer (Gitea #778). A buffer
    /// whose generation is behind the driver's is re-formatted before the
    /// packer writes into it, which is how a latch-blanking change reaches
    /// both swap buffers without a reboot.
    fmt_gen: u32,
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
    fn alloc(g: Geometry, c: Control, s: &Schedule) -> Option<(Block, &'static mut DynFb)> {
        let block = Block::zeroed(Self::buffer_layout(g)?)?;
        let words = Self::format_words(block.ptr, g, c, s);
        Some((
            block,
            alloc::boxed::Box::leak(alloc::boxed::Box::new(DynFb { g, words, fmt_gen: 0 })),
        ))
    }

    /// Allocate a framebuffer through someone else's allocator and leak it —
    /// spare-plane mode's staging buffer, which comes from the PSRAM arena on
    /// a `psram-arena` board (it is never a DMA source, and the arena has no
    /// `free`).
    #[cfg(feature = "hub75-spare-plane")]
    fn alloc_words_leaked(
        g: Geometry,
        c: Control,
        s: &Schedule,
        alloc_zeroed: impl FnOnce(AllocLayout) -> Option<*mut u8>,
    ) -> Option<*mut u16> {
        let p = alloc_zeroed(Self::buffer_layout(g)?)?;
        Some(Self::format_words(p, g, c, s))
    }

    /// The layout one framebuffer's buffer needs. Align 4: the LCD_CAM's GDMA
    /// wants word-aligned sources, and `u16` alignment alone would let an
    /// odd-sized geometry land on a halfword. 16 under `hub75-pie`: the vector
    /// packer's loads and stores FORCE 16-byte alignment rather than fault
    /// (`luxel_hub75::pie`), and it refuses a buffer that is not.
    fn buffer_layout(g: Geometry) -> Option<AllocLayout> {
        let align = if cfg!(feature = "hub75-pie") { 16 } else { 4 };
        AllocLayout::from_size_align(g.bytes(), align).ok()
    }

    /// Write the control template into a freshly zeroed buffer — per-plane OE
    /// windows per the schedule (Gitea #460).
    fn format_words(p: *mut u8, g: Geometry, c: Control, s: &Schedule) -> *mut u16 {
        let words = p.cast::<u16>();
        // SAFETY: the caller allocated `g.bytes()` zeroed, 4-byte-aligned
        // bytes at `p` and holds the only reference to them.
        let flat = unsafe { core::slice::from_raw_parts_mut(words, g.words()) };
        luxel_hub75::format_scheduled(flat, g, c, s);
        words
    }

    /// The framebuffer as the flat entry array the packer writes.
    fn fb_words(&mut self) -> &mut [u16] {
        // SAFETY: the allocation is `g.words()` contiguous `u16`s and `self`
        // is its only owner.
        unsafe { core::slice::from_raw_parts_mut(self.words, self.g.words()) }
    }

    /// Bring this buffer's control template up to `gen` (Gitea #778).
    ///
    /// Re-`format` CLEARS every colour bit and rewrites every control bit; the
    /// packer then writes every colour bit of every entry and touches no
    /// control bit (`compose_into`'s contract, asserted in `luxel-hub75`). So
    /// format-then-pack, in that order, leaves the frame exact — which is why
    /// this runs here rather than after composing.
    fn refmt(&mut self, c: Control, s: &Schedule, gen: u32) {
        if self.fmt_gen == gen {
            return;
        }
        let g = self.g;
        luxel_hub75::format_scheduled(self.fb_words(), g, c, s);
        self.fmt_gen = gen;
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
    // Plane 0's blocks: with the trailing display block (Geometry::trail)
    // block 0 is dark by design and the trailing block latches nothing, so
    // judge the plane as a whole, not its first block.
    let block = &words[..g.plane_words()];
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

/// Heap-allocate the packer's spread tables (2.25 KiB with the byte LUT the
/// PIE packer reads, Gitea #855). `Tables::zeroed()` is
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

/// One ring's worth of DMA descriptors for `g` under schedule `s` — the same
/// chunking esp-hub75 uses, with the schedule's per-plane repeats in place of
/// the stock `2^k` (Gitea #460). Equal to `esp_hub75::dma_descriptor_count`
/// at `lsb 0`, and to what the patched `CircularBcmBuf::new` builds once
/// `set_plane_repeats` has been given the same schedule.
fn descs_per_ring(g: Geometry, s: &Schedule) -> usize {
    s.descriptors(g.plane_bytes(), esp_hub75::max_dma_chunk_size())
}

/// Heap-allocate the descriptor rings. `hub75_dma_descriptors!` is a
/// compile-time static sized for a const framebuffer type, so a runtime
/// geometry has to bring its own — [`esp_hub75::DESCRIPTOR_RINGS`] of them,
/// because the frame-atomic swap (#376) needs one ring per framebuffer and
/// `CircularBcmBuf::ring_count` derives that from the slice length.
/// Descriptors must be in internal RAM (the peripheral walks them) and
/// 4-byte aligned, both of which `Layout::array` over the type gives.
fn alloc_descriptors(g: Geometry, s: &Schedule) -> Option<(Block, &'static mut [DmaDescriptor])> {
    let n = esp_hub75::DESCRIPTOR_RINGS * descs_per_ring(g, s);
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

    /// A staged frame the DMA gave no position for within this long is
    /// abandoned rather than freezing the engine behind it — the same
    /// liveness floor as the output task's vsync hold (a dead DMA must not
    /// stop the world).
    pub const HOLD_US: u64 = 50_000;
    /// How long one `flush` poll spins waiting for the DMA to finish reading
    /// the next plane before handing the core back and resuming on the next
    /// poll. Long enough to ride out a plane read at the fast schedules (a
    /// 4 KB plane is ~100 µs at 20 MHz), short enough that the output task
    /// never disappears for a whole 13 ms pass.
    pub const SPIN_US: u64 = 400;
    /// The lowest planes are copied WITHOUT waiting for the DMA to pass them
    /// (Gitea #829): their weights are 1/127, 2/127 and 4/127, a torn pass
    /// of them is invisible, and they are read last, right before the wrap,
    /// where waiting would be the one tight deadline this design has left.
    /// (Two was 7 % `torn_wrap` at an 861 Hz pass on the bench.)
    pub const LOW_FREE: usize = 3;

    /// The ring geometry the chase needs, computed once at boot (#829).
    #[derive(Clone, Copy)]
    pub struct Ring {
        /// Per plane (0 = MSB), the descriptor index just past the plane's
        /// last descriptor: once the DMA is at or beyond it, that plane has
        /// been read for this pass and can be rewritten without tearing.
        pub ends: [usize; MAX_PLANES],
        pub planes: usize,
    }

    impl Ring {
        pub fn new(g: Geometry, s: &luxel_hub75::Schedule) -> Self {
            Self {
                ends: s.plane_end_descs(g.plane_bytes(), esp_hub75::max_dma_chunk_size()),
                planes: g.planes,
            }
        }

        pub const fn empty() -> Self {
            Self { ends: [0; MAX_PLANES], planes: 0 }
        }

        /// Has the DMA, at descriptor `idx` of this pass, finished reading
        /// plane `p`?
        pub fn consumed(&self, p: usize, idx: usize) -> bool {
            idx >= self.ends[p]
        }

        /// Is descriptor `idx` still inside the MSB run — the first and
        /// longest stretch of the pass, where a chase has the most room?
        pub fn in_msb_run(&self, idx: usize) -> bool {
            idx < self.ends[0]
        }

        /// Planes copied without waiting: the last [`LOW_FREE`], but never
        /// plane 1 (the first read after the MSB run is the tear you would
        /// see).
        pub fn free_from(&self) -> usize {
            self.planes.saturating_sub(LOW_FREE).max(2)
        }
    }

    /// A flush in progress across polls: the flip is armed on `ring`, planes
    /// `1..next` are already copied, `next..` are waiting for the beam.
    /// `dst` is the plane table of the view the flip was armed with — planes
    /// `1..` are the memory both views share.
    #[derive(Clone, Copy)]
    pub struct Chase {
        pub ring: usize,
        pub next: usize,
        pub t0: esp_hal::time::Instant,
        pub worst_us: u32,
        pub dst: [(*mut u8, usize); MAX_PLANES],
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
///
/// The table is 2 B/px and the DMA never reads it — only the compose, from
/// task context — so it comes from the array arena
/// (`luxel_core::arena::ArrVec`), which on a `psram-arena` board is the 8 MB
/// external region and everywhere else is exactly the main heap it always was
/// (Gitea #768). That matters at the new cap: a 2x2 chain is non-identity by
/// construction, so a 128x128 wall pays 32 KB here — a third of the internal
/// DRAM heap on the S3 panel board, and free in the arena. Unlike the
/// spare-plane staging buffer this goes through the allocator rather than
/// `psram::alloc_bulk_zeroed`, because an identity table has to be handed
/// BACK, and `ArrVec`'s `Drop` does that through the same hook.
fn build_remap(m: &Matrix, g: Geometry) -> Option<&'static [u16]> {
    let stripes = arrange::stripes(m);
    let (fb_w, fb_h) = (g.cols / stripes, 2 * g.rows * stripes);
    // Fallible, then infallibly filled: a reserve that fails is a `None`, and
    // `try_boot` then formats the frame exactly as it did before remaps
    // existed. `build_lut` overwrites every entry; the zeros are only so the
    // slice is initialised.
    let mut lut: luxel_core::arena::ArrVec<u16> = luxel_core::arena::empty();
    lut.try_reserve_exact(g.pixels()).ok()?;
    lut.resize(g.pixels(), 0);
    arrange::build_lut(&mut lut, m, fb_w, fb_h);
    if arrange::is_identity(&lut) {
        return None; // dropped here — back to whichever heap it came from
    }
    // Leaked, like the framebuffers: the compose reads it for the life of the
    // driver. The `Box` is the 24-byte handle, not the table.
    Some(alloc::boxed::Box::leak(alloc::boxed::Box::new(lut)).as_mut_slice())
}

/// What the running driver was built with — the `live` block of
/// `GET /api/layout`'s `driver`. `None` = no panel output.
pub fn live_driver() -> Option<LiveDriver> {
    LIVE.lock(Cell::get)
}

/// Ask the output task to apply a latch blanking on its next frame
/// (Gitea #778) — the `panel` line's one live field. Idempotent: the task
/// compares it with the control template it is running and does nothing when
/// they agree, so `POST /api/layout` may call this on every body.
///
/// A no-op before the driver has booted (nothing to re-format yet): the boot
/// builds its template from the stored Layout anyway.
pub fn want_blank(blank: u8) {
    if WANT_BLANK.load(Ordering::Relaxed) != BLANK_NONE {
        WANT_BLANK.store(blank, Ordering::Relaxed);
    }
}

/// Would `blank` leave the LIVE framebuffer's row block with no OE-active
/// clock at all — i.e. a legal `panel` line that cannot light the panel?
///
/// [`luxel_hub75::format`] puts OE on over `blank .. cols - latch - blank`,
/// so the window is empty exactly when `2·blank + latch >= cols`. That is the
/// same question [`template_lights`] asks at boot; this is the POST-time form,
/// against the geometry that is RUNNING, so a live blanking change is refused
/// with numbers instead of blacking the panel out.
///
/// `Some((latch_clocks, cols))` = it would go dark, and those are the numbers
/// to name. `None` = it fits, or there is no panel output to fit it into.
pub fn blank_would_darken(blank: u8) -> Option<(u8, u16)> {
    let live = live_driver()?;
    let cols = LIVE_COLS.load(Ordering::Relaxed);
    let latch = live.chip.latch_clocks();
    (2 * u32::from(blank) + u32::from(latch) >= u32::from(cols)).then_some((latch, cols))
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
        est_hz: arrange::est_hz_driver(m, &d),
        drive: arrange::driven_panels(m, fb_w, fb_h) as u32,
        driver_live: live,
    }
}

/// The channel LUT the packer folds into its tables: what the panel is
/// actually driven with for each source value. Exactly the arithmetic the
/// per-pixel path did inline, so the two are byte-identical by construction
/// (asserted in `luxel-hub75`'s tests against `hub75-framebuffer` itself).
pub(crate) fn brightness_lut(brightness5: u8) -> [u8; 256] {
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
    /// The control template the buffers carry: the OE window and the latch
    /// tail. Its `latch_clocks` is the booted chip's and never moves; its
    /// `blank` follows [`WANT_BLANK`] (Gitea #778).
    control: Control,
    /// The BCM emission schedule the descriptor rings were built with and the
    /// buffers' per-plane OE windows follow (Gitea #460 / #789). Its `trunc`
    /// is fixed for the life of the driver — the rings encode it — and its
    /// `lsb` is re-clamped whenever `control.blank` moves
    /// ([`Schedule::refit`]).
    sched: Schedule,
    /// Bumped whenever `control` changes. A buffer whose [`DynFb::fmt_gen`] is
    /// behind this is re-formatted before the packer writes into it, so both
    /// swap buffers catch up on their own next turn.
    fmt_gen: u32,
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
    /// Typical single-plane copy, microseconds (an EWMA) — for the record;
    /// nothing is sized from it since the chase (#829).
    #[cfg(feature = "hub75-spare-plane")]
    plane_us: u32,
    /// The ring geometry the chase follows.
    #[cfg(feature = "hub75-spare-plane")]
    ring: spare::Ring,
    /// A flush that has armed its flip and is still copying planes behind
    /// the beam, resumed by the next poll.
    #[cfg(feature = "hub75-spare-plane")]
    chase: Option<spare::Chase>,
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
    /// The vector packer's pair pads (Gitea #855), same lifetime and reason.
    #[cfg(feature = "hub75-pie")]
    pads: luxel_hub75::pie::PairPads,
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
            control: Control { blank: 0, latch_clocks: 0 },
            sched: Schedule::plan(Geometry::new(0, 0, 0), Control { blank: 0, latch_clocks: 0 }, 0),
            fmt_gen: 0,
            back: None,
            pending: None,
            last_shown_rescan: 0,
            next_seq: 1,
            shown_cursor: 0,
            last_shown_seq: 0,
            tables: None,
            tables_b5: u8::MAX,
            scratch: Scratch::new(0),
            #[cfg(feature = "hub75-pie")]
            pads: luxel_hub75::pie::PairPads::new(0),
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
            ring: spare::Ring::empty(),
            #[cfg(feature = "hub75-spare-plane")]
            chase: None,
        }
    }

    /// Print the `lsb` schedule once, at boot — the numbers the bench
    /// compares `rescan_hz` against (Gitea #460 / #789). The brightness is
    /// relative to the stock schedule, on-time per unit time.
    fn print_schedule(s: &Schedule, g: Geometry, d: &PanelDriver) {
        println!(
            "hub75: lsb {} of {} lit clocks, {} low plane{} truncated, \
             {} row shifts/pass (stock {}), est {} Hz at {}.{}% of stock brightness",
            s.lsb,
            s.width,
            s.trunc,
            if s.trunc == 1 { "" } else { "s" },
            s.emissions(),
            s.full_emissions(),
            s.est_hz(g, d.clock_hz()),
            s.brightness_permille_at(g) / 10,
            s.brightness_permille_at(g) % 10,
        );
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
        // Before anything is allocated (Gitea #822): the heap a boot starts
        // the panel with is what `POST /api/layout` predicts against later.
        // load-then-store, not a CAS — rv32imc has no atomic RMW and this runs
        // once, on the main task, before any other core is awake.
        if BOOT_HEAP_BEFORE.load(Ordering::Relaxed) == 0 {
            BOOT_HEAP_BEFORE.store(esp_alloc::HEAP.free() as u32, Ordering::Relaxed);
        }
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
        let s = schedule_of(g, &d);
        // Truncated planes need the trailing display block, or the last
        // address row's bit weights come out rotated (Gitea #795 — the bottom
        // row of each half wrong on the bench). One block more per plane.
        let g = g.with_trail(s.needs_trail());
        let stripes = arrange::stripes(&m);
        let (w, h) = (g.cols / stripes, 2 * g.rows * stripes);
        // The descriptor chain follows the schedule (Gitea #460): install the
        // per-plane repeats before anything sizes or builds a ring. Every boot
        // attempt sets it, so a fallback to the board default (stock
        // schedule) is not left running the configured one's counts.
        esp_hub75::set_plane_repeats(&s.reps_u8());

        // ---- buffers. Nothing is leaked until every allocation landed. ----
        let per_ring = descs_per_ring(g, &s);
        debug_assert_eq!(
            per_ring,
            esp_hub75::dma_descriptor_count_scheduled(g.planes, g.plane_bytes()),
            "descriptor arithmetic drifted from the patched driver's"
        );
        let (desc_block, descriptors) =
            alloc_descriptors(g, &s).ok_or("DMA descriptor alloc failed")?;
        let (tables_block, tables) = alloc_tables().ok_or("packer table alloc failed")?;

        #[cfg(not(feature = "hub75-spare-plane"))]
        let (fb_blocks, front, back, live_bytes) = {
            let (fb0, front) = DynFb::alloc(g, c, &s).ok_or("framebuffer alloc failed")?;
            if !template_lights(front.fb_words(), g, c) {
                return Err("this blank/latch template cannot light the panel");
            }
            let (fb1, back) = DynFb::alloc(g, c, &s).ok_or("second framebuffer alloc failed")?;
            ([fb0, fb1], front, back, g.bytes())
        };
        // Spare-plane mode (#610): one framebuffer plus a spare MSB block,
        // both internal, and a staging framebuffer only the compose writes.
        #[cfg(feature = "hub75-spare-plane")]
        let (fb_blocks, front, back, mut staging, live_bytes) = {
            let (fb_block, fb) = DynFb::alloc(g, c, &s).ok_or("framebuffer alloc failed")?;
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
                DynFb::alloc_words_leaked(g, c, &s, |l| {
                    let p = crate::psram::alloc_bulk_zeroed(l);
                    (!p.is_null()).then_some(p)
                }),
                "arena",
            );
            #[cfg(not(feature = "psram-arena"))]
            let (words, place) =
                (DynFb::alloc_words_leaked(g, c, &s, |l| Block::zeroed(l).map(Block::leak)), "heap");
            let words = words.ok_or("staging framebuffer alloc failed")?;
            let staging =
                alloc::boxed::Box::leak(alloc::boxed::Box::new(DynFb { g, words, fmt_gen: 0 }));
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
        // The panel is allocated BEFORE the WiFi blob, embassy-net, the web
        // slots and the engine take their share of the same heap — which is
        // what makes the framebuffers a certainty, and also what lets a chain
        // that is too wide SUCCEED here and starve everything after it. On
        // 2026-09-26 a 2x1 chain (two 57 KB framebuffers) booted, and the
        // board then answered `/api/status` with 503 and hung `/api/brightness`
        // for 30 s: nothing had failed, so nothing fell back (Gitea #768). Ask
        // the question the allocator cannot: is there still room for the rest
        // of the boot?
        let left = esp_alloc::HEAP.free();
        let took = (BOOT_HEAP_BEFORE.load(Ordering::Relaxed) as usize).saturating_sub(left);
        if left < BOOT_HEAP_FLOOR {
            println!(
                "hub75: this panel leaves {} B of heap for WiFi, the web server and the engine \
                 (floor {} B)",
                left, BOOT_HEAP_FLOOR
            );
            return Err("the framebuffers leave too little heap for the rest of the boot");
        }
        // On the record every boot, not only on refusal: this is the number
        // the floor is calibrated against, and the 2026-09-27 2x1 boot passed
        // it and starved anyway (Gitea #822) — the floor can only be argued
        // from boots that printed it.
        println!("hub75: panel took {} B of internal heap, {} B left (floor {} B)", took, left, BOOT_HEAP_FLOOR);
        let scratch = Scratch::for_geometry(g);
        #[cfg(feature = "hub75-pie")]
        let pads = {
            let pads = luxel_hub75::pie::PairPads::for_geometry(g);
            // Whether the boot framebuffer meets the kernel's rules: 16-byte
            // aligned (buffer_layout) and a column count in whole lanes.
            let fits = luxel_hub75::pie::fits(
                // SAFETY: `front` is `g.words()` formatted `u16`s at a
                // buffer_layout-aligned block; read-only here, for its address.
                unsafe { core::slice::from_raw_parts(front.words, g.words()) },
                g,
            );
            println!(
                "hub75: PIE vector packer {} ({} B of pair pads{})",
                if fits { "active" } else { "INACTIVE — scalar packer" },
                pads.bytes(),
                if fits { "" } else { "; cols must be a multiple of 8" }
            );
            pads
        };

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
            arrange::est_hz_driver(&m, &d),
        );
        Self::print_schedule(&s, g, &d);

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
                    lsb: s.lsb,
                    w: w as u16,
                    h: h as u16,
                    scan: g.rows as u16,
                    fb_bytes: live_bytes as u32,
                    fallback,
                };
                LIVE.lock(|c| c.set(Some(live)));
                LIVE_SCAN.store(g.rows as u16, Ordering::Relaxed);
                LIVE_COLS.store(g.cols as u16, Ordering::Relaxed);
                // The blanking the output task is now running. Arming this
                // also turns `want_blank` from a no-op into a request, so a
                // POST before the panel exists cannot ask for a template the
                // boot has not built yet (Gitea #778).
                WANT_BLANK.store(d.blank, Ordering::Relaxed);
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
                    control: c,
                    sched: s,
                    fmt_gen: 0,
                    back: Some(back),
                    pending: None,
                    last_shown_rescan: 0,
                    next_seq: 1,
                    shown_cursor: 0,
                    last_shown_seq: 0,
                    tables: Some(tables),
                    tables_b5: u8::MAX,
                    scratch,
                    #[cfg(feature = "hub75-pie")]
                    pads,
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
                    ring: spare::Ring::new(g, &s),
                    #[cfg(feature = "hub75-spare-plane")]
                    chase: None,
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
    /// Pick up a latch-blanking change (Gitea #778) — the one `panel` field
    /// that does not wait for a boot.
    ///
    /// `blank` is control bits only, so applying it is: bump the generation
    /// and let each buffer re-`format` itself on its next compose
    /// ([`DynFb::refmt`]). The two swap buffers therefore catch up on their
    /// own next turns, one frame apart, and each is re-formatted while it is
    /// the compose target — never while the DMA is reading it.
    ///
    /// **Spare-plane mode** needs nothing more either: the buffer re-formatted
    /// is the STAGING one, and `flush` copies whole planes out of it —
    /// control bits included, since a plane's bytes are its entry words. The
    /// flip is armed for the view whose plane 0 that copy also rewrites, so
    /// the view the next pass reads is wholly on the new template. The pass
    /// still running reads its old plane 0 against new planes 1.., which is
    /// one rescan of mixed OE width and exactly the transient the mode
    /// already accepts for colour bits.
    ///
    /// Refuses a value that would leave the running row block with no
    /// OE-active clock — the panel would go black — and puts the atomic back
    /// so it is not re-judged every frame. `POST /api/layout` already refuses
    /// that with the numbers ([`blank_would_darken`]); this is the guard for
    /// the one case the POST cannot see, a blanking stored against a geometry
    /// the boot then fell back from.
    fn adopt_blank(&mut self) {
        let want = WANT_BLANK.load(Ordering::Relaxed);
        if want == BLANK_NONE || want == self.control.blank {
            return;
        }
        if 2 * u32::from(want) + u32::from(self.control.latch_clocks) >= self.g.cols as u32 {
            WANT_BLANK.store(self.control.blank, Ordering::Relaxed);
            return;
        }
        self.control.blank = want;
        // The lit width moved; the schedule's `trunc` cannot (the rings
        // encode it), so re-clamp `lsb` to keep every plane's weight exact
        // (Gitea #460). The `live.lsb` reading follows below.
        self.sched = self.sched.refit(self.g, self.control);
        self.fmt_gen = self.fmt_gen.wrapping_add(1);
        // `driver.live.blank` is the console's "what is on the panel" reading,
        // so it moves when the TEMPLATE does — a frame at most after the POST
        // was answered, never eagerly at the POST.
        let lsb = self.sched.lsb;
        LIVE.lock(|c| {
            if let Some(mut l) = c.get() {
                l.blank = want;
                l.lsb = lsb;
                c.set(Some(l));
            }
        });
    }

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
        // A latch-blanking change the API stored since the last frame: control
        // bits only, so it lands here rather than at the next boot (#778).
        self.adopt_blank();
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
                #[cfg(feature = "hub75-pie")]
                &mut self.pads,
                self.g,
                self.control,
                &self.sched,
                self.fmt_gen,
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
                #[cfg(feature = "hub75-pie")]
                &mut self.pads,
                self.g,
                self.control,
                &self.sched,
                self.fmt_gen,
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
        let Some(staging) = self.staging.as_deref() else {
            self.staged = false;
            return true;
        };
        // ---- Start a chase, or resume the one in flight (#829). ----
        //
        // The chase follows the beam instead of waiting for a window: the
        // flip is armed at once (it lands at the wrap — the atomic-swap
        // patch's single tail store), the spare MSB block is written at once
        // (no ring the DMA can reach before the wrap names it), and every
        // other plane is written the moment the DMA has finished reading it
        // for this pass — from then on nothing reads it again until the next
        // pass, which reads the other view. A frame therefore goes out
        // within one pass at every schedule, with no deferral, and the only
        // way to tear is the wrap arriving before the copies did.
        let chase = match self.chase {
            Some(c) => c,
            None => {
                // The previous flip must have landed: only then is the DMA on
                // the other ring and the displaced view's MSB block idle.
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
                let Some((ring, idx, eof)) = hub75.dma_position() else {
                    self.back = Some(back);
                    if self.staged_at.elapsed().as_micros() >= spare::HOLD_US {
                        self.staged = false;
                        shared::SPARE_ABANDONED.fetch_add(1, Ordering::Relaxed);
                        return true;
                    }
                    shared::SPARE_DEFERRED.fetch_add(1, Ordering::Relaxed);
                    return false;
                };
                // Start inside the MSB run, i.e. near the top of a pass, so
                // the whole pass is available to the chase; past it, the
                // next poll (250 µs) finds the next pass soon enough. An
                // unserviced EOF means the position is from the instant of a
                // wrap — re-poll rather than guess which ring.
                if eof || !self.ring.in_msb_run(idx) {
                    self.back = Some(back);
                    shared::SPARE_DEFERRED.fetch_add(1, Ordering::Relaxed);
                    return false;
                }
                let dst = back.planes();
                note_handoff(&mut self.last_shown_rescan, &mut self.next_seq, hub75);
                let swap = hub75.swap(back);
                // The spare MSB first: idle memory until the flip lands.
                let t = esp_hal::time::Instant::now();
                let (src, slen) = staging.plane_ptr_len(0);
                let (d, len) = dst[0];
                debug_assert_eq!(len, slen);
                // SAFETY: both spans are `len` bytes, live, and disjoint —
                // the staging buffer is never a DMA source and the spare
                // block is in neither ring the DMA can reach before the flip.
                unsafe { core::ptr::copy_nonoverlapping(src, d, len) };
                let c = spare::Chase {
                    ring,
                    next: 1,
                    t0: t,
                    worst_us: t.elapsed().as_micros() as u32,
                    dst,
                };
                self.pending = Some(swap);
                self.chase = Some(c);
                c
            }
        };
        let dst = chase.dst;
        let planes = self.ring.planes;
        let free_from = self.ring.free_from();
        let mut c = chase;
        // Spin budget for this poll: at least SPIN_US, and up to one pass —
        // the polls come about one pass apart, so a chase that hands the
        // core back mid-pass at a fast schedule would find the wrap gone by
        // when it resumes. One pass of the output task per frame is what the
        // two-buffer swap already spends waiting for the flip to land.
        let spin_budget = u64::from(hub75.pass_stats().3).max(spare::SPIN_US);
        let spin_from = esp_hal::time::Instant::now();
        let mut torn = false;
        while c.next < planes {
            let p = c.next;
            if p < free_from {
                // Wait for the beam to pass plane `p`, briefly; hand the core
                // back if it takes longer and resume on the next poll.
                loop {
                    match hub75.dma_position() {
                        Some((r, idx, _)) if r == c.ring => {
                            if self.ring.consumed(p, idx) {
                                break;
                            }
                        }
                        // The wrap came first (or the position is gone):
                        // the new pass is already reading whatever is in
                        // the planes not copied yet. Finish now; it tears
                        // for this one pass and is counted.
                        _ => {
                            torn = true;
                            break;
                        }
                    }
                    if spin_from.elapsed().as_micros() >= spin_budget {
                        self.chase = Some(c);
                        shared::SPARE_DEFERRED.fetch_add(1, Ordering::Relaxed);
                        return false;
                    }
                }
            }
            let (src, slen) = staging.plane_ptr_len(p);
            let (d, len) = dst[p];
            debug_assert_eq!(len, slen);
            let t = esp_hal::time::Instant::now();
            // SAFETY: as above; the live plane has been read for this pass
            // (or is one of the two lowest, whose torn pass is invisible).
            unsafe { core::ptr::copy_nonoverlapping(src, d, len) };
            c.worst_us = c.worst_us.max(t.elapsed().as_micros() as u32);
            c.next += 1;
        }
        // Everything is in place. Was it before the wrap? A ring change
        // (or a lost position) means the new pass started on stale planes.
        if torn || hub75.dma_position().is_none_or(|(r, _, _)| r != c.ring) {
            shared::SPARE_TORN_WRAP.fetch_add(1, Ordering::Relaxed);
        }
        let total = c.t0.elapsed().as_micros() as u32;
        // Typical single-plane copy: a fast EWMA (1/4 weight) of the slowest
        // plane in each flush. The worst ever is kept for the record only.
        self.plane_us = (self.plane_us * 3 + c.worst_us) / 4;
        shared::SPARE_PLANE_US.store(self.plane_us, Ordering::Relaxed);
        shared::SPARE_PLANE_US_MAX.fetch_max(c.worst_us, Ordering::Relaxed);
        shared::SPARE_COPY_US.store(total, Ordering::Relaxed);
        shared::SPARE_COPY_US_MAX.fetch_max(total, Ordering::Relaxed);
        shared::SPARE_FLUSHES.fetch_add(1, Ordering::Relaxed);
        self.chase = None;
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
    #[cfg(feature = "hub75-pie")] pads: &mut luxel_hub75::pie::PairPads,
    g: Geometry,
    control: Control,
    sched: &Schedule,
    fmt_gen: u32,
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
    // A control template this buffer has not been written with yet (a latch
    // blanking change, #778). Re-format FIRST: it clears every colour bit,
    // and the pack below writes every one of them back.
    target.refmt(control, sched, fmt_gen);
    let dst = target.fb_words();
    // The vector packer first (Gitea #855); it writes nothing and says so
    // when the buffer does not meet its rules, and the scalar path is then
    // exactly what ran before.
    #[cfg(feature = "hub75-pie")]
    if luxel_hub75::pie::pack_pie(dst, g, rgb, remap, t, pads) {
        return;
    }
    match remap {
        None => luxel_hub75::pack(dst, g, rgb, t, scratch),
        Some(lut) => luxel_hub75::pack_remap(dst, g, rgb, lut, t, scratch),
    }
}
