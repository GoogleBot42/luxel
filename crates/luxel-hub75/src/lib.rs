//! Row-oriented bulk bitplane packer for HUB75 BCM framebuffers (Gitea #329).
//!
//! The firmware composes every engine frame into an `esp-hub75`
//! `bitplane::plain::DmaFrameBuffer`, which the LCD_CAM DMA rescans
//! autonomously. Stock composition is `set_pixel` per pixel: for each of the
//! `PLANES` bitplanes it re-derives the row/column index, bounds-checks it,
//! extracts one bit from each of three channel bytes and read-modify-writes a
//! `u16`. On the 64x64 bench panel that is 4096 x 7 = 28,672 such updates and
//! costs a flat **7.0-7.4 ms** per frame — 85 % of the panel's 8.7 ms rescan
//! window, which is why any core-0 stall longer than the remaining 1.3 ms
//! makes the panel rescan the previous frame (Gitea #395).
//!
//! This crate does the same work per *row pair* instead. The framebuffer is a
//! flat array of 16-bit entries laid out `plane -> row -> column`, and the six
//! colour bits of an entry are
//!
//! ```text
//! bit 14 13 12 | 11 10  9
//!     B2 G2 R2 | B1 G1 R1        (1 = top panel half, 2 = bottom half)
//! ```
//!
//! so one entry carries one bitplane of one *pixel pair* — column `x` of row
//! `r` (top half) and of row `r + NROWS` (bottom half). For a given pixel pair
//! all `PLANES` entries are built from the same six channel bytes; only the
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
//! Everything here is layout-sensitive by construction, so the firmware
//! probes the real framebuffer at boot before trusting it (see
//! `firmware/src/hub75.rs`) and the tests in this crate assert byte-for-byte
//! equality against `hub75-framebuffer`'s own `erase()` + `set_pixel()` path.

#![no_std]
#![deny(unsafe_code)]

#[cfg(test)]
extern crate std;

/// The six colour bits of an entry word: R1 G1 B1 R2 G2 B2 at bits 9..=14.
/// Everything else in the word (row address, latch, output-enable) is written
/// by the framebuffer's own `format()` and must survive composition.
pub const COLOR_MASK: u16 = 0b0111_1110_0000_0000;

/// Lowest colour bit position (R1).
const COLOR_SHIFT: u32 = 9;

/// Planes are addressed `bit = 7 - plane`, so eight planes exhaust a byte.
pub const MAX_PLANES: usize = 8;

/// Number of 16-bit entries in a `plane -> row -> column` framebuffer.
#[must_use]
pub const fn words(nrows: usize, cols: usize, planes: usize) -> usize {
    nrows * cols * planes
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

/// Take a `COLS`-wide slice of `rgb` starting at `start`, padding with black
/// if the frame is short (the per-pixel path leaves those pixels erased, so
/// black is exactly equivalent).
fn row_slice<'a, const COLS: usize>(
    rgb: &'a [[u8; 3]],
    start: usize,
    pad: &'a mut [[u8; 3]; COLS],
) -> &'a [[u8; 3]] {
    if let Some(s) = rgb.get(start..start + COLS) {
        return s;
    }
    *pad = [[0; 3]; COLS];
    let avail = rgb.len().saturating_sub(start);
    if avail > 0 {
        pad[..avail].copy_from_slice(&rgb[start..start + avail]);
    }
    &pad[..]
}

/// Pack an RGB888 frame into a `plane -> row -> column` bitplane framebuffer.
///
/// `dst` is the whole framebuffer viewed as 16-bit entries; it must already
/// have been `format()`ed (row address / latch / output-enable bits are
/// preserved, colour bits are overwritten). `rgb` is row-major, `COLS` wide,
/// up to `NROWS * 2` tall; a short frame leaves the remaining pixels black,
/// and pixels past the panel are ignored — the same contract as the
/// per-pixel path it replaces.
///
/// Replaces `erase()` + a full `set_pixel` sweep: every colour bit of every
/// entry is written, so no separate erase is needed.
///
/// # Panics
/// If `dst` is not exactly `NROWS * COLS * PLANES` entries, or `PLANES`
/// exceeds [`MAX_PLANES`].
pub fn pack<const NROWS: usize, const COLS: usize, const PLANES: usize>(
    dst: &mut [u16],
    rgb: &[[u8; 3]],
    tables: &Tables,
) {
    const {
        assert!(PLANES <= MAX_PLANES, "bit = 7 - plane; more than 8 planes has no source bit");
    }
    assert_eq!(dst.len(), words(NROWS, COLS, PLANES), "framebuffer length");

    let plane_stride = NROWS * COLS;
    // Per row pair: the six-channel spread of every column, split across two
    // u32s because eight planes at a 6-bit stride would not fit one.
    let mut xlo = [0u32; COLS];
    let mut xhi = [0u32; COLS];
    let mut tpad = [[0u8; 3]; COLS];
    let mut bpad = [[0u8; 3]; COLS];

    for r in 0..NROWS {
        let top = row_slice::<COLS>(rgb, r * COLS, &mut tpad);
        let bot = row_slice::<COLS>(rgb, (r + NROWS) * COLS, &mut bpad);

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
    type Fb = DmaFrameBuffer<NROWS, COLS, PLANES>;

    /// The assumption the whole packer rests on: the framebuffer is a flat
    /// array of entries laid out plane-major, with no padding. If a
    /// `hub75-framebuffer` feature (`inter-row-blank-*`, `tail-closes-latch`)
    /// were ever switched on this fails first, and loudly.
    #[test]
    fn framebuffer_is_a_flat_entry_array() {
        assert_eq!(core::mem::size_of::<Fb>(), words(NROWS, COLS, PLANES) * 2);
    }

    fn as_words(fb: &Fb) -> &[u16] {
        // SAFETY-equivalent: the struct is repr(C) over [Entry; N] with
        // Entry a repr(transparent) u16, asserted above.
        #[allow(unsafe_code)]
        unsafe {
            core::slice::from_raw_parts((fb as *const Fb).cast::<u16>(), words(NROWS, COLS, PLANES))
        }
    }

    fn as_words_mut(fb: &mut Fb) -> &mut [u16] {
        #[allow(unsafe_code)]
        unsafe {
            core::slice::from_raw_parts_mut((fb as *mut Fb).cast::<u16>(), words(NROWS, COLS, PLANES))
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
        pack::<NROWS, COLS, PLANES>(as_words_mut(&mut fb), rgb, &tables);
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
        let mut k = 0usize;
        for px in &mut frame {
            *px = [
                edges[k % edges.len()],
                edges[(k / edges.len()) % edges.len()],
                edges[(k / (edges.len() * edges.len())) % edges.len()],
            ];
            k += 1;
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

    #[test]
    fn control_bits_survive_composition() {
        let mut rng = Rng(5);
        let frame = rng.frame(PIXELS);
        let blank = Fb::new();
        let got = packed(&frame, 19);
        for (a, b) in as_words(&blank).iter().zip(as_words(&got).iter()) {
            assert_eq!(a & !COLOR_MASK, b & !COLOR_MASK, "non-colour bits changed");
        }
    }

    #[test]
    fn packing_twice_is_idempotent_and_clears_the_previous_frame() {
        let mut rng = Rng(11);
        let first = rng.frame(PIXELS);
        let second = rng.frame(PIXELS);
        let mut fb = Fb::new();
        let t = Tables::from_lut(&lut_for(24));
        pack::<NROWS, COLS, PLANES>(as_words_mut(&mut fb), &first, &t);
        pack::<NROWS, COLS, PLANES>(as_words_mut(&mut fb), &second, &t);
        assert_eq!(as_words(&fb), as_words(&reference(&second, 24)));
    }

    #[test]
    fn other_panel_geometries_pack_identically() {
        // Smaller/odd shapes, including the 8-plane full-depth case.
        fn check<const NR: usize, const C: usize, const P: usize>(seed: u64) {
            let mut rng = Rng(seed);
            let frame = rng.frame(C * NR * 2);
            let mut want = DmaFrameBuffer::<NR, C, P>::new();
            want.erase();
            for (i, px) in frame.iter().enumerate() {
                let [r, g, b] = px.map(|c| scale5(c, 9));
                want.set_pixel(Point::new((i % C) as i32, (i / C) as i32), Color::new(r, g, b));
            }
            let mut got = DmaFrameBuffer::<NR, C, P>::new();
            let n = words(NR, C, P);
            let t = Tables::from_lut(&lut_for(9));
            #[allow(unsafe_code)]
            let dst = unsafe {
                core::slice::from_raw_parts_mut((&mut got as *mut DmaFrameBuffer<NR, C, P>).cast::<u16>(), n)
            };
            pack::<NR, C, P>(dst, &frame, &t);
            #[allow(unsafe_code)]
            let (w, g) = unsafe {
                (
                    core::slice::from_raw_parts((&want as *const DmaFrameBuffer<NR, C, P>).cast::<u16>(), n),
                    core::slice::from_raw_parts((&got as *const DmaFrameBuffer<NR, C, P>).cast::<u16>(), n),
                )
            };
            assert_eq!(w, g);
        }
        check::<16, 64, 8>(1);
        check::<16, 32, 6>(2);
        check::<32, 64, 7>(3);
        check::<8, 128, 4>(4);
        check::<4, 16, 1>(5);
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

        let start = Instant::now();
        for _ in 0..N {
            pack::<NROWS, COLS, PLANES>(as_words_mut(&mut fb), std::hint::black_box(&frame), &t);
        }
        let new = start.elapsed() / N;

        std::println!(
            "per-pixel erase+set_pixel: {old:?}/frame\nrow-oriented pack:         {new:?}/frame\nspeedup: {:.2}x",
            old.as_secs_f64() / new.as_secs_f64()
        );
    }
}
