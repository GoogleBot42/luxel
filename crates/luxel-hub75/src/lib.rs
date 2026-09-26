//! Row-oriented bulk bitplane packer for HUB75 BCM framebuffers (Gitea #329),
//! with the panel geometry chosen at RUNTIME (Gitea #401).
//!
//! The firmware composes every engine frame into a `plane -> row -> column`
//! array of 16-bit LCD_CAM bus words which the DMA rescans autonomously.
//! Stock composition (`hub75-framebuffer`'s `set_pixel`) is per pixel: for
//! each of the `planes` bitplanes it re-derives the row/column index,
//! bounds-checks it, extracts one bit from each of three channel bytes and
//! read-modify-writes a `u16`. On the 64x64 bench panel that is 4096 x 7 =
//! 28,672 such updates and costs a flat **7.0-7.4 ms** per frame — 85 % of
//! the panel's 8.7 ms rescan window, which is why any core-0 stall longer
//! than the remaining 1.3 ms makes the panel rescan the previous frame
//! (Gitea #395).
//!
//! This crate does the same work per *row pair* instead. The six colour bits
//! of an entry are
//!
//! ```text
//! bit 14 13 12 | 11 10  9
//!     B2 G2 R2 | B1 G1 R1        (1 = top panel half, 2 = bottom half)
//! ```
//!
//! so one entry carries one bitplane of one *pixel pair* — column `x` of row
//! `r` (top half) and of row `r + rows` (bottom half). For a given pixel pair
//! all `planes` entries are built from the same six channel bytes; only the
//! bit position changes. So we compute, once per pixel pair, a `u32` in which
//! every plane's six colour bits already sit at a fixed stride, and then each
//! plane is one shift, one mask and one OR.
//!
//! The per-pair value comes out of two 256-entry tables, one lookup per
//! channel:
//!
//! ```text
//! LO[c] = bit(8p)     set to bit(7 - p) of c, for p in 0..4
//! HI[c] = bit(8(p-4)) set to bit(7 - p) of c, for p in 4..8
//! ```
//!
//! Six lookups + five shifts + five ORs give `xlo`/`xhi` for the pair, and
//! plane `p`'s colour field is `(x >> (8 * (p % 4))) & 0x3f`. The tables are
//! built from a caller-supplied 256-entry byte LUT, so **brightness scaling is
//! folded into the same lookup** and disappears from the inner loop entirely.
//!
//! # Runtime geometry
//!
//! Every panel parameter is a persisted device setting applied at boot
//! (Gitea #401/#525), so nothing here is a const generic. A framebuffer is
//! described by a [`Geometry`] — `rows` (address rows, i.e. the scan depth),
//! `cols` (words clocked out per address row) and `planes` — and the caller
//! owns the buffer:
//!
//! 1. allocate `g.words()` `u16`s (plus one [`Tables`] and one [`Scratch`]),
//! 2. [`format`] it once, which writes the control template (row address,
//!    latch, output-enable) that composition must not disturb,
//! 3. [`pack`] (or [`pack_remap`]) every frame, which writes only the colour
//!    bits.
//!
//! [`Scratch`] holds the per-row working pads the packer needs. It is
//! allocated once, sized for the widest `cols` it will ever see, and passed
//! back in on every frame — the packer never allocates.
//!
//! [`arrange`] derives that `Geometry` from a stored `Matrix` layout
//! ([`arrange::fb_geometry`]) and builds the driver→engine remap that makes a
//! chain, a rotated wall or a 1/N-scan panel look row-major to the engine.
//! [`chip`] carries the driver-chip register-init sequences the firmware
//! bit-bangs before the DMA starts, and [`chip::ChipInit::latch_clocks`] is half
//! of the [`Control`] that goes into [`format`].
//!
//! Everything here is layout-sensitive by construction, so the firmware
//! probes the real framebuffer at boot before trusting it (see
//! `firmware/src/hub75.rs`) and the tests in this crate assert byte-for-byte
//! equality against `hub75-framebuffer`'s own `new()` (which formats) +
//! `erase()` + `set_pixel()` path.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

use alloc::vec;
use alloc::vec::Vec;

pub mod arrange;
pub mod chip;

/// The six colour bits of an entry word: R1 G1 B1 R2 G2 B2 at bits 9..=14.
/// Everything else in the word (row address, latch, output-enable) is written
/// by [`format`] and must survive composition.
pub const COLOR_MASK: u16 = 0b0111_1110_0000_0000;

/// Lowest colour bit position (R1).
const COLOR_SHIFT: u32 = 9;

/// Row-address lines A..E, bits 0..=4 of an entry word.
pub const ADDR_MASK: u16 = 0b0001_1111;

/// Latch / STB, bit 5 of an entry word.
pub const LATCH: u16 = 0b0010_0000;

/// Output-enable, bit 8. The pin is active-low in hardware, so a SET bit is
/// output ACTIVE (the panel is lit) and a clear bit is blanked.
pub const OE_ACTIVE: u16 = 0b0001_0000_0000;

/// Planes are addressed `bit = 7 - plane`, so eight planes exhaust a byte.
pub const MAX_PLANES: usize = 8;

/// Number of 16-bit entries in a `plane -> row -> column` framebuffer.
#[must_use]
pub const fn words(nrows: usize, cols: usize, planes: usize) -> usize {
    nrows * cols * planes
}

/// A framebuffer's shape, chosen at boot from the stored layout.
///
/// `rows` is the panel's ADDRESS depth (the `scan` of a 1/N-scan panel), not
/// its pixel height: a row block drives `rows` and `rows + r` at once through
/// R1G1B1/R2G2B2. `cols` is the number of words clocked out per row block —
/// the whole chain, times the stripe count of a panel whose scan is shallower
/// than half its height (see [`arrange::fb_geometry`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Geometry {
    /// Address rows (scan depth).
    pub rows: usize,
    /// Words clocked out per address row.
    pub cols: usize,
    /// Bitplanes (BCM depth), at most [`MAX_PLANES`].
    pub planes: usize,
}

impl Geometry {
    /// Construct a geometry. Nothing is validated here — [`format`] and
    /// [`pack`] assert what they need.
    #[must_use]
    pub const fn new(rows: usize, cols: usize, planes: usize) -> Self {
        Self { rows, cols, planes }
    }

    /// Total 16-bit entries.
    #[must_use]
    pub const fn words(&self) -> usize {
        self.rows * self.cols * self.planes
    }

    /// Entries in one bitplane.
    #[must_use]
    pub const fn plane_words(&self) -> usize {
        self.rows * self.cols
    }

    /// Bytes in one bitplane — the DMA descriptor chunk size.
    #[must_use]
    pub const fn plane_bytes(&self) -> usize {
        self.plane_words() * 2
    }

    /// Driver pixels the framebuffer covers: `rows * 2 * cols`.
    #[must_use]
    pub const fn pixels(&self) -> usize {
        self.rows * 2 * self.cols
    }

    /// Bytes of one whole framebuffer.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.words() * 2
    }
}

/// Where the latch and the output-enable blanking sit in a row block.
///
/// `blank` clocks at the start of the block, and `blank` again just before
/// the latch words, have OE off — the gap that keeps the previous row from
/// ghosting while the address lines settle. `latch_clocks` is how many of the
/// block's final words assert the latch (1 for a plain shift register, 3 for
/// a DP3246 — see [`chip::ChipInit::latch_clocks`]).
///
/// `Control { blank: 1, latch_clocks: 1 }` reproduces `hub75-framebuffer`'s
/// stock template exactly, which the tests assert.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Control {
    /// OE-off clocks at the head of a row block, and again before the latch.
    pub blank: u8,
    /// Words at the end of a row block that assert the latch.
    pub latch_clocks: u8,
}

impl Control {
    #[must_use]
    pub const fn new(blank: u8, latch_clocks: u8) -> Self {
        Self { blank, latch_clocks }
    }
}

impl Default for Control {
    /// `blank: 1, latch_clocks: 1` — the stock template.
    fn default() -> Self {
        Self { blank: 1, latch_clocks: 1 }
    }
}

/// Write the control template into every word of `words`: row address, latch
/// and output-enable per [`Control`], colour bits cleared.
///
/// The address a row block drives is the PREVIOUS row, `(r + rows - 1) %
/// rows`: the latch at the end of block `r` is what moves the shifted data
/// into row `r`, so the address lines must already name `r`'s predecessor
/// while `r` is being clocked in. Only the low five bits are driven (A..E),
/// so a geometry with more than 32 address rows aliases — HUB75 has no sixth
/// address line, and `scan` never exceeds 32 in practice.
///
/// Call this once, at boot, before the first [`pack`]. Packing preserves
/// every bit this writes.
///
/// # Panics
/// If `words` is not exactly `g.words()` entries.
pub fn format(words: &mut [u16], g: Geometry, c: Control) {
    assert_eq!(words.len(), g.words(), "framebuffer length");
    let (rows, cols) = (g.rows, g.cols);
    if rows == 0 || cols == 0 {
        return;
    }
    let blank = usize::from(c.blank);
    let latch = usize::from(c.latch_clocks);
    // OE is on between the head blanking and the trailing blanking, and the
    // latch words are the block's tail. The two never overlap.
    let oe_from = blank;
    let oe_to = cols.saturating_sub(latch + blank);
    let latch_from = cols.saturating_sub(latch);
    let plane_stride = g.plane_words();

    for r in 0..rows {
        let addr = (((r + rows - 1) % rows) as u16) & ADDR_MASK;
        for p in 0..g.planes {
            let base = p * plane_stride + r * cols;
            for (i, w) in words[base..base + cols].iter_mut().enumerate() {
                let mut v = addr;
                if i >= oe_from && i < oe_to {
                    v |= OE_ACTIVE;
                }
                if i >= latch_from {
                    v |= LATCH;
                }
                *w = v;
            }
        }
    }
}

/// The two spread tables, rebuilt whenever brightness changes.
///
/// 2 KiB. Held by the caller (the firmware heap-allocates one alongside the
/// framebuffers) rather than being a static, so nothing is paid on boards
/// without a panel.
#[derive(Clone)]
pub struct Tables {
    lo: [u32; 256],
    hi: [u32; 256],
}

impl Tables {
    /// A zeroed table set — packs every frame black until [`Tables::build`].
    #[must_use]
    pub const fn zeroed() -> Self {
        Self { lo: [0; 256], hi: [0; 256] }
    }

    /// Rebuild from a channel LUT: `lut[c]` is the 8-bit value actually driven
    /// onto the panel for source channel value `c` (i.e. brightness scaling,
    /// gamma, or identity).
    pub fn build(&mut self, lut: &[u8; 256]) {
        for (c, out) in lut.iter().enumerate() {
            let v = u32::from(*out);
            // plane p takes bit 7-p; park it at 8p (planes 0..4) or
            // 8(p-4) (planes 4..8) so the six channels can be ORed in at
            // offsets 0..6 without colliding.
            self.lo[c] = ((v >> 7) & 1) | (((v >> 6) & 1) << 8) | (((v >> 5) & 1) << 16) | (((v >> 4) & 1) << 24);
            self.hi[c] = ((v >> 3) & 1) | (((v >> 2) & 1) << 8) | (((v >> 1) & 1) << 16) | ((v & 1) << 24);
        }
    }

    /// Convenience constructor.
    #[must_use]
    pub fn from_lut(lut: &[u8; 256]) -> Self {
        let mut t = Self::zeroed();
        t.build(lut);
        t
    }
}

impl Default for Tables {
    fn default() -> Self {
        Self::zeroed()
    }
}

/// Per-row working pads the packer needs, allocated ONCE by the caller.
///
/// Two `u32` spreads and two RGB pads, `cols` wide — 14 bytes per column, so
/// 3.5 KiB for a 256-wide chain. The firmware builds one at boot next to the
/// framebuffers and hands the same one to every [`pack`], which is what
/// keeps composition allocation-free on the render path.
///
/// A `Scratch` may be WIDER than the geometry it is used with (the packer
/// slices it to `g.cols`), so a board that falls back to a smaller panel
/// after an allocation failure can reuse the one it already has.
pub struct Scratch {
    xlo: Vec<u32>,
    xhi: Vec<u32>,
    tpad: Vec<[u8; 3]>,
    bpad: Vec<[u8; 3]>,
}

impl Scratch {
    /// Allocate pads for a framebuffer up to `cols` words wide.
    #[must_use]
    pub fn new(cols: usize) -> Self {
        Self {
            xlo: vec![0u32; cols],
            xhi: vec![0u32; cols],
            tpad: vec![[0u8; 3]; cols],
            bpad: vec![[0u8; 3]; cols],
        }
    }

    /// Pads for [`Geometry::cols`].
    #[must_use]
    pub fn for_geometry(g: Geometry) -> Self {
        Self::new(g.cols)
    }

    /// The widest geometry this scratch can pack.
    #[must_use]
    pub fn cols(&self) -> usize {
        self.xlo.len()
    }

    /// Bytes held.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.cols() * (2 * core::mem::size_of::<u32>() + 2 * 3)
    }
}

/// Take a `pad.len()`-wide slice of `rgb` starting at `start`, padding with
/// black if the frame is short (the per-pixel path leaves those pixels
/// erased, so black is exactly equivalent).
fn row_slice<'a>(rgb: &'a [[u8; 3]], start: usize, pad: &'a mut [[u8; 3]]) -> &'a [[u8; 3]] {
    let cols = pad.len();
    if let Some(s) = rgb.get(start..start + cols) {
        return s;
    }
    pad.fill([0; 3]);
    let avail = rgb.len().saturating_sub(start);
    if avail > 0 {
        pad[..avail].copy_from_slice(&rgb[start..start + avail]);
    }
    pad
}

/// Gather one driver row through one row of a remap table ([`arrange`]):
/// `lut[x]` is the engine pixel that belongs at driver column `x`.
/// [`arrange::UNMAPPED`] — and any index past the end of the frame — reads
/// black, exactly as a short frame does above.
fn gather_row<'a>(rgb: &[[u8; 3]], lut: &[u16], pad: &'a mut [[u8; 3]]) -> &'a [[u8; 3]] {
    for (d, e) in pad.iter_mut().zip(lut.iter()) {
        *d = rgb.get(usize::from(*e)).copied().unwrap_or([0; 3]);
    }
    pad
}

/// Pack an RGB888 frame into a `plane -> row -> column` bitplane framebuffer.
///
/// `dst` is the whole framebuffer viewed as 16-bit entries; it must already
/// have been [`format`]ed (row address / latch / output-enable bits are
/// preserved, colour bits are overwritten). `rgb` is row-major, `g.cols`
/// wide, up to `g.rows * 2` tall; a short frame leaves the remaining pixels
/// black, and pixels past the panel are ignored — the same contract as the
/// per-pixel path it replaces.
///
/// Replaces `erase()` + a full `set_pixel` sweep: every colour bit of every
/// entry is written, so no separate erase is needed.
///
/// # Panics
/// If `dst` is not exactly `g.words()` entries, `g.planes` exceeds
/// [`MAX_PLANES`], or `scratch` is narrower than `g.cols`.
pub fn pack(dst: &mut [u16], g: Geometry, rgb: &[[u8; 3]], tables: &Tables, scratch: &mut Scratch) {
    pack_inner(dst, g, rgb, None, tables, scratch);
}

/// [`pack`], but the frame is gathered through a panel→pixel remap
/// (Gitea #475): `lut[driver index]` is the engine pixel that belongs there.
///
/// This is what makes a multi-panel chain — or a rotated, snaked, corner-
/// started or 1/N-scan one — look like a single row-major grid to the engine.
/// A remap that turns out to be the identity is thrown away at boot rather
/// than run through here, so nothing on a plain single upright panel pays
/// for the gather (see [`arrange::is_identity`]).
///
/// # Panics
/// If `lut` is not exactly `g.pixels()` entries, or on [`pack`]'s own
/// conditions.
pub fn pack_remap(
    dst: &mut [u16],
    g: Geometry,
    rgb: &[[u8; 3]],
    lut: &[u16],
    tables: &Tables,
    scratch: &mut Scratch,
) {
    assert_eq!(lut.len(), g.pixels(), "remap table length");
    pack_inner(dst, g, rgb, Some(lut), tables, scratch);
}

fn pack_inner(
    dst: &mut [u16],
    g: Geometry,
    rgb: &[[u8; 3]],
    lut: Option<&[u16]>,
    tables: &Tables,
    scratch: &mut Scratch,
) {
    assert!(g.planes <= MAX_PLANES, "bit = 7 - plane; more than 8 planes has no source bit");
    assert_eq!(dst.len(), g.words(), "framebuffer length");
    assert!(scratch.cols() >= g.cols, "scratch too narrow for the geometry");

    let (rows, cols, planes) = (g.rows, g.cols, g.planes);
    let plane_stride = g.plane_words();
    // Per row pair: the six-channel spread of every column, split across two
    // u32s because eight planes at a 6-bit stride would not fit one.
    let Scratch { xlo, xhi, tpad, bpad } = scratch;
    let xlo = &mut xlo[..cols];
    let xhi = &mut xhi[..cols];

    for r in 0..rows {
        // One branch per ROW PAIR, not per pixel: an unmapped panel walks
        // exactly the slices it always walked.
        let (top, bot) = match lut {
            None => (
                row_slice(rgb, r * cols, &mut tpad[..cols]),
                row_slice(rgb, (r + rows) * cols, &mut bpad[..cols]),
            ),
            Some(l) => (
                gather_row(rgb, &l[r * cols..(r + 1) * cols], &mut tpad[..cols]),
                gather_row(rgb, &l[(r + rows) * cols..(r + rows + 1) * cols], &mut bpad[..cols]),
            ),
        };

        // Iterators, not indices: with `cols` no longer a const generic, an
        // `xlo[x]` here is a bounds check the compiler cannot hoist.
        let pairs = top.iter().zip(bot.iter());
        for ((xl, xh), (t, b)) in xlo.iter_mut().zip(xhi.iter_mut()).zip(pairs) {
            let (rt, gt, bt) = (usize::from(t[0]), usize::from(t[1]), usize::from(t[2]));
            let (rb, gb, bb) = (usize::from(b[0]), usize::from(b[1]), usize::from(b[2]));
            *xl = tables.lo[rt]
                | (tables.lo[gt] << 1)
                | (tables.lo[bt] << 2)
                | (tables.lo[rb] << 3)
                | (tables.lo[gb] << 4)
                | (tables.lo[bb] << 5);
            *xh = tables.hi[rt]
                | (tables.hi[gt] << 1)
                | (tables.hi[bt] << 2)
                | (tables.hi[rb] << 3)
                | (tables.hi[gb] << 4)
                | (tables.hi[bb] << 5);
        }

        for p in 0..planes {
            let base = p * plane_stride + r * cols;
            let row = &mut dst[base..base + cols];
            let (src, sh) = if p < 4 { (&xlo, 8 * p as u32) } else { (&xhi, 8 * (p - 4) as u32) };
            for (d, v) in row.iter_mut().zip(src.iter()) {
                let bits = ((v >> sh) as u16) & 0x3f;
                *d = (*d & !COLOR_MASK) | (bits << COLOR_SHIFT);
            }
        }
    }
}

/// Spare-plane swap window check (Gitea #610), pure so the firmware's
/// `hub75.rs` can delegate and this crate can test it on the host.
///
/// The DMA ring is plane-major with plane 0 the MSB repeated `2^(planes-1)`
/// times, so descriptor indices `0..msb_descs` of a pass read only plane 0.
/// `idx` is the engine's descriptor index from the ring head, `eof_pending`
/// whether an unserviced `out_eof` says the reading is from the instant of a
/// wrap, `nominal_us` the ISR's pass length (0 = not yet known), `plane_us`
/// the slowest single-plane copy seen, `descs` the ring length, `slack_ns`
/// the margin kept beyond the copy cost.
///
/// Room means: plane 1 (the first thing read after the MSB run, and the
/// first plane copied once the flip is armed) can land before the DMA leaves
/// the run — two copies' worth, the second being margin — and all `planes`
/// copies can land before the wrap.
#[must_use]
// the ring geometry is eight plain numbers; bundling them into a struct here
// would only move the same eight to the caller (the firmware builds one — see
// `spare::Window` in `firmware/src/hub75.rs`)
#[allow(clippy::too_many_arguments)]
pub fn spare_window_fits(
    idx: usize,
    eof_pending: bool,
    nominal_us: u32,
    plane_us: u32,
    descs: usize,
    msb_descs: usize,
    planes: usize,
    slack_ns: u64,
) -> bool {
    if eof_pending || idx >= msb_descs || descs == 0 {
        return false;
    }
    if nominal_us == 0 {
        return idx <= msb_descs / 2;
    }
    let desc_ns = u64::from(nominal_us) * 1000 / descs as u64;
    let plane_ns = u64::from(plane_us) * 1000;
    let left_msb = (msb_descs - idx) as u64 * desc_ns;
    let left_wrap = (descs - idx) as u64 * desc_ns;
    left_msb >= 2 * plane_ns + slack_ns && left_wrap >= planes as u64 * plane_ns + slack_ns
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::prelude::Point;
    use hub75_framebuffer::bitplane::plain::DmaFrameBuffer;
    use hub75_framebuffer::Color;
    use std::vec::Vec;
    use std::{format, vec};

    const NROWS: usize = 32;
    const COLS: usize = 64;
    const PLANES: usize = 7;
    const PIXELS: usize = COLS * NROWS * 2;
    const G: Geometry = Geometry::new(NROWS, COLS, PLANES);
    type Fb = DmaFrameBuffer<NROWS, COLS, PLANES>;

    /// The assumption the whole packer rests on: the framebuffer is a flat
    /// array of entries laid out plane-major, with no padding. If a
    /// `hub75-framebuffer` feature (`inter-row-blank-*`, `tail-closes-latch`)
    /// were ever switched on this fails first, and loudly.
    #[test]
    fn framebuffer_is_a_flat_entry_array() {
        assert_eq!(core::mem::size_of::<Fb>(), G.bytes());
        assert_eq!(G.words(), words(NROWS, COLS, PLANES));
    }

    fn as_words(fb: &Fb) -> &[u16] {
        // SAFETY-equivalent: the struct is repr(C) over [Entry; N] with
        // Entry a repr(transparent) u16, asserted above.
        #[allow(unsafe_code)]
        unsafe {
            core::slice::from_raw_parts((fb as *const Fb).cast::<u16>(), G.words())
        }
    }

    fn as_words_mut(fb: &mut Fb) -> &mut [u16] {
        #[allow(unsafe_code)]
        unsafe {
            core::slice::from_raw_parts_mut((fb as *mut Fb).cast::<u16>(), G.words())
        }
    }

    /// Exactly `firmware/src/hub75.rs`'s pre-#329 compose.
    fn scale5(channel: u8, brightness5: u8) -> u8 {
        ((u16::from(channel) * u16::from(brightness5 & 0x1f)) / 31) as u8
    }

    fn lut_for(brightness5: u8) -> [u8; 256] {
        let mut lut = [0u8; 256];
        let full = brightness5 >= 31;
        for (c, v) in lut.iter_mut().enumerate() {
            let c = c as u8;
            *v = if full { c } else { scale5(c, brightness5) };
        }
        lut
    }

    fn reference(rgb: &[[u8; 3]], brightness5: u8) -> Fb {
        let mut fb = Fb::new();
        fb.erase();
        if brightness5 > 0 {
            let full = brightness5 >= 31;
            for (i, px) in rgb.iter().enumerate().take(PIXELS) {
                let [r, g, b] = *px;
                let (r, g, b) = if full {
                    (r, g, b)
                } else {
                    (scale5(r, brightness5), scale5(g, brightness5), scale5(b, brightness5))
                };
                let p = Point::new((i % COLS) as i32, (i / COLS) as i32);
                fb.set_pixel(p, Color::new(r, g, b));
            }
        }
        fb
    }

    fn packed(rgb: &[[u8; 3]], brightness5: u8) -> Fb {
        let mut fb = Fb::new();
        let tables = Tables::from_lut(&lut_for(brightness5));
        let mut scratch = Scratch::for_geometry(G);
        pack(as_words_mut(&mut fb), G, rgb, &tables, &mut scratch);
        fb
    }

    fn assert_identical(rgb: &[[u8; 3]], brightness5: u8, what: &str) {
        let want = reference(rgb, brightness5);
        let got = packed(rgb, brightness5);
        let (w, g) = (as_words(&want), as_words(&got));
        if w != g {
            let i = w.iter().zip(g.iter()).position(|(a, b)| a != b).unwrap();
            let plane = i / (NROWS * COLS);
            let row = (i / COLS) % NROWS;
            let col = i % COLS;
            panic!(
                "{what} @ b5={brightness5}: word {i} (plane {plane} row {row} col {col}) \
                 want {:#06x} got {:#06x}",
                w[i], g[i]
            );
        }
    }

    /// Deterministic xorshift — no dev-dependency for a handful of frames.
    struct Rng(u64);
    impl Rng {
        fn next_u8(&mut self) -> u8 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 24) as u8
        }
        fn frame(&mut self, n: usize) -> Vec<[u8; 3]> {
            (0..n).map(|_| [self.next_u8(), self.next_u8(), self.next_u8()]).collect()
        }
    }

    #[test]
    fn random_frames_are_byte_identical_at_every_brightness() {
        let mut rng = Rng(0x2026_0907_0329);
        for trial in 0..8 {
            let frame = rng.frame(PIXELS);
            for b5 in 0..=31u8 {
                assert_identical(&frame, b5, &format!("random frame {trial}"));
            }
        }
    }

    #[test]
    fn edge_colours_are_byte_identical() {
        // Every combination of the interesting channel values, tiled over a
        // full frame so each lands at many (row, column, half) positions.
        let edges = [0u8, 1, 2, 127, 128, 129, 254, 255];
        let mut frame = vec![[0u8; 3]; PIXELS];
        for (k, px) in frame.iter_mut().enumerate() {
            *px = [
                edges[k % edges.len()],
                edges[(k / edges.len()) % edges.len()],
                edges[(k / (edges.len() * edges.len())) % edges.len()],
            ];
        }
        for b5 in 0..=31u8 {
            assert_identical(&frame, b5, "edge colours");
        }
        // And the solid extremes.
        for v in edges {
            let solid = vec![[v; 3]; PIXELS];
            for b5 in [0u8, 1, 3, 15, 30, 31] {
                assert_identical(&solid, b5, "solid");
            }
        }
    }

    #[test]
    fn brightness_31_is_a_passthrough() {
        let mut rng = Rng(7);
        let frame = rng.frame(PIXELS);
        let fb = packed(&frame, 31);
        // Reconstruct pixel (5, 40) from the planes and check it round-trips.
        let w = as_words(&fb);
        let (x, y) = (5usize, 40usize);
        let (row, half_shift) = (y - NROWS, 12u32);
        let mut got = [0u8; 3];
        for p in 0..PLANES {
            let e = w[p * NROWS * COLS + row * COLS + x];
            let bits = (e >> half_shift) & 0x7;
            let bit = 7 - p;
            for (c, g) in got.iter_mut().enumerate() {
                *g |= (((bits >> c) & 1) as u8) << bit;
            }
        }
        // Plane set covers bits 7..=1, so bit 0 of the source is not carried.
        let want = frame[y * COLS + x].map(|c| c & 0xfe);
        assert_eq!(got, want);
    }

    #[test]
    fn short_and_oversized_frames_match_the_per_pixel_path() {
        let mut rng = Rng(99);
        for n in [0usize, 1, 63, 64, 65, COLS * NROWS, PIXELS - 1, PIXELS, PIXELS + 137] {
            let frame = rng.frame(n);
            for b5 in [0u8, 3, 17, 31] {
                assert_identical(&frame, b5, &format!("frame of {n} px"));
            }
        }
    }

    /// Our own [`format`] template, then composition on top of it: the
    /// control bits must come out of `pack` exactly as `format` left them.
    #[test]
    fn control_bits_survive_composition() {
        let mut rng = Rng(5);
        let frame = rng.frame(PIXELS);
        let mut template = vec![0u16; G.words()];
        format(&mut template, G, Control::default());
        let mut got = template.clone();
        let t = Tables::from_lut(&lut_for(19));
        let mut scratch = Scratch::for_geometry(G);
        pack(&mut got, G, &frame, &t, &mut scratch);
        for (a, b) in template.iter().zip(got.iter()) {
            assert_eq!(a & !COLOR_MASK, b & !COLOR_MASK, "non-colour bits changed");
        }
        // …and something actually landed in the colour bits.
        assert!(got.iter().any(|w| w & COLOR_MASK != 0));
    }

    /// The canary for the whole control template: `format` on a stock
    /// `Control` must be byte-identical to what `hub75-framebuffer` writes
    /// for itself in `DmaFrameBuffer::new()`.
    #[test]
    fn format_matches_the_stock_framebuffer_template() {
        let stock = Fb::new();
        let mut ours = vec![0u16; G.words()];
        format(&mut ours, G, Control::default());
        assert_eq!(as_words(&stock), &ours[..]);

        // Other shapes too — the address wrap is per-row-count.
        fn check<const NR: usize, const C: usize, const P: usize>() {
            let g = Geometry::new(NR, C, P);
            let stock = DmaFrameBuffer::<NR, C, P>::new();
            #[allow(unsafe_code)]
            let w = unsafe {
                core::slice::from_raw_parts(
                    (&stock as *const DmaFrameBuffer<NR, C, P>).cast::<u16>(),
                    g.words(),
                )
            };
            let mut ours = vec![0u16; g.words()];
            format(&mut ours, g, Control::default());
            assert_eq!(w, &ours[..], "{NR}x{C}x{P}");
        }
        check::<16, 64, 7>();
        check::<32, 128, 7>();
        check::<8, 32, 5>();
        check::<32, 64, 4>();
        check::<1, 16, 1>();
    }

    /// Wider blanking and a DP3246's three latch clocks, written out
    /// explicitly: `blank = 2` pushes OE on at word 2 and off again three
    /// words before the latch run, which itself is the last three words.
    #[test]
    fn a_wider_template_has_the_expected_word_pattern() {
        let g = Geometry::new(4, 16, 2);
        let mut w = vec![0u16; g.words()];
        format(&mut w, g, Control::new(2, 3));
        // row 0 of plane 0 addresses row 3 (the previous one)
        let row = &w[0..16];
        let a = 3u16;
        let want: [u16; 16] = [
            a,              // 0  head blanking
            a,              // 1  head blanking
            a | OE_ACTIVE,  // 2  ..
            a | OE_ACTIVE,  // 3
            a | OE_ACTIVE,  // 4
            a | OE_ACTIVE,  // 5
            a | OE_ACTIVE,  // 6
            a | OE_ACTIVE,  // 7
            a | OE_ACTIVE,  // 8
            a | OE_ACTIVE,  // 9
            a | OE_ACTIVE,  // 10 last OE word: cols - latch - blank = 11
            a,              // 11 trailing blanking
            a,              // 12 trailing blanking
            a | LATCH,      // 13 latch run = the last 3 words
            a | LATCH,      // 14
            a | LATCH,      // 15
        ];
        assert_eq!(row, &want[..]);
        // every row block of every plane is the same but for the address
        for p in 0..g.planes {
            for r in 0..g.rows {
                let base = p * g.plane_words() + r * g.cols;
                let addr = ((r + g.rows - 1) % g.rows) as u16;
                for (i, got) in w[base..base + g.cols].iter().enumerate() {
                    assert_eq!(*got, (want[i] & !ADDR_MASK) | addr, "plane {p} row {r} word {i}");
                }
            }
        }
    }

    /// `blank = 0` lights the whole block up to the latch; `latch_clocks = 0`
    /// asserts no latch at all (a degenerate but well-defined template).
    #[test]
    fn degenerate_templates_are_well_defined() {
        let g = Geometry::new(2, 8, 1);
        let mut w = vec![0u16; g.words()];
        // blank 0: OE from word 0, off only for the latch word itself.
        format(&mut w, g, Control::new(0, 1));
        let lit = 1 | OE_ACTIVE;
        assert_eq!(&w[0..8], &[lit, lit, lit, lit, lit, lit, lit, 1 | LATCH][..]);
        format(&mut w, g, Control::new(0, 0));
        assert!(w[0..8].iter().all(|x| x & LATCH == 0));
        // A latch run longer than the block leaves the whole block latched.
        format(&mut w, g, Control::new(1, 9));
        assert!(w[0..8].iter().all(|x| x & LATCH != 0 && x & OE_ACTIVE == 0));
    }

    #[test]
    fn packing_twice_is_idempotent_and_clears_the_previous_frame() {
        let mut rng = Rng(11);
        let first = rng.frame(PIXELS);
        let second = rng.frame(PIXELS);
        let mut fb = Fb::new();
        let t = Tables::from_lut(&lut_for(24));
        let mut scratch = Scratch::for_geometry(G);
        pack(as_words_mut(&mut fb), G, &first, &t, &mut scratch);
        pack(as_words_mut(&mut fb), G, &second, &t, &mut scratch);
        assert_eq!(as_words(&fb), as_words(&reference(&second, 24)));
    }

    #[test]
    fn other_panel_geometries_pack_identically() {
        // Smaller/odd shapes, including the 8-plane full-depth case, all
        // against `DmaFrameBuffer<NR, C, P>` as the oracle.
        fn check<const NR: usize, const C: usize, const P: usize>(seed: u64) {
            let g = Geometry::new(NR, C, P);
            let mut rng = Rng(seed);
            let frame = rng.frame(C * NR * 2);
            let mut want = DmaFrameBuffer::<NR, C, P>::new();
            want.erase();
            for (i, px) in frame.iter().enumerate() {
                let [r, g, b] = px.map(|c| scale5(c, 9));
                want.set_pixel(Point::new((i % C) as i32, (i / C) as i32), Color::new(r, g, b));
            }
            let mut got = DmaFrameBuffer::<NR, C, P>::new();
            let n = g.words();
            let t = Tables::from_lut(&lut_for(9));
            let mut scratch = Scratch::for_geometry(g);
            #[allow(unsafe_code)]
            let dst = unsafe {
                core::slice::from_raw_parts_mut((&mut got as *mut DmaFrameBuffer<NR, C, P>).cast::<u16>(), n)
            };
            pack(dst, g, &frame, &t, &mut scratch);
            #[allow(unsafe_code)]
            let (w, gw) = unsafe {
                (
                    core::slice::from_raw_parts((&want as *const DmaFrameBuffer<NR, C, P>).cast::<u16>(), n),
                    core::slice::from_raw_parts((&got as *const DmaFrameBuffer<NR, C, P>).cast::<u16>(), n),
                )
            };
            assert_eq!(w, gw, "{NR}x{C}x{P}");
        }
        check::<16, 64, 8>(1);
        check::<16, 32, 6>(2);
        check::<32, 64, 7>(3);
        check::<8, 128, 4>(4);
        check::<4, 16, 1>(5);
        // the geometries the runtime API newly has to cover (#401): 1/16 and
        // 1/8 scan panels, wide chains, and low bit depths
        check::<16, 64, 7>(6);
        check::<32, 64, 8>(7);
        check::<32, 128, 7>(8);
        check::<8, 32, 5>(9);
        check::<32, 64, 4>(10);
        check::<16, 256, 7>(11);
    }

    /// One `Scratch` sized for the widest geometry must pack a narrower one
    /// identically — what a board that fell back to a smaller panel does.
    #[test]
    fn an_oversized_scratch_packs_a_narrower_geometry() {
        let g = Geometry::new(16, 32, 6);
        let mut rng = Rng(0x2026_0925);
        let frame = rng.frame(g.pixels());
        let t = Tables::from_lut(&lut_for(21));
        let mut a = vec![0u16; g.words()];
        let mut b = vec![0u16; g.words()];
        format(&mut a, g, Control::default());
        format(&mut b, g, Control::default());
        pack(&mut a, g, &frame, &t, &mut Scratch::for_geometry(g));
        pack(&mut b, g, &frame, &t, &mut Scratch::new(256));
        assert_eq!(a, b);
    }

    /// Frames from the real engine, on the real 64x64 grid map the bench
    /// panel runs — the inputs the packer will actually see. Rendered here
    /// rather than committed as fixtures so a library or engine change can
    /// never leave the assertion testing a stale frame.
    #[test]
    fn rendered_library_frames_are_byte_identical() {
        use luxel_core::engine::Engine;
        use luxel_core::fixed::Fx;

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for name in ["frame-rate-scan", "rainbow", "snake-2d-v2"] {
            let path = root.join("library").join(format!("{name}.js"));
            let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
            let mut engine = Engine::new_at(&src, PIXELS as u32, 1, Some(1_757_000_000))
                .unwrap_or_else(|d| panic!("{name}: {}", d.message));
            let coords: Vec<[Fx; 3]> = (0..PIXELS)
                .map(|i| [Fx::from_int((i % COLS) as i32), Fx::from_int((i / COLS) as i32), Fx::ZERO])
                .collect();
            assert!(engine.set_map(2, &coords), "{name}: set_map");
            // A spread of frames: the first (often a special case), then far
            // enough in that time-varying patterns have moved.
            let delta = Fx::from_f64(1000.0 / 60.0);
            for f in 0..40u32 {
                let frame: Vec<[u8; 3]> = engine.frame(delta).to_vec();
                if f % 7 != 0 {
                    continue;
                }
                // Brightness 3 is the bench panel's found state; 31 and 0 are
                // the passthrough and blackout edges.
                for b5 in [0u8, 1, 3, 12, 30, 31] {
                    assert_identical(&frame, b5, &format!("{name} frame {f}"));
                }
            }
            assert!(engine.take_error().is_none(), "{name}: runtime error");
        }
    }

    /// Host micro-benchmark, opt-in:
    ///   cargo test -p luxel-hub75 --release -- --ignored --nocapture
    /// x86 numbers are not Xtensa numbers, but the ratio is indicative.
    ///
    /// Three ways of composing the same frame: the per-pixel path, the
    /// runtime-dimension packer, and a const-generic copy of the packer that
    /// exists ONLY here — it is the pre-#401 shape, kept so the cost of
    /// moving the dimensions to run time stays visible. Nothing is asserted:
    /// timings on a shared host are not a pass/fail signal.
    #[test]
    #[ignore = "timing benchmark"]
    fn bench_pack_vs_set_pixel() {
        use std::time::Instant;
        let mut rng = Rng(0xbeef);
        let frame = rng.frame(PIXELS);
        let t = Tables::from_lut(&lut_for(3));
        let mut fb = Fb::new();
        const N: u32 = 500;

        // Compose only — the framebuffer is allocated and format()ed once on
        // the device too, so neither side pays for that here.
        let lut = lut_for(3);
        let start = Instant::now();
        for _ in 0..N {
            fb.erase();
            for (i, px) in std::hint::black_box(&frame).iter().enumerate().take(PIXELS) {
                let [r, g, b] = px.map(|c| lut[usize::from(c)]);
                fb.set_pixel(Point::new((i % COLS) as i32, (i / COLS) as i32), Color::new(r, g, b));
            }
        }
        let old = start.elapsed() / N;

        let mut scratch = Scratch::for_geometry(G);
        let start = Instant::now();
        for _ in 0..N {
            pack(as_words_mut(&mut fb), G, std::hint::black_box(&frame), &t, &mut scratch);
        }
        let new = start.elapsed() / N;

        let start = Instant::now();
        for _ in 0..N {
            pack_const::<NROWS, COLS, PLANES>(as_words_mut(&mut fb), std::hint::black_box(&frame), &t);
        }
        let konst = start.elapsed() / N;

        std::println!(
            "per-pixel erase+set_pixel: {old:?}/frame\n\
             runtime-dimension pack:    {new:?}/frame  ({:.2}x)\n\
             const-generic pack:        {konst:?}/frame  ({:.2}x)\n\
             runtime vs const-generic:  {:.2}x",
            old.as_secs_f64() / new.as_secs_f64(),
            old.as_secs_f64() / konst.as_secs_f64(),
            konst.as_secs_f64() / new.as_secs_f64(),
        );
    }

    /// The pre-#401 const-generic packer, for the benchmark above only.
    fn pack_const<const NROWS: usize, const COLS: usize, const PLANES: usize>(
        dst: &mut [u16],
        rgb: &[[u8; 3]],
        tables: &Tables,
    ) {
        let plane_stride = NROWS * COLS;
        let mut xlo = [0u32; COLS];
        let mut xhi = [0u32; COLS];
        let mut tpad = [[0u8; 3]; COLS];
        let mut bpad = [[0u8; 3]; COLS];
        for r in 0..NROWS {
            let top = row_slice(rgb, r * COLS, &mut tpad[..]);
            let bot = row_slice(rgb, (r + NROWS) * COLS, &mut bpad[..]);
            for (x, (t, b)) in top.iter().zip(bot.iter()).enumerate() {
                let (rt, gt, bt) = (usize::from(t[0]), usize::from(t[1]), usize::from(t[2]));
                let (rb, gb, bb) = (usize::from(b[0]), usize::from(b[1]), usize::from(b[2]));
                xlo[x] = tables.lo[rt]
                    | (tables.lo[gt] << 1)
                    | (tables.lo[bt] << 2)
                    | (tables.lo[rb] << 3)
                    | (tables.lo[gb] << 4)
                    | (tables.lo[bb] << 5);
                xhi[x] = tables.hi[rt]
                    | (tables.hi[gt] << 1)
                    | (tables.hi[bt] << 2)
                    | (tables.hi[rb] << 3)
                    | (tables.hi[gb] << 4)
                    | (tables.hi[bb] << 5);
            }
            for p in 0..PLANES {
                let base = p * plane_stride + r * COLS;
                let row = &mut dst[base..base + COLS];
                let (src, sh) = if p < 4 { (&xlo, 8 * p as u32) } else { (&xhi, 8 * (p - 4) as u32) };
                for (d, v) in row.iter_mut().zip(src.iter()) {
                    let bits = ((v >> sh) as u16) & 0x3f;
                    *d = (*d & !COLOR_MASK) | (bits << COLOR_SHIFT);
                }
            }
        }
    }

    // --- the panel→pixel remap (Gitea #475) -------------------------------

    /// An identity remap must compose exactly what the plain path composes —
    /// the property the firmware relies on when it throws the table away.
    #[test]
    fn an_identity_remap_packs_identically() {
        let mut rng = Rng(0x2026_0919_51d0);
        let frame = rng.frame(PIXELS);
        let t = Tables::from_lut(&lut_for(19));
        let lut: Vec<u16> = (0..PIXELS as u16).collect();
        let mut a = Fb::new();
        let mut b = Fb::new();
        let mut scratch = Scratch::for_geometry(G);
        pack(as_words_mut(&mut a), G, &frame, &t, &mut scratch);
        pack_remap(as_words_mut(&mut b), G, &frame, &lut, &t, &mut scratch);
        assert_eq!(as_words(&a), as_words(&b));
    }

    /// A real arrangement must compose exactly what packing the rearranged
    /// frame would — i.e. the gather is the only difference.
    #[test]
    fn a_remapped_frame_packs_as_the_rearranged_frame() {
        let mut rng = Rng(0x2026_0919_475a);
        let frame = rng.frame(PIXELS);
        let t = Tables::from_lut(&lut_for(31));
        // two 32-wide tiles, chain starting at the top-right: halves swapped
        let mut m = luxel_core::layout::Matrix::single(32, 64);
        m.cols = 2;
        m.start = luxel_core::layout::Corner::Tr;
        let mut lut = vec![0u16; PIXELS];
        assert_eq!(crate::arrange::build_lut(&mut lut, &m, COLS, NROWS * 2), 2);

        let rearranged: Vec<[u8; 3]> =
            lut.iter().map(|&e| frame.get(usize::from(e)).copied().unwrap_or([0; 3])).collect();
        let mut a = Fb::new();
        let mut b = Fb::new();
        let mut scratch = Scratch::for_geometry(G);
        pack(as_words_mut(&mut a), G, &rearranged, &t, &mut scratch);
        pack_remap(as_words_mut(&mut b), G, &frame, &lut, &t, &mut scratch);
        assert_eq!(as_words(&a), as_words(&b));
        // and it really did move: the two halves are not where they were
        let plain = {
            let mut fb = Fb::new();
            pack(as_words_mut(&mut fb), G, &frame, &t, &mut scratch);
            fb
        };
        assert_ne!(as_words(&plain), as_words(&b));
    }

    /// An unmapped driver pixel is black, exactly like a short frame's tail.
    #[test]
    fn unmapped_driver_pixels_compose_black() {
        let mut rng = Rng(0x2026_0919_1234);
        let frame = rng.frame(PIXELS);
        let t = Tables::from_lut(&lut_for(31));
        let mut lut: Vec<u16> = (0..PIXELS as u16).collect();
        for e in lut.iter_mut().skip(PIXELS / 2) {
            *e = crate::arrange::UNMAPPED;
        }
        let mut half = frame.clone();
        half[PIXELS / 2..].fill([0; 3]);
        let mut a = Fb::new();
        let mut b = Fb::new();
        let mut scratch = Scratch::for_geometry(G);
        pack(as_words_mut(&mut a), G, &half, &t, &mut scratch);
        pack_remap(as_words_mut(&mut b), G, &frame, &lut, &t, &mut scratch);
        assert_eq!(as_words(&a), as_words(&b));
    }

    /// The 64x64 / 7-plane bench ring: 254 descriptors, MSB run = 128 of
    /// them, 8,700 us per pass (~34 us per descriptor).
    #[test]
    fn spare_window_bench_geometry() {
        let (descs, msb, planes, slack) = (254usize, 128usize, 7usize, 300_000u64);
        let fits = |idx, eof, nominal, plane_us| spare_window_fits(idx, eof, nominal, plane_us, descs, msb, planes, slack);
        // Head of the pass, 200 us per plane: 4.4 ms of MSB run left. Fits.
        assert!(fits(0, false, 8_700, 200));
        // A pending EOF is a reading from the wrap: never trust it.
        assert!(!fits(0, true, 8_700, 200));
        // Past the MSB run the low planes are being read: never.
        assert!(!fits(128, false, 8_700, 200));
        assert!(!fits(253, false, 8_700, 200));
        // 20 descriptors before the run ends = ~685 us left: two 200 us
        // copies plus 300 us slack is 700 us — just too tight.
        assert!(!fits(108, false, 8_700, 200));
        // 25 descriptors = ~856 us: fits.
        assert!(fits(103, false, 8_700, 200));
        // Wrap constraint: 7 planes x 1,000 us + slack = 7.3 ms, but from
        // idx 100 only ~5.3 ms of the pass remain.
        assert!(!fits(100, false, 8_700, 1_000));
        // From the head 8.7 ms remain: fits (MSB run 4.4 ms vs 2.3 ms needed).
        assert!(fits(0, false, 8_700, 1_000));
        // No pass length known yet: first half of the run only.
        assert!(fits(64, false, 0, 200));
        assert!(!fits(65, false, 0, 200));
    }

    /// The 256-column chain at 7 planes plain BCM: 635 descriptors, MSB run
    /// 320, 34.7 ms per pass. Plane copies of ~450 us fit from anywhere in
    /// the first ~90 % of the run.
    #[test]
    fn spare_window_wide_chain() {
        let fits = |idx| spare_window_fits(idx, false, 34_700, 450, 635, 320, 7, 300_000);
        assert!(fits(0));
        assert!(fits(290));
        assert!(!fits(319));
        assert!(!fits(320));
    }

    #[test]
    fn spare_window_empty_ring() {
        assert!(!spare_window_fits(0, false, 8_700, 200, 0, 0, 7, 0));
    }
}
