//! The vector (PIE) row-pair packer for the ESP32-S3 (Gitea #855, the ring
//! driver's step 1 — docs/hub75-ring-design.md §5).
//!
//! [`crate::pack`] costs ~146 cycles per pixel: six table lookups and ten
//! shift/OR pairs to spread a pixel pair's channel bits, then seven
//! read-modify-write words. On a ring driver that re-packs the whole frame
//! every pass, that packer IS the refresh ceiling. This module does the
//! per-plane work sixteen pixels at a time on the S3's 128-bit PIE unit and
//! leaves the packer's contract untouched: given the same formatted words,
//! the same RGB frame and the same brightness LUT it writes the same bytes as
//! [`crate::pack`] — a host test asserts that against
//! a portable model of the lane arithmetic, and the firmware's `packbench`
//! image asserts the assembly against `pack` on metal.
//!
//! # Why the deinterleave stays scalar
//!
//! The obvious plan — `ee.vunzip.8` the RGB888 row into planar channels —
//! does not exist as a short instruction sequence. Every PIE data-movement
//! instruction (`ee.vzip.*`, `ee.vunzip.*`, `ee.slci.2q`/`ee.srci.2q`, the
//! lane-constant multiply) maps a lane index affinely with a slope that is a
//! power of two; the RGB stride is 3 bytes per pixel and the output stride is
//! 2 bytes (one bus word). No composition of such maps turns a stride of 3
//! into a stride of 2 for more than a couple of pixels, so a SIMD gather
//! costs an op per pixel per channel anyway. The scalar core does it instead
//! — hand-scheduled Xtensa ([`gather_words`]), 9.7 cycles per pixel for both
//! rows on the Seengreat, where the compiler's best for the same loop was 15
//! — and the brightness is applied afterwards on the vector unit as an exact
//! fixed-point multiply ([`crate::LutMode::Scale`], 2.1 cycles/px); only an
//! arbitrary table ([`crate::LutMode::Table`]) goes through a lookup in the
//! gather, at ~48 cycles/px, which nothing in the firmware asks for. The
//! frame format is the lever if the gather ever needs to go: a planar or
//! RGBX frame would make this step a handful of `vld`s.
//!
//! Measured (docs/boards.md "The PIE packer", 2026-09-27): 22.4 cycles/px
//! cache-hot, 39.9 cache-cold, against 115.3 / 140.5 for [`crate::pack`] —
//! 5.1× / 3.5×; the seven planes are 10.1 of that.
//!
//! # The kernel
//!
//! The gather writes three **pair pads**, one `u16` per driver column each:
//!
//! ```text
//! rr[x] = R(bottom) << 8 | R(top)      gg, bb likewise
//! ```
//!
//! so one 128-bit register holds eight columns of one channel for BOTH rows
//! of the pair, and for plane `p` (source bit `k = 7 - p`) the two bits that
//! belong in the bus word sit at lane bits `k` and `8 + k`. Per plane, per
//! eight columns:
//!
//! ```text
//! t  = (rr & (0x0101 << k)) * 2 >> k      R1 -> bit 1, R2 -> bit 9   (ee.vmul.u16, SAR = k)
//!    | (gg & (0x0101 << k)) * 4 >> k      G1 -> bit 2, G2 -> bit 10
//!    | (bb & (0x0101 << k)) * 8 >> k      B1 -> bit 3, B2 -> bit 11
//! c  = t * (0x108 << k) >> k              = t * 0x108: low byte << 8, high byte << 3
//!                                          -> bits 9..11 = R1 G1 B1, 12..14 = R2 G2 B2
//! w  = w ^ ((w ^ c) & COLOR_MASK)         colour bits from c, everything else from w
//! ```
//!
//! `ee.vmul.u16` computes the 32-bit product and shifts it right by SAR
//! before truncating (TRM §1.8.128), so the shifts lose nothing and are
//! logical, which `ee.vsr.32` (arithmetic) is not. The last multiply folds
//! the two halves into the 6-bit colour field in one instruction: the low
//! byte's bits 1..3 land at 9..11 and the high byte's at 12..14 with no
//! carries between them; the junk it leaves in bits 4..6 is masked by the
//! merge. Six constants, one accumulator and one temporary fill the eight
//! `q` registers exactly, so the pads are re-read once per plane (three
//! aligned loads per eight columns) instead of being held across planes.
//!
//! # Contract with the scheduler
//!
//! Nothing saves the `q` registers or `SAR` on a context switch, so the
//! kernel masks interrupts on its own core for the duration of one plane of
//! one row pair (`cols / 8` iterations of a 17-instruction loop: ~5 µs at 256
//! columns) and restores `PS`/`SAR` on exit. Each CPU has its own vector
//! unit, so two cores may pack concurrently with no shared state beyond the
//! read-only constants — the reentrancy the ring driver's work queue needs
//! (design §6).
//!
//! # Preconditions (checked, never assumed)
//!
//! [`pack_pie`] returns `false` and writes nothing when `cols` is not a
//! multiple of 8, when `dst` is not 16-byte aligned, or when the plane stride
//! is not a multiple of 8 words — the caller falls back to [`crate::pack`].
//! PIE loads and stores FORCE 16-byte alignment (TRM §1.5.3) rather than
//! faulting, so an unchecked misaligned buffer would pack silently into the
//! wrong words.

use alloc::vec;
use alloc::vec::Vec;

use crate::{Geometry, LutMode, Tables, COLOR_MASK, MAX_PLANES, SCALE_SHIFT};

/// Columns one loop iteration covers: eight 16-bit lanes.
pub const LANES: usize = 8;

/// A 16-byte-aligned line, so a `Vec<Line>` is a valid PIE load target.
#[derive(Clone, Copy, Default)]
#[repr(C, align(16))]
struct Line([u8; 16]);

/// The three pair pads the kernel reads (`rr`, `gg`, `bb`), each `cols`
/// `u16`s, 16-byte aligned, allocated once and reused for every row pair.
///
/// 6 bytes per column — 1.5 KiB for a 256-wide chain. A pad set may be
/// WIDER than the geometry it is used with, like [`crate::Scratch`].
pub struct PairPads {
    lines: Vec<Line>,
    cols: usize,
}

impl PairPads {
    /// Pads for up to `cols` columns (rounded up to a whole line).
    #[must_use]
    pub fn new(cols: usize) -> Self {
        let per_pad = pad_lines(cols);
        Self { lines: vec![Line::default(); per_pad * 3], cols: per_pad * LANES }
    }

    /// Pads for [`Geometry::cols`].
    #[must_use]
    pub fn for_geometry(g: Geometry) -> Self {
        Self::new(g.cols)
    }

    /// The widest geometry these pads can pack.
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Bytes held.
    #[must_use]
    pub fn bytes(&self) -> usize {
        pads_bytes(self.cols)
    }

    /// The three pads as `u16` slices, `self.cols()` wide each.
    fn split(&mut self) -> (&mut [u16], &mut [u16], &mut [u16]) {
        let per_pad = self.lines.len() / 3;
        let (r, rest) = self.lines.split_at_mut(per_pad);
        let (g, b) = rest.split_at_mut(per_pad);
        (as_u16(r), as_u16(g), as_u16(b))
    }
}

/// Lines one pad of `cols` columns takes.
const fn pad_lines(cols: usize) -> usize {
    cols.div_ceil(LANES)
}

/// Bytes a [`PairPads`] for `cols` columns holds — three `u16` pads, rounded
/// up to whole 16-byte lines. Spelled once for the boot-cost prediction, like
/// [`crate::scratch_bytes`].
#[must_use]
pub const fn pads_bytes(cols: usize) -> usize {
    pad_lines(cols) * 16 * 3
}

/// View a run of lines as `u16`s. Safe: `Line` is `repr(C)` over bytes with
/// 16-byte alignment, `u16` needs 2, and the length is exact.
fn as_u16(lines: &mut [Line]) -> &mut [u16] {
    // `bytemuck`-free: rebuild through a raw pointer with the invariants above.
    let n = lines.len() * (16 / 2);
    #[allow(unsafe_code)]
    // SAFETY: `lines` is a contiguous, 16-byte-aligned run of `len * 16`
    // initialised bytes; reinterpreting them as `len * 8` `u16`s is in bounds
    // and aligned, and the borrow is exclusive for the returned lifetime.
    unsafe {
        core::slice::from_raw_parts_mut(lines.as_mut_ptr().cast::<u16>(), n)
    }
}

/// The scalar prologue: gather row pair `r` of `rgb` into the pair pads.
/// The frame is already in driver order (Gitea #948 — the engine writes a
/// tiled chain's frame the way the driver clocks it), so both rows are
/// plain slices. With [`LutMode::Table`] the byte
/// LUT is applied here (six dependent loads per pixel — measured at ~48
/// cycles/px on the S3, which is why the firmware never uses this mode);
/// with `Identity` or `Scale` the bytes are copied and the scale, if any,
/// is applied by [`scale_pads`] on the vector unit.
///
/// A short frame reads black past its end — the same contract as
/// [`crate::pack`].
fn gather_pair(
    rgb: &[[u8; 3]],
    r: usize,
    rows: usize,
    cols: usize,
    tables: &Tables,
    pads: &mut PairPads,
) {
    let (rr, gg, bb) = pads.split();
    let (rr, gg, bb) = (&mut rr[..cols], &mut gg[..cols], &mut bb[..cols]);
    let black = [0u8; 3];
    let table = matches!(tables.mode(), LutMode::Table);
    let blut = tables.lut();
    // The plain case: both rows are whole slices of the frame, no LUT.
    if !table {
        let top = r * cols;
        let bot = (r + rows) * cols;
        if let (Some(t), Some(b)) = (rgb.get(top..top + cols), rgb.get(bot..bot + cols)) {
            if !gather_words(t, b, rr, gg, bb) {
                gather_bytes(t, b, rr, gg, bb);
            }
            return;
        }
    }
    let pair = |t: [u8; 3], b: [u8; 3]| -> (u16, u16, u16) {
        let ch = |i: usize| {
            if table {
                (u16::from(blut[usize::from(b[i])]) << 8) | u16::from(blut[usize::from(t[i])])
            } else {
                (u16::from(b[i]) << 8) | u16::from(t[i])
            }
        };
        (ch(0), ch(1), ch(2))
    };
    let top = r * cols;
    let bot = (r + rows) * cols;
    for x in 0..cols {
        let t = rgb.get(top + x).copied().unwrap_or(black);
        let b = rgb.get(bot + x).copied().unwrap_or(black);
        let (pr, pg, pb) = pair(t, b);
        rr[x] = pr;
        gg[x] = pg;
        bb[x] = pb;
    }
}

/// The plain gather a byte at a time: the fallback for rows the word form
/// cannot take, and the yardstick the bench times it against.
pub fn gather_bytes(top: &[[u8; 3]], bot: &[[u8; 3]], rr: &mut [u16], gg: &mut [u16], bb: &mut [u16]) {
    for ((((r_, g_), b_), t), b) in rr.iter_mut().zip(gg.iter_mut()).zip(bb.iter_mut()).zip(top).zip(bot) {
        *r_ = (u16::from(b[0]) << 8) | u16::from(t[0]);
        *g_ = (u16::from(b[1]) << 8) | u16::from(t[1]);
        *b_ = (u16::from(b[2]) << 8) | u16::from(t[2]);
    }
}

/// The plain gather on 32-bit words: four columns per step — three words
/// of each row in, six words of pairs out — so the Xtensa does 6 loads and
/// 6 stores per 4 columns instead of 24 byte loads and 12 halfword stores,
/// and the byte extraction is ALU work with no load-use stalls. Needs both
/// rows 4-byte aligned and `cols` a multiple of 4 (the pads always are —
/// 16-byte lines); returns `false` (nothing written) otherwise.
#[allow(unsafe_code)]
pub fn gather_words(top: &[[u8; 3]], bot: &[[u8; 3]], rr: &mut [u16], gg: &mut [u16], bb: &mut [u16]) -> bool {
    let cols = rr.len();
    let (t, b) = (top.as_flattened(), bot.as_flattened());
    if !cols.is_multiple_of(4)
        || !(t.as_ptr() as usize).is_multiple_of(4)
        || !(b.as_ptr() as usize).is_multiple_of(4)
        || !(rr.as_ptr() as usize).is_multiple_of(4)
        || !(gg.as_ptr() as usize).is_multiple_of(4)
        || !(bb.as_ptr() as usize).is_multiple_of(4)
    {
        return false;
    }
    // SAFETY: alignment checked above; lengths are exact multiples (3 bytes
    // per column, 4 columns per 3 words; 2 columns per pad word); `u32` and
    // `u16` have no invalid bit patterns; the borrows are the callers'.
    let (tw, bw, rw, gw, bw2) = unsafe {
        (
            core::slice::from_raw_parts(t.as_ptr().cast::<u32>(), cols * 3 / 4),
            core::slice::from_raw_parts(b.as_ptr().cast::<u32>(), cols * 3 / 4),
            core::slice::from_raw_parts_mut(rr.as_mut_ptr().cast::<u32>(), cols / 2),
            core::slice::from_raw_parts_mut(gg.as_mut_ptr().cast::<u32>(), cols / 2),
            core::slice::from_raw_parts_mut(bb.as_mut_ptr().cast::<u32>(), cols / 2),
        )
    };
    // Little-endian: column x's bytes are 3x, 3x+1, 3x+2 of the row, so
    // the three words of four columns hold R0 G0 B0 R1 | G1 B1 R2 G2 |
    // B2 R3 G3 B3. Each pad word (two columns: `b1 t1 b0 t0`) is four
    // bytes pulled out with `(w >> s) & 0xff` — one `extui` on Xtensa — and
    // shifted into place; a mask against a non-contiguous constant would be
    // a literal load and an AND each.
    #[cfg(all(feature = "pie", target_arch = "xtensa"))]
    {
        gather_words_asm(tw, bw, rw, gw, bw2);
        return true;
    }
    #[cfg(not(all(feature = "pie", target_arch = "xtensa")))]
    for ((((tw, bw), rw), gw), bw2) in
        tw.chunks_exact(3).zip(bw.chunks_exact(3)).zip(rw.chunks_exact_mut(2)).zip(gw.chunks_exact_mut(2)).zip(bw2.chunks_exact_mut(2))
    {
        // bytes of the three words: [R0 G0 B0 R1] [G1 B1 R2 G2] [B2 R3 G3 B3]
        let [tr0, tg0, tb0, tr1] = tw[0].to_le_bytes();
        let [tg1, tb1, tr2, tg2] = tw[1].to_le_bytes();
        let [tb2, tr3, tg3, tb3] = tw[2].to_le_bytes();
        let [br0, bg0, bb0, br1] = bw[0].to_le_bytes();
        let [bg1, bb1, br2, bg2] = bw[1].to_le_bytes();
        let [bb2, br3, bg3, bb3] = bw[2].to_le_bytes();
        // pad word = column 2i+1 in the high half, column 2i in the low: t b t b
        rw[0] = u32::from_le_bytes([tr0, br0, tr1, br1]);
        rw[1] = u32::from_le_bytes([tr2, br2, tr3, br3]);
        gw[0] = u32::from_le_bytes([tg0, bg0, tg1, bg1]);
        gw[1] = u32::from_le_bytes([tg2, bg2, tg3, bg3]);
        bw2[0] = u32::from_le_bytes([tb0, bb0, tb1, bb1]);
        bw2[1] = u32::from_le_bytes([tb2, bb2, tb3, bb3]);
    }
    true
}

/// [`gather_words`] as hand-scheduled Xtensa: the instruction count the
/// machine needs (6 loads, 24 `extui`, 36 shift/OR, 6 stores per four
/// columns) rather than the ~110 the compiler makes of the Rust form. Same
/// preconditions (the caller checked them); the device bench checks the
/// output word for word against the scalar packer.
#[cfg(all(feature = "pie", target_arch = "xtensa"))]
#[allow(unsafe_code)]
fn gather_words_asm(tw: &[u32], bw: &[u32], rw: &mut [u32], gw: &mut [u32], bw2: &mut [u32]) {
    let n = rw.len() / 2;
    debug_assert!(tw.len() >= 3 * n && bw.len() >= 3 * n && gw.len() >= 2 * n && bw2.len() >= 2 * n);
    // SAFETY: `n` iterations read 3 words of each source and write 2 words
    // of each pad, all within the slices the caller sized; no memory
    // outside them is touched and no register state survives the block.
    unsafe {
        core::arch::asm!(
            "loopnez {n}, 2f",
            "l32i {t0}, {t}, 0",
            "l32i {t1}, {t}, 4",
            "l32i {t2}, {t}, 8",
            "l32i {b0}, {b}, 0",
            "l32i {b1}, {b}, 4",
            "l32i {b2}, {b}, 8",
            "addi {t}, {t}, 12",
            "addi {b}, {b}, 12",
            "extui {x}, {t0}, 0, 8",
            "extui {y}, {b0}, 0, 8",
            "slli {y}, {y}, 8",
            "or {x}, {x}, {y}",
            "extui {y}, {t0}, 24, 8",
            "slli {y}, {y}, 16",
            "or {x}, {x}, {y}",
            "extui {y}, {b0}, 24, 8",
            "slli {y}, {y}, 24",
            "or {x}, {x}, {y}",
            "s32i {x}, {r}, 0",
            "extui {x}, {t1}, 16, 8",
            "extui {y}, {b1}, 16, 8",
            "slli {y}, {y}, 8",
            "or {x}, {x}, {y}",
            "extui {y}, {t2}, 8, 8",
            "slli {y}, {y}, 16",
            "or {x}, {x}, {y}",
            "extui {y}, {b2}, 8, 8",
            "slli {y}, {y}, 24",
            "or {x}, {x}, {y}",
            "s32i {x}, {r}, 4",
            "extui {x}, {t0}, 8, 8",
            "extui {y}, {b0}, 8, 8",
            "slli {y}, {y}, 8",
            "or {x}, {x}, {y}",
            "extui {y}, {t1}, 0, 8",
            "slli {y}, {y}, 16",
            "or {x}, {x}, {y}",
            "extui {y}, {b1}, 0, 8",
            "slli {y}, {y}, 24",
            "or {x}, {x}, {y}",
            "s32i {x}, {g}, 0",
            "extui {x}, {t1}, 24, 8",
            "extui {y}, {b1}, 24, 8",
            "slli {y}, {y}, 8",
            "or {x}, {x}, {y}",
            "extui {y}, {t2}, 16, 8",
            "slli {y}, {y}, 16",
            "or {x}, {x}, {y}",
            "extui {y}, {b2}, 16, 8",
            "slli {y}, {y}, 24",
            "or {x}, {x}, {y}",
            "s32i {x}, {g}, 4",
            "extui {x}, {t0}, 16, 8",
            "extui {y}, {b0}, 16, 8",
            "slli {y}, {y}, 8",
            "or {x}, {x}, {y}",
            "extui {y}, {t1}, 8, 8",
            "slli {y}, {y}, 16",
            "or {x}, {x}, {y}",
            "extui {y}, {b1}, 8, 8",
            "slli {y}, {y}, 24",
            "or {x}, {x}, {y}",
            "s32i {x}, {p}, 0",
            "extui {x}, {t2}, 0, 8",
            "extui {y}, {b2}, 0, 8",
            "slli {y}, {y}, 8",
            "or {x}, {x}, {y}",
            "extui {y}, {t2}, 24, 8",
            "slli {y}, {y}, 16",
            "or {x}, {x}, {y}",
            "extui {y}, {b2}, 24, 8",
            "slli {y}, {y}, 24",
            "or {x}, {x}, {y}",
            "s32i {x}, {p}, 4",
            "addi {r}, {r}, 8",
            "addi {g}, {g}, 8",
            "addi {p}, {p}, 8",
            "2:",
            n = in(reg) n,
            t = inout(reg) tw.as_ptr() => _,
            b = inout(reg) bw.as_ptr() => _,
            r = inout(reg) rw.as_mut_ptr() => _,
            g = inout(reg) gw.as_mut_ptr() => _,
            p = inout(reg) bw2.as_mut_ptr() => _,
            t0 = out(reg) _, t1 = out(reg) _, t2 = out(reg) _,
            b0 = out(reg) _, b1 = out(reg) _, b2 = out(reg) _,
            x = out(reg) _, y = out(reg) _,
            options(nostack),
        );
    }
}

/// Apply a [`LutMode::Scale`] multiplier to every byte of the pair pads:
/// `(c * mul) >> SCALE_SHIFT` per byte, exact `scale5`. Each 16-bit lane
/// holds two bytes, so each half is masked out, multiplied on its own (the
/// 32-bit product shifted by SAR = 13 — the high byte's product carries
/// its junk below bit 8, masked away) and the halves are OR-ed back.
#[cfg_attr(all(feature = "pie", target_arch = "xtensa"), allow(dead_code))]
pub fn scale_pads_model(pad: &mut [u16], mul: u16) {
    let m = u32::from(mul);
    for w in pad.iter_mut() {
        let lo = ((u32::from(*w & 0x00ff) * m) >> SCALE_SHIFT) as u16;
        let hi = (((u32::from(*w & 0xff00) * m) >> SCALE_SHIFT) as u16) & 0xff00;
        *w = lo | hi;
    }
}

/// [`scale_pads_model`] on the PIE unit: `pad` is 16-byte aligned and a
/// multiple of [`LANES`] long (the caller checked).
#[cfg(all(feature = "pie", target_arch = "xtensa"))]
#[allow(unsafe_code)]
fn scale_pads_asm(pad: &mut [u16], mul: u16) {
    let consts: [u16; 3] = [0x00ff, 0xff00, mul];
    let chunks = pad.len() / LANES;
    let shift = SCALE_SHIFT;
    // SAFETY: as for `pack_plane_asm` — every access is inside `pad`, PS
    // and SAR are restored, and the q registers belong to no one.
    unsafe {
        core::arch::asm!(
            "rsil {ps}, 15",
            "rsr.sar {sar}",
            "ssr {k}",
            "ee.vldbc.16 q0, {c}",
            "addi {c}, {c}, 2",
            "ee.vldbc.16 q1, {c}",
            "addi {c}, {c}, 2",
            "ee.vldbc.16 q2, {c}",
            "loopnez {n}, 2f",
            "ee.vld.128.ip q3, {p}, 0",
            "ee.andq q4, q3, q0",
            "ee.andq q3, q3, q1",
            "ee.vmul.u16 q4, q4, q2",        // low byte scaled
            "ee.vmul.u16 q3, q3, q2",        // high byte scaled, junk below bit 8
            "ee.andq q3, q3, q1",
            "ee.orq q3, q3, q4",
            "ee.vst.128.ip q3, {p}, 16",
            "2:",
            "wsr.sar {sar}",
            "wsr.ps {ps}",
            "rsync",
            ps = out(reg) _,
            sar = out(reg) _,
            k = in(reg) shift,
            c = inout(reg) consts.as_ptr() => _,
            n = in(reg) chunks,
            p = inout(reg) pad.as_mut_ptr() => _,
            options(nostack),
        );
    }
}

#[cfg(all(feature = "pie", target_arch = "xtensa"))]
use scale_pads_asm as scale_pads;
#[cfg(not(all(feature = "pie", target_arch = "xtensa")))]
use scale_pads_model as scale_pads;

/// The scale pass on one 16-byte-aligned pad of `LANES`-multiple length
/// (the vector unit where there is one) — exposed for the bench.
pub fn scale_pad(pad: &mut [u16], mul: u16) {
    assert!(pad.len().is_multiple_of(LANES) && (pad.as_ptr() as usize).is_multiple_of(16));
    scale_pads(pad, mul);
}

/// The three pads as mutable slices, `cols` wide — exposed for the bench.
pub fn pads_mut(pads: &mut PairPads, cols: usize) -> (&mut [u16], &mut [u16], &mut [u16]) {
    let (r, g, b) = pads.split();
    (&mut r[..cols], &mut g[..cols], &mut b[..cols])
}

/// The per-plane constants the kernel broadcasts into `q0..q5`, in load
/// order. Spelled once so the model and the assembly cannot disagree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
struct PlaneConsts {
    /// `0x0101 << k` — the plane's source bit in both bytes.
    mask: u16,
    /// Channel shift multipliers: R to bit 1, G to bit 2, B to bit 3.
    mul: [u16; 3],
    /// `0x108 << k` — folds the two 3-bit halves into bits 9..14 (with SAR = k).
    fold: u16,
    /// [`COLOR_MASK`].
    color: u16,
}

impl PlaneConsts {
    const fn for_bit(k: u32) -> Self {
        Self { mask: 0x0101 << k, mul: [2, 4, 8], fold: 0x108 << k, color: COLOR_MASK }
    }
}

/// Portable model of one plane of the vector kernel over `cols` columns —
/// exactly the lane arithmetic the assembly performs, in plain integer
/// operations, so the host can prove the ALGORITHM byte-identical to
/// [`crate::pack`]. Also the implementation on targets without PIE.
// On the S3 build the assembly replaces it; it stays compiled as the
// specification the host tests run.
#[cfg_attr(all(feature = "pie", target_arch = "xtensa"), allow(dead_code))]
fn pack_plane_model(dst: &mut [u16], k: u32, rr: &[u16], gg: &[u16], bb: &[u16]) {
    let c = PlaneConsts::for_bit(k);
    // (x * m) >> SAR on the 32-bit product, low 16 bits kept: ee.vmul.u16.
    let vmul = |x: u16, m: u16| ((u32::from(x) * u32::from(m)) >> k) as u16;
    for (((w, &r), &g), &b) in dst.iter_mut().zip(rr).zip(gg).zip(bb) {
        let t = vmul(r & c.mask, c.mul[0]) | vmul(g & c.mask, c.mul[1]) | vmul(b & c.mask, c.mul[2]);
        let col = vmul(t, c.fold);
        *w ^= (*w ^ col) & c.color;
    }
}

/// One plane over `cols` columns on the PIE unit. `dst`, `rr`, `gg`, `bb`
/// are 16-byte aligned and `cols` is a multiple of [`LANES`] (the caller
/// checked). Interrupts are masked on this core for the call.
#[cfg(all(feature = "pie", target_arch = "xtensa"))]
#[allow(unsafe_code)]
fn pack_plane_asm(dst: &mut [u16], k: u32, rr: &[u16], gg: &[u16], bb: &[u16]) {
    let consts = PlaneConsts::for_bit(k);
    let chunks = dst.len() / LANES;
    debug_assert!(dst.len().is_multiple_of(LANES) && rr.len() >= dst.len() && gg.len() >= dst.len() && bb.len() >= dst.len());
    debug_assert!((dst.as_ptr() as usize).is_multiple_of(16) && (rr.as_ptr() as usize).is_multiple_of(16));
    // SAFETY: every load and store is within `chunks * 16` bytes of a
    // 16-byte-aligned slice the caller owns (`dst` exclusively); the loop
    // touches nothing else. PS and SAR are restored; the q registers belong
    // to no one (see the module docs).
    unsafe {
        core::arch::asm!(
            // ---- prologue: mask interrupts, save SAR, load the constants
            "rsil {ps}, 15",
            "rsr.sar {sar}",
            "ssr {k}",                       // SAR = k
            "ee.vldbc.16 q0, {c}",           // mask
            "addi {c}, {c}, 2",
            "ee.vldbc.16 q1, {c}",           // mul R
            "addi {c}, {c}, 2",
            "ee.vldbc.16 q2, {c}",           // mul G
            "addi {c}, {c}, 2",
            "ee.vldbc.16 q3, {c}",           // mul B
            "addi {c}, {c}, 2",
            "ee.vldbc.16 q4, {c}",           // fold
            "addi {c}, {c}, 2",
            "ee.vldbc.16 q5, {c}",           // colour mask
            // ---- eight columns per iteration (zero-overhead loop)
            "loopnez {n}, 2f",
            "ee.vld.128.ip q7, {r}, 16",
            "ee.vld.128.ip q6, {g}, 16",
            "ee.andq q7, q7, q0",
            "ee.vmul.u16 q7, q7, q1",        // R pair -> bits 1 / 9
            "ee.andq q6, q6, q0",
            "ee.vmul.u16 q6, q6, q2",        // G pair -> bits 2 / 10
            "ee.orq q7, q7, q6",
            "ee.vld.128.ip q6, {b}, 16",
            "ee.andq q6, q6, q0",
            "ee.vmul.u16 q6, q6, q3",        // B pair -> bits 3 / 11
            "ee.orq q7, q7, q6",
            "ee.vmul.u16 q7, q7, q4",        // fold: bits 9..14
            "ee.vld.128.ip q6, {d}, 0",      // the formatted words
            "ee.xorq q7, q6, q7",
            "ee.andq q7, q7, q5",
            "ee.xorq q7, q6, q7",            // w ^ ((w ^ c) & COLOR)
            "ee.vst.128.ip q7, {d}, 16",
            "2:",
            // ---- epilogue: restore SAR and PS
            "wsr.sar {sar}",
            "wsr.ps {ps}",
            "rsync",
            ps = out(reg) _,
            sar = out(reg) _,
            k = in(reg) k,
            c = inout(reg) &consts as *const PlaneConsts => _,
            n = in(reg) chunks,
            r = inout(reg) rr.as_ptr() => _,
            g = inout(reg) gg.as_ptr() => _,
            b = inout(reg) bb.as_ptr() => _,
            d = inout(reg) dst.as_mut_ptr() => _,
            options(nostack),
        );
    }
}

#[cfg(all(feature = "pie", target_arch = "xtensa"))]
use pack_plane_asm as pack_plane;
#[cfg(not(all(feature = "pie", target_arch = "xtensa")))]
use pack_plane_model as pack_plane;

/// True when the assembly kernel is compiled in (the `pie` feature on an
/// Xtensa target); false where [`pack_pie`] runs the portable model.
#[must_use]
pub const fn has_vector_unit() -> bool {
    cfg!(all(feature = "pie", target_arch = "xtensa"))
}

/// Whether `dst` and `g` satisfy the kernel's alignment and width rules.
#[must_use]
pub fn fits(dst: &[u16], g: Geometry) -> bool {
    g.cols.is_multiple_of(LANES)
        && (dst.as_ptr() as usize).is_multiple_of(16)
        && g.plane_words().is_multiple_of(LANES)
}

/// Pack one row pair's planes: `dst_planes` holds `planes` rows of `cols`
/// words, `stride` words apart (a whole-frame layout uses
/// [`Geometry::plane_words`]; a ring slot uses `cols`). The pads hold the
/// gathered pair. `k(p) = 7 - p`.
fn pack_pair_planes(dst: &mut [u16], stride: usize, cols: usize, planes: usize, tables: &Tables, pads: &mut PairPads) {
    let (rr, gg, bb) = pads.split();
    if let LutMode::Scale(m) = tables.mode() {
        scale_pads(&mut rr[..cols], m);
        scale_pads(&mut gg[..cols], m);
        scale_pads(&mut bb[..cols], m);
    }
    for p in 0..planes {
        let base = p * stride;
        let k = (7 - p) as u32;
        pack_plane(&mut dst[base..base + cols], k, &rr[..cols], &gg[..cols], &bb[..cols]);
    }
}

/// [`crate::pack`] on the vector unit.
///
/// Same contract, same output: `dst` is the whole formatted framebuffer,
/// `rgb` the frame in driver order, and `tables` the brightness tables (this reads their byte LUT). Returns
/// `false` — having written NOTHING — when [`fits`] fails or the pads are too
/// narrow; the caller then packs the scalar way.
///
/// # Panics
/// On [`crate::pack`]'s own conditions: `dst` not `g.words()` entries, more
/// than [`MAX_PLANES`] planes.
pub fn pack_pie(
    dst: &mut [u16],
    g: Geometry,
    rgb: &[[u8; 3]],
    tables: &Tables,
    pads: &mut PairPads,
) -> bool {
    assert!(g.planes <= MAX_PLANES, "bit = 7 - plane; more than 8 planes has no source bit");
    assert_eq!(dst.len(), g.words(), "framebuffer length");
    if !fits(dst, g) || pads.cols() < g.cols {
        return false;
    }
    let (rows, cols) = (g.rows, g.cols);
    let stride = g.plane_words();
    for r in 0..rows {
        gather_pair(rgb, r, rows, cols, tables, pads);
        pack_pair_planes(&mut dst[r * cols..], stride, cols, g.planes, tables, pads);
    }
    true
}

/// The ring-driver form (design §9): pack ONE row pair straight into a slot
/// of `planes * cols` words laid out plane after plane. `dst_slot` must be
/// exactly that long and 16-byte aligned, `cols` a multiple of [`LANES`].
/// Returns `false` (nothing written) otherwise.
pub fn pack_row_pair(
    dst_slot: &mut [u16],
    g: Geometry,
    r: usize,
    rgb: &[[u8; 3]],
    tables: &Tables,
    pads: &mut PairPads,
) -> bool {
    assert!(g.planes <= MAX_PLANES, "bit = 7 - plane; more than 8 planes has no source bit");
    assert_eq!(dst_slot.len(), g.planes * g.cols, "slot length");
    if !g.cols.is_multiple_of(LANES)
        || !(dst_slot.as_ptr() as usize).is_multiple_of(16)
        || pads.cols() < g.cols
        || r >= g.rows
    {
        return false;
    }
    gather_pair(rgb, r, g.rows, g.cols, tables, pads);
    pack_pair_planes(dst_slot, g.cols, g.cols, g.planes, tables, pads);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{format, pack, Control, Scratch};
    use std::vec::Vec;
    use std::{format, vec};

    const ROWS: usize = 32;
    const COLS: usize = 64;
    const PLANES: usize = 7;
    const G: Geometry = Geometry::new(ROWS, COLS, PLANES);

    /// A 16-byte-aligned framebuffer, as the firmware allocates one.
    fn aligned_words(n: usize) -> Vec<Line> {
        vec![Line::default(); n.div_ceil(8)]
    }

    fn words_of(v: &mut Vec<Line>, n: usize) -> &mut [u16] {
        &mut as_u16(v.as_mut_slice())[..n]
    }

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

    fn scale5_lut(b5: u8) -> [u8; 256] {
        let mut lut = [0u8; 256];
        for (c, v) in lut.iter_mut().enumerate() {
            *v = if b5 >= 31 { c as u8 } else { ((c as u16 * u16::from(b5)) / 31) as u8 };
        }
        lut
    }

    fn gamma_lut() -> [u8; 256] {
        let mut lut = [0u8; 256];
        for (c, v) in lut.iter_mut().enumerate() {
            // an arbitrary non-linear table: the kernel must not care
            *v = ((c * c) / 255) as u8 ^ 0x5a;
        }
        lut
    }

    fn assert_pie_matches_pack(g: Geometry, rgb: &[[u8; 3]], blut: &[u8; 256], what: &str) {
        assert_pie_matches_pack_with(g, rgb, &Tables::from_lut(blut), what);
    }

    fn assert_pie_matches_pack_with(g: Geometry, rgb: &[[u8; 3]], tables: &Tables, what: &str) {
        let tables = tables.clone();
        let c = Control::default();
        let mut want_v = aligned_words(g.words());
        let mut got_v = aligned_words(g.words());
        let want = words_of(&mut want_v, g.words());
        let got = words_of(&mut got_v, g.words());
        format(want, g, c);
        format(got, g, c);
        let mut scratch = Scratch::for_geometry(g);
        pack(want, g, rgb, &tables, &mut scratch);
        let mut pads = PairPads::for_geometry(g);
        assert!(pack_pie(got, g, rgb, &tables, &mut pads), "{what}: preconditions");
        if want != got {
            let i = want.iter().zip(got.iter()).position(|(a, b)| a != b).unwrap();
            let plane = i / g.plane_words();
            let row = (i / g.cols) % g.blocks();
            let col = i % g.cols;
            panic!(
                "{what}: word {i} (plane {plane} row {row} col {col}) want {:#06x} got {:#06x}",
                want[i], got[i]
            );
        }
    }

    #[test]
    fn model_is_byte_identical_to_pack_at_every_brightness() {
        let mut rng = Rng(0x2026_0927_0855);
        for trial in 0..4 {
            let frame = rng.frame(G.pixels());
            for b5 in 0..=31u8 {
                assert_pie_matches_pack(G, &frame, &scale5_lut(b5), &format!("random frame {trial} b5={b5}"));
            }
        }
    }

    /// The firmware's path: `build_scale5`, so the LUT is the identity or
    /// an exact multiply on the vector unit, never a table lookup.
    #[test]
    fn scale5_mode_is_byte_identical_and_exact() {
        for b5 in 0..=31u8 {
            let mut t = Tables::zeroed();
            t.build_scale5(b5);
            assert_eq!(t.mode(), if b5 >= 31 { crate::LutMode::Identity } else { crate::LutMode::Scale(crate::scale5_mul(b5).unwrap()) });
            // the multiplier reproduces scale5 for every channel value
            if let crate::LutMode::Scale(m) = t.mode() {
                for c in 0..=255u32 {
                    assert_eq!(((c * u32::from(m)) >> SCALE_SHIFT) as u8, crate::scale5(c as u8, b5), "b5 {b5} c {c}");
                }
            }
        }
        let mut rng = Rng(0x5ca1e);
        let frame = rng.frame(G.pixels());
        for b5 in [0u8, 1, 3, 14, 17, 30, 31] {
            let mut t = Tables::zeroed();
            t.build_scale5(b5);
            assert_pie_matches_pack_with(G, &frame, &t, &format!("scale5 b5={b5}"));
        }
        // and through the model's scale pass directly
        let mut pad: Vec<u16> = (0..64u16).map(|i| (i * 977) ^ 0x3c5a).collect();
        let want: Vec<u16> = pad.iter().map(|&w| (u16::from(crate::scale5((w >> 8) as u8, 9)) << 8) | u16::from(crate::scale5(w as u8, 9))).collect();
        scale_pads_model(&mut pad, crate::scale5_mul(9).unwrap());
        assert_eq!(pad, want);
    }

    #[test]
    fn edge_colours_and_arbitrary_luts_match() {
        let edges = [0u8, 1, 2, 127, 128, 129, 254, 255];
        let mut frame = vec![[0u8; 3]; G.pixels()];
        for (k, px) in frame.iter_mut().enumerate() {
            *px = [edges[k % 8], edges[(k / 8) % 8], edges[(k / 64) % 8]];
        }
        assert_pie_matches_pack(G, &frame, &scale5_lut(31), "edges full");
        assert_pie_matches_pack(G, &frame, &scale5_lut(7), "edges b5=7");
        assert_pie_matches_pack(G, &frame, &gamma_lut(), "edges gamma");
        for v in edges {
            let solid = vec![[v; 3]; G.pixels()];
            assert_pie_matches_pack(G, &solid, &scale5_lut(31), "solid");
        }
    }

    #[test]
    fn short_frames_trailing_block_and_other_geometries_match() {
        let mut rng = Rng(9);
        let short = rng.frame(G.pixels() / 3);
        assert_pie_matches_pack(G, &short, &scale5_lut(31), "short frame");
        let long = rng.frame(G.pixels() + 1000);
        assert_pie_matches_pack(G, &long, &scale5_lut(20), "long frame");
        let trail = G.with_trail(true);
        let frame = rng.frame(G.pixels());
        assert_pie_matches_pack(trail, &frame, &scale5_lut(31), "trailing block");
        for (rows, cols, planes) in [(16usize, 32usize, 7usize), (32, 128, 7), (32, 256, 7), (8, 8, 8), (32, 64, 4)] {
            let g = Geometry::new(rows, cols, planes);
            let frame = rng.frame(g.pixels());
            assert_pie_matches_pack(g, &frame, &scale5_lut(13), &format!("{rows}x{cols}x{planes}"));
        }
    }

    #[test]
    fn refuses_what_it_cannot_pack() {
        let tables = Tables::from_lut(&scale5_lut(31));
        let g = Geometry::new(4, 12, 7); // cols not a multiple of 8
        let mut v = aligned_words(g.words());
        let words = words_of(&mut v, g.words());
        let frame = vec![[1u8; 3]; g.pixels()];
        let mut pads = PairPads::for_geometry(g);
        assert!(!pack_pie(words, g, &frame, &tables, &mut pads));
        assert!(words.iter().all(|&w| w == 0), "nothing written on refusal");
        // Misaligned destination.
        let mut v = aligned_words(G.words() + 8);
        let all = as_u16(v.as_mut_slice());
        let words = &mut all[1..1 + G.words()];
        let mut pads = PairPads::for_geometry(G);
        let frame = vec![[1u8; 3]; G.pixels()];
        assert!(!pack_pie(words, G, &frame, &tables, &mut pads));
        // Pads too narrow.
        let mut v = aligned_words(G.words());
        let words = words_of(&mut v, G.words());
        let mut narrow = PairPads::new(8);
        assert!(!pack_pie(words, G, &frame, &tables, &mut narrow));
    }

    #[test]
    fn a_ring_slot_equals_the_frame_rows() {
        let mut rng = Rng(3);
        let frame = rng.frame(G.pixels());
        let tables = Tables::from_lut(&scale5_lut(17));
        let mut fb_v = aligned_words(G.words());
        let fb = words_of(&mut fb_v, G.words());
        format(fb, G, Control::default());
        let mut pads = PairPads::for_geometry(G);
        assert!(pack_pie(fb, G, &frame, &tables, &mut pads));
        for r in [0usize, 7, 31] {
            let mut slot_v = aligned_words(G.planes * G.cols);
            let slot = words_of(&mut slot_v, G.planes * G.cols);
            // The slot carries the same control template as the frame rows.
            for p in 0..G.planes {
                let base = p * G.plane_words() + r * G.cols;
                slot[p * G.cols..(p + 1) * G.cols]
                    .copy_from_slice(&fb[base..base + G.cols]);
                for w in &mut slot[p * G.cols..(p + 1) * G.cols] {
                    *w &= !COLOR_MASK;
                }
            }
            assert!(pack_row_pair(slot, G, r, &frame, &tables, &mut pads));
            for p in 0..G.planes {
                let base = p * G.plane_words() + r * G.cols;
                assert_eq!(&slot[p * G.cols..(p + 1) * G.cols], &fb[base..base + G.cols], "row {r} plane {p}");
            }
        }
        assert_eq!(pads_bytes(64), PairPads::new(64).bytes());
        assert_eq!(pads_bytes(256), 3 * 512);
    }
}
