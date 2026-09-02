//! Perlin noise — bit-compatible with Pixel Blaze's `perlin` family.
//!
//! Fitted offline from the 3,320 captured oracle samples in
//! `tools/oracle/sweeps/` (perlin1d, perlin1d_fine, perlin_seed,
//! perlin_wrap4, fbm1d, fbm_arg4/5/6, ridge1d, turb1d — fw 3.67), see
//! Gitea #65 and docs/research/04-oracle-findings.md.
//!
//! The fit is conclusive: PB's family is a float32 port of Sean Barrett's
//! `stb_perlin.h`, using its **non-power-of-two** wrap variant —
//!
//! | pattern language                        | stb_perlin.h                       |
//! |-----------------------------------------|------------------------------------|
//! | `perlin(x, y, z, seed)`                 | `stb_perlin_noise3_wrap_nonpow2`   |
//! | `perlinFbm(x,y,z,lac,gain,oct)`         | `stb_perlin_fbm_noise3`            |
//! | `perlinRidge(x,y,z,lac,gain,off,oct)`   | `stb_perlin_ridge_noise3`          |
//! | `perlinTurbulence(x,y,z,lac,gain,oct)`  | `stb_perlin_turbulence_noise3`     |
//! | `setPerlinWrap(x, y, z)`                | the wrap arguments (default 256)   |
//!
//! Evidence for each piece: a per-lattice-cell polynomial fit of the fine
//! sweep is exactly degree 6 (⇒ gradient noise × quintic fade, not value or
//! simplex noise); the recovered per-corner gradients are all drawn from
//! stb's 12-direction basis; and the lattice byte at every sampled cell is
//! `randtab[randtab[floor(x) mod wrap] + seed]`, which is the *nonpow2*
//! variant's hash chain (the plain `noise3_internal` chain does not match).
//! The fbm/ridge/turbulence octave loops use the octave index as the seed,
//! exactly as stb does.
//!
//! Arithmetic is `f32`, not fixed point, because that is what PB does: a
//! fixed-point re-derivation lands ±5 raw units off, while an f32
//! evaluation with the result truncated toward zero into 16.16 reproduces
//! PB's raws exactly on 99.5% of the 3,320 samples and within one LSB
//! (1/65536) on all of them. IEEE-754 single precision is bit-deterministic
//! across our three targets (xtensa, wasm32, native) — Rust never contracts
//! to FMA — so the crate's determinism requirement still holds.
//!
//! `RANDTAB`/`GRAD_IDX` below are stb_perlin.h's tables, reproduced from
//! upstream (© 2017 Sean Barrett, dual MIT / public domain). stb stores two
//! back-to-back copies to avoid a mask; we store one and mask the index,
//! which is equivalent and saves 512 bytes of flash.

use crate::fixed::Fx;

const ONE: i64 = 1 << 16;

/// Hard cap on the octave loop so a runaway argument can't stall a frame.
/// Above ~24 the frequency has run off the end of 16.16 anyway; PB's own
/// ceiling (if any) is above the 8 octaves the sweeps cover.
const MAX_OCTAVES: i32 = 32;

#[inline]
fn fmul(a: i64, b: i64) -> i64 {
    (a * b) >> 16
}

#[inline]
fn hash(x: i32, y: i32, z: i32, seed: i32) -> u32 {
    let mut h = (x as u32)
        .wrapping_mul(0x8DA6_B343)
        .wrapping_add((y as u32).wrapping_mul(0xD816_3841))
        .wrapping_add((z as u32).wrapping_mul(0xCB1A_B31F))
        .wrapping_add((seed as u32).wrapping_mul(0x9E37_79B9));
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    h
}

/// Ken Perlin's 12-direction gradient dot product.
#[inline]
fn grad(h: u32, fx: i64, fy: i64, fz: i64) -> i64 {
    let h = h & 15;
    let u = if h < 8 { fx } else { fy };
    let v = if h < 4 {
        fy
    } else if h == 12 || h == 14 {
        fx
    } else {
        fz
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

// ---- stb_perlin.h tables (© 2017 Sean Barrett, MIT / public domain).
// `RANDTAB` is the permutation; `GRAD_IDX` maps a permuted byte straight to
// one of the 12 basis directions (stb pre-folds its `indices[hash & 63]`
// lookup into this table so the gradient bias is 5/64 or 6/64 per direction
// instead of Perlin's uneven 1/16 vs 2/16). ----

static RANDTAB: [u8; 256] = [
    23, 125, 161, 52, 103, 117, 70, 37, 247, 101, 203, 169, 124, 126, 44, 123, //
    152, 238, 145, 45, 171, 114, 253, 10, 192, 136, 4, 157, 249, 30, 35, 72, //
    175, 63, 77, 90, 181, 16, 96, 111, 133, 104, 75, 162, 93, 56, 66, 240, //
    8, 50, 84, 229, 49, 210, 173, 239, 141, 1, 87, 18, 2, 198, 143, 57, //
    225, 160, 58, 217, 168, 206, 245, 204, 199, 6, 73, 60, 20, 230, 211, 233, //
    94, 200, 88, 9, 74, 155, 33, 15, 219, 130, 226, 202, 83, 236, 42, 172, //
    165, 218, 55, 222, 46, 107, 98, 154, 109, 67, 196, 178, 127, 158, 13, 243, //
    65, 79, 166, 248, 25, 224, 115, 80, 68, 51, 184, 128, 232, 208, 151, 122, //
    26, 212, 105, 43, 179, 213, 235, 148, 146, 89, 14, 195, 28, 78, 112, 76, //
    250, 47, 24, 251, 140, 108, 186, 190, 228, 170, 183, 139, 39, 188, 244, 246, //
    132, 48, 119, 144, 180, 138, 134, 193, 82, 182, 120, 121, 86, 220, 209, 3, //
    91, 241, 149, 85, 205, 150, 113, 216, 31, 100, 41, 164, 177, 214, 153, 231, //
    38, 71, 185, 174, 97, 201, 29, 95, 7, 92, 54, 254, 191, 118, 34, 221, //
    131, 11, 163, 99, 234, 81, 227, 147, 156, 176, 17, 142, 69, 12, 110, 62, //
    27, 255, 0, 194, 59, 116, 242, 252, 19, 21, 187, 53, 207, 129, 64, 135, //
    61, 40, 167, 237, 102, 223, 106, 159, 197, 189, 215, 137, 36, 32, 22, 5, //
];

static GRAD_IDX: [u8; 256] = [
    7, 9, 5, 0, 11, 1, 6, 9, 3, 9, 11, 1, 8, 10, 4, 7, //
    8, 6, 1, 5, 3, 10, 9, 10, 0, 8, 4, 1, 5, 2, 7, 8, //
    7, 11, 9, 10, 1, 0, 4, 7, 5, 0, 11, 6, 1, 4, 2, 8, //
    8, 10, 4, 9, 9, 2, 5, 7, 9, 1, 7, 2, 2, 6, 11, 5, //
    5, 4, 6, 9, 0, 1, 1, 0, 7, 6, 9, 8, 4, 10, 3, 1, //
    2, 8, 8, 9, 10, 11, 5, 11, 11, 2, 6, 10, 3, 4, 2, 4, //
    9, 10, 3, 2, 6, 3, 6, 10, 5, 3, 4, 10, 11, 2, 9, 11, //
    1, 11, 10, 4, 9, 4, 11, 0, 4, 11, 4, 0, 0, 0, 7, 6, //
    10, 4, 1, 3, 11, 5, 3, 4, 2, 9, 1, 3, 0, 1, 8, 0, //
    6, 7, 8, 7, 0, 4, 6, 10, 8, 2, 3, 11, 11, 8, 0, 2, //
    4, 8, 3, 0, 0, 10, 6, 1, 2, 2, 4, 5, 6, 0, 1, 3, //
    11, 9, 5, 5, 9, 6, 9, 8, 3, 8, 1, 8, 9, 6, 9, 11, //
    10, 7, 5, 6, 5, 9, 1, 3, 7, 0, 2, 10, 11, 2, 6, 1, //
    3, 11, 7, 7, 2, 1, 7, 3, 0, 8, 1, 1, 5, 0, 6, 10, //
    11, 11, 0, 2, 7, 0, 10, 8, 3, 5, 7, 1, 11, 1, 0, 7, //
    9, 0, 11, 5, 10, 3, 2, 3, 5, 9, 7, 9, 8, 4, 6, 5, //
];

/// stb's 12 gradient directions (every ±1/±1/0 permutation).
const BASIS: [[f32; 3]; 12] = [
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [1.0, -1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [1.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0],
    [0.0, -1.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
];

/// Permutation lookup; the mask stands in for stb's duplicated table half.
#[inline]
fn rt(i: usize) -> usize {
    RANDTAB[i & 255] as usize
}

/// Fx → f32. Exact: the scale is a power of two and 16.16 raws below 2²⁴
/// round-trip; larger ones round exactly as PB's own conversion does.
#[inline]
fn to_f(v: Fx) -> f32 {
    v.raw() as f32 / 65536.0
}

/// f32 → Fx the way PB lands its float results back in the VM: scale by
/// 65536 and truncate **toward zero** (`as` saturates at the ends rather
/// than trapping). Rounding-to-nearest here costs ~50% of the exact matches.
#[inline]
fn from_f(v: f32) -> Fx {
    Fx::from_raw((v * 65536.0) as i32)
}

/// `fabsf` without `std` (core has no float math on our targets).
#[inline]
fn fabs(v: f32) -> f32 {
    f32::from_bits(v.to_bits() & 0x7fff_ffff)
}

/// stb's `fastfloor`: truncate, then step down if the cast rounded up.
#[inline]
fn fast_floor(a: f32) -> i32 {
    let ai = a as i32;
    if a < ai as f32 {
        ai - 1
    } else {
        ai
    }
}

/// Quintic ease 6t⁵−15t⁴+10t³, in stb's exact evaluation order (the order
/// matters: it fixes where the f32 roundings land).
#[inline]
fn ease(a: f32) -> f32 {
    ((a * 6.0 - 15.0) * a + 10.0) * a * a * a
}

#[inline]
fn lerpf(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn gradf(gi: usize, x: f32, y: f32, z: f32) -> f32 {
    let g = &BASIS[gi % 12];
    g[0] * x + g[1] * y + g[2] * z
}

/// One lattice axis: the cell index and its successor, wrapped. `w` is the
/// `setPerlinWrap` interval (2..=256; 0 means stb's default of 256).
#[inline]
fn wrap_pair(p: i32, w: i32) -> (usize, usize) {
    let w = if w <= 0 { 256 } else { w };
    let mut a = p % w;
    if a < 0 {
        a += w;
    }
    (a as usize, ((a + 1) % w) as usize)
}

/// `stb_perlin_noise3_wrap_nonpow2` — 3D gradient noise in ~[-1, 1].
fn noise3(x: f32, y: f32, z: f32, wraps: [i32; 3], seed: usize) -> f32 {
    let (px, py, pz) = (fast_floor(x), fast_floor(y), fast_floor(z));
    let (x0, x1) = wrap_pair(px, wraps[0]);
    let (y0, y1) = wrap_pair(py, wraps[1]);
    let (z0, z1) = wrap_pair(pz, wraps[2]);

    let x = x - px as f32;
    let y = y - py as f32;
    let z = z - pz as f32;
    let (u, v, w) = (ease(x), ease(y), ease(z));
    let (xm, ym, zm) = (x - 1.0, y - 1.0, z - 1.0);

    // The seed is folded in *after* the first permutation step — that is
    // what distinguishes the nonpow2 variant, and it is what PB does.
    let r0 = rt(rt(x0) + seed);
    let r1 = rt(rt(x1) + seed);
    let r00 = rt(r0 + y0);
    let r01 = rt(r0 + y1);
    let r10 = rt(r1 + y0);
    let r11 = rt(r1 + y1);
    let gi = |r: usize, zc: usize| GRAD_IDX[(r + zc) & 255] as usize;

    let n000 = gradf(gi(r00, z0), x, y, z);
    let n001 = gradf(gi(r00, z1), x, y, zm);
    let n010 = gradf(gi(r01, z0), x, ym, z);
    let n011 = gradf(gi(r01, z1), x, ym, zm);
    let n100 = gradf(gi(r10, z0), xm, y, z);
    let n101 = gradf(gi(r10, z1), xm, y, zm);
    let n110 = gradf(gi(r11, z0), xm, ym, z);
    let n111 = gradf(gi(r11, z1), xm, ym, zm);

    let n00 = lerpf(n000, n001, w);
    let n01 = lerpf(n010, n011, w);
    let n10 = lerpf(n100, n101, w);
    let n11 = lerpf(n110, n111, w);
    lerpf(lerpf(n00, n01, v), lerpf(n10, n11, v), u)
}

/// 3D gradient noise in ~[-1, 1]; lattice wraps at `wraps` (per axis).
/// `seed` selects one of 256 fields (truncated to an integer, then to a
/// byte — PB casts it to `unsigned char`).
pub fn perlin(x: Fx, y: Fx, z: Fx, seed: Fx, wraps: [i32; 3]) -> Fx {
    let s = (seed.to_int_trunc() & 255) as usize;
    from_f(noise3(to_f(x), to_f(y), to_f(z), wraps, s))
}

// ---- simplex noise (Luxel extension; smoother than perlin, no axis
// artifacts). Standard Gustavson construction in 16.16: skew to the
// simplex lattice, sum per-corner (t = r² − d²)⁴ · grad·d falloffs, scale
// to roughly [-1, 1]. The lattice does NOT wrap (setPerlinWrap doesn't
// apply — wrapping a skewed lattice isn't meaningful). ----

/// 2D skew/unskew constants in 16-frac: F2 = (√3−1)/2, G2 = (3−√3)/6.
const F2: i64 = 23994;
const G2: i64 = 13849;

/// 8 gradient directions for 2D (never a zero component pair).
#[inline]
fn grad2(h: u32, fx: i64, fy: i64) -> i64 {
    match h & 7 {
        0 => fx + fy,
        1 => -fx + fy,
        2 => fx - fy,
        3 => -fx - fy,
        4 => fx,
        5 => -fx,
        6 => fy,
        _ => -fy,
    }
}

/// The same eight directions as component pairs. [`grad2`] only ever
/// returns the dot product `g·d`; the analytic derivative also needs the
/// components of `g` themselves.
#[inline]
fn grad2_vec(h: u32) -> (i64, i64) {
    match h & 7 {
        0 => (ONE, ONE),
        1 => (-ONE, ONE),
        2 => (ONE, -ONE),
        3 => (-ONE, -ONE),
        4 => (ONE, 0),
        5 => (-ONE, 0),
        6 => (0, ONE),
        _ => (0, -ONE),
    }
}

/// [`grad`]'s 12 directions as component triples — the `u`/`v` axis pick
/// and the two sign bits, read out as a vector instead of a dot product.
#[inline]
fn grad3_vec(h: u32) -> (i64, i64, i64) {
    let h = h & 15;
    let su = if h & 1 == 0 { ONE } else { -ONE };
    let sv = if h & 2 == 0 { ONE } else { -ONE };
    let (mut gx, mut gy, mut gz) = (0i64, 0i64, 0i64);
    // u = fx for h < 8 else fy
    if h < 8 {
        gx += su;
    } else {
        gy += su;
    }
    // v = fy for h < 4, fx for h == 12/14, else fz
    if h < 4 {
        gy += sv;
    } else if h == 12 || h == 14 {
        gx += sv;
    } else {
        gz += sv;
    }
    (gx, gy, gz)
}

// ---- per-corner falloff and its analytic derivative ----
//
// A corner contributes (t² · t²) · (g·d) with t = cap − |d|².
//
// With per-corner offset d (components dᵢ), t = cap − |d|², g the corner
// gradient and n = S·Σ t⁴·(g·d), the exact partial is
//
//   ∂n/∂xᵢ = S·Σ [ t⁴·g_i − 8·t³·dᵢ·(g·d) ]
//
// over the corners with t > 0, because ∂d/∂x = 1 and ∂t/∂xᵢ = −2·dᵢ. The
// `GRAD` const parameter switches the derivative accumulation on: with it
// false every derivative line is dead code, so the plain-value path is
// *the same arithmetic in the same order* it always was and the noise value
// is bit-identical by construction rather than by convention (pinned anyway
// by `simplex_grad_value_matches_plain`).

/// Extra fraction bits the derivative terms carry (16.24 instead of
/// 16.16). t⁴ and t³ are small numbers — 0.0625 and 0.125 at most in 2D —
/// so truncating their products at 16.16 costs a whole percent of the
/// result, which a finite-difference divergence check sees immediately.
/// `SUBBITS` more bits makes that a rounding detail; the sums are still
/// nowhere near overflowing i64.
const SUBBITS: u32 = 8;

/// One 2D corner: `(t⁴·(g·d) as 16.16, ∂/∂x, ∂/∂y as 16.24)`.
#[inline]
fn corner2<const GRAD: bool>(t: i64, h: u32, dx: i64, dy: i64) -> (i64, i64, i64) {
    if t <= 0 {
        return (0, 0, 0);
    }
    let g = grad2(h, dx, dy);
    let t2 = fmul(t, t);
    let n = fmul(fmul(t2, t2), g);
    if !GRAD {
        return (n, 0, 0);
    }
    let t4 = (t2 * t2) >> (16 - SUBBITS);
    let k = 8 * ((((t2 * t) >> (16 - SUBBITS)) * g) >> 16); // 8·t³·(g·d)
    let (gx, gy) = grad2_vec(h);
    (
        n,
        ((t4 * gx) >> 16) - ((k * dx) >> 16),
        ((t4 * gy) >> 16) - ((k * dy) >> 16),
    )
}

/// One 3D corner: `(t⁴·(g·d) as 16.16, ∂/∂x, ∂/∂y, ∂/∂z as 16.24)`.
#[inline]
fn corner3<const GRAD: bool>(t: i64, h: u32, dx: i64, dy: i64, dz: i64) -> (i64, i64, i64, i64) {
    if t <= 0 {
        return (0, 0, 0, 0);
    }
    let g = grad(h, dx, dy, dz);
    let t2 = fmul(t, t);
    let n = fmul(fmul(t2, t2), g);
    if !GRAD {
        return (n, 0, 0, 0);
    }
    let t4 = (t2 * t2) >> (16 - SUBBITS);
    let k = 8 * ((((t2 * t) >> (16 - SUBBITS)) * g) >> 16);
    let (gx, gy, gz) = grad3_vec(h);
    (
        n,
        ((t4 * gx) >> 16) - ((k * dx) >> 16),
        ((t4 * gy) >> 16) - ((k * dy) >> 16),
        ((t4 * gz) >> 16) - ((k * dz) >> 16),
    )
}

fn simplex2_inner<const GRAD: bool>(x: Fx, y: Fx, seed: Fx) -> (Fx, Fx, Fx) {
    let (xr, yr) = (x.raw() as i64, y.raw() as i64);
    let s = fmul(xr + yr, F2);
    let i = ((xr + s) >> 16) as i32;
    let j = ((yr + s) >> 16) as i32;
    let t = fmul(((i as i64) + (j as i64)) << 16, G2);
    // distance from cell origin
    let x0 = xr - ((i as i64) << 16) + t;
    let y0 = yr - ((j as i64) << 16) + t;
    // which triangle of the skewed cell
    let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
    let x1 = x0 - ((i1 as i64) << 16) + G2;
    let y1 = y0 - ((j1 as i64) << 16) + G2;
    let x2 = x0 - (1 << 16) + 2 * G2;
    let y2 = y0 - (1 << 16) + 2 * G2;

    let sd = seed.raw();
    let half = ONE / 2;
    let c0 = corner2::<GRAD>(
        half - fmul(x0, x0) - fmul(y0, y0),
        hash(i, j, 0, sd),
        x0,
        y0,
    );
    let c1 = corner2::<GRAD>(
        half - fmul(x1, x1) - fmul(y1, y1),
        hash(i + i1, j + j1, 0, sd),
        x1,
        y1,
    );
    let c2 = corner2::<GRAD>(
        half - fmul(x2, x2) - fmul(y2, y2),
        hash(i + 1, j + 1, 0, sd),
        x2,
        y2,
    );
    // 70× puts the sum in ~[-1, 1] (Gustavson's constant)
    (
        Fx::from_raw((70 * (c0.0 + c1.0 + c2.0)) as i32),
        Fx::from_raw(((70 * (c0.1 + c1.1 + c2.1)) >> SUBBITS) as i32),
        Fx::from_raw(((70 * (c0.2 + c1.2 + c2.2)) >> SUBBITS) as i32),
    )
}

/// 2D simplex noise, roughly in [-1, 1]. Deterministic (seed included in
/// the corner hash, like perlin's).
pub fn simplex2(x: Fx, y: Fx, seed: Fx) -> Fx {
    simplex2_inner::<false>(x, y, seed).0
}

/// 2D simplex noise together with its **analytic** gradient:
/// `(n, ∂n/∂x, ∂n/∂y)`. `n` is bit-identical to [`simplex2`].
pub fn simplex2_grad(x: Fx, y: Fx, seed: Fx) -> (Fx, Fx, Fx) {
    simplex2_inner::<true>(x, y, seed)
}

/// 3D simplex noise, roughly in [-1, 1].
pub fn simplex3(x: Fx, y: Fx, z: Fx, seed: Fx) -> Fx {
    simplex3_inner::<false>(x, y, z, seed).0
}

/// 3D simplex noise together with its **analytic** gradient:
/// `(n, ∂n/∂x, ∂n/∂y, ∂n/∂z)`. `n` is bit-identical to [`simplex3`].
pub fn simplex3_grad(x: Fx, y: Fx, z: Fx, seed: Fx) -> (Fx, Fx, Fx, Fx) {
    simplex3_inner::<true>(x, y, z, seed)
}

/// Curl of a single 2D simplex potential: `(∂n/∂y, −∂n/∂x)`. The
/// divergence of that is `∂²n/∂x∂y − ∂²n/∂y∂x = 0` identically, so the
/// field has no sources or sinks — particles advected by it swirl along
/// closed streamlines instead of piling up in the noise's maxima, which is
/// the whole point of curl noise.
pub fn curl2(x: Fx, y: Fx, seed: Fx) -> (Fx, Fx) {
    let (_, dx, dy) = simplex2_grad(x, y, seed);
    (dy, -dx)
}

/// Curl of three 3D simplex potentials P1/P2/P3, drawn from seeds `seed`,
/// `seed + 1` and `seed + 2` (pattern units — the seed reaches the corner
/// hash as raw 16.16, so the three fields are unrelated):
/// `(∂P3/∂y − ∂P2/∂z, ∂P1/∂z − ∂P3/∂x, ∂P2/∂x − ∂P1/∂y)`.
pub fn curl3(x: Fx, y: Fx, z: Fx, seed: Fx) -> (Fx, Fx, Fx) {
    let (_, _, p1y, p1z) = simplex3_grad(x, y, z, seed);
    let (_, p2x, _, p2z) = simplex3_grad(x, y, z, seed + Fx::ONE);
    let (_, p3x, p3y, _) = simplex3_grad(x, y, z, seed + Fx::from_int(2));
    (p3y - p2z, p1z - p3x, p2x - p1y)
}

fn simplex3_inner<const GRAD: bool>(x: Fx, y: Fx, z: Fx, seed: Fx) -> (Fx, Fx, Fx, Fx) {
    const F3: i64 = ONE / 3;
    const G3: i64 = ONE / 6;
    let (xr, yr, zr) = (x.raw() as i64, y.raw() as i64, z.raw() as i64);
    let s = fmul(xr + yr + zr, F3);
    let i = ((xr + s) >> 16) as i32;
    let j = ((yr + s) >> 16) as i32;
    let k = ((zr + s) >> 16) as i32;
    let t = fmul(((i as i64) + (j as i64) + (k as i64)) << 16, G3);
    let x0 = xr - ((i as i64) << 16) + t;
    let y0 = yr - ((j as i64) << 16) + t;
    let z0 = zr - ((k as i64) << 16) + t;

    // simplex (tetrahedron) traversal order by coordinate ranking
    let (i1, j1, k1, i2, j2, k2) = if x0 >= y0 {
        if y0 >= z0 {
            (1, 0, 0, 1, 1, 0)
        } else if x0 >= z0 {
            (1, 0, 0, 1, 0, 1)
        } else {
            (0, 0, 1, 1, 0, 1)
        }
    } else if y0 < z0 {
        (0, 0, 1, 0, 1, 1)
    } else if x0 < z0 {
        (0, 1, 0, 0, 1, 1)
    } else {
        (0, 1, 0, 1, 1, 0)
    };

    let x1 = x0 - ((i1 as i64) << 16) + G3;
    let y1 = y0 - ((j1 as i64) << 16) + G3;
    let z1 = z0 - ((k1 as i64) << 16) + G3;
    let x2 = x0 - ((i2 as i64) << 16) + 2 * G3;
    let y2 = y0 - ((j2 as i64) << 16) + 2 * G3;
    let z2 = z0 - ((k2 as i64) << 16) + 2 * G3;
    let x3 = x0 - (1 << 16) + 3 * G3;
    let y3 = y0 - (1 << 16) + 3 * G3;
    let z3 = z0 - (1 << 16) + 3 * G3;

    let sd = seed.raw();
    let cap = (ONE * 6) / 10; // 0.6, the classic 3D falloff radius²
    let d = |xx: i64, yy: i64, zz: i64| cap - fmul(xx, xx) - fmul(yy, yy) - fmul(zz, zz);
    let c0 = corner3::<GRAD>(d(x0, y0, z0), hash(i, j, k, sd), x0, y0, z0);
    let c1 = corner3::<GRAD>(
        d(x1, y1, z1),
        hash(i + i1, j + j1, k + k1, sd),
        x1,
        y1,
        z1,
    );
    let c2 = corner3::<GRAD>(
        d(x2, y2, z2),
        hash(i + i2, j + j2, k + k2, sd),
        x2,
        y2,
        z2,
    );
    let c3 = corner3::<GRAD>(
        d(x3, y3, z3),
        hash(i + 1, j + 1, k + 1, sd),
        x3,
        y3,
        z3,
    );
    (
        Fx::from_raw((32 * (c0.0 + c1.0 + c2.0 + c3.0)) as i32),
        Fx::from_raw(((32 * (c0.1 + c1.1 + c2.1 + c3.1)) >> SUBBITS) as i32),
        Fx::from_raw(((32 * (c0.2 + c1.2 + c2.2 + c3.2)) >> SUBBITS) as i32),
        Fx::from_raw(((32 * (c0.3 + c1.3 + c2.3 + c3.3)) >> SUBBITS) as i32),
    )
}

/// Octave count: truncated toward zero (`(int)` in stb), non-positive means
/// an empty sum, and capped so a bad argument can't stall the render loop.
fn octaves_of(n: Fx) -> i32 {
    n.to_int_trunc().clamp(0, MAX_OCTAVES)
}

/// Fractal Brownian motion — `stb_perlin_fbm_noise3`. Each octave is a
/// *different* noise field: the octave index doubles as the seed, so the
/// layers don't share lattice lines. Not normalized; with gain 0.5 the sum
/// lands in roughly ±1.9.
pub fn fbm(x: Fx, y: Fx, z: Fx, lacunarity: Fx, gain: Fx, octaves: Fx, wraps: [i32; 3]) -> Fx {
    let (x, y, z) = (to_f(x), to_f(y), to_f(z));
    let (lac, gain) = (to_f(lacunarity), to_f(gain));
    let mut freq = 1.0f32;
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    for i in 0..octaves_of(octaves) {
        sum += noise3(x * freq, y * freq, z * freq, wraps, (i & 255) as usize) * amp;
        freq *= lac;
        amp *= gain;
    }
    from_f(sum)
}

/// Ridged multifractal — `stb_perlin_ridge_noise3`. Octaves of
/// (offset − |noise|)², each weighted by the *previous* octave's value as
/// well as the amplitude, which is what sharpens the ridges. Note stb's
/// starting amplitude is 0.5, not 1.
#[allow(clippy::too_many_arguments)] // mirrors the pattern-language signature
pub fn ridge(
    x: Fx,
    y: Fx,
    z: Fx,
    lacunarity: Fx,
    gain: Fx,
    offset: Fx,
    octaves: Fx,
    wraps: [i32; 3],
) -> Fx {
    let (x, y, z) = (to_f(x), to_f(y), to_f(z));
    let (lac, gain, offset) = (to_f(lacunarity), to_f(gain), to_f(offset));
    let mut freq = 1.0f32;
    let mut prev = 1.0f32;
    let mut amp = 0.5f32;
    let mut sum = 0.0f32;
    for i in 0..octaves_of(octaves) {
        let mut r = noise3(x * freq, y * freq, z * freq, wraps, (i & 255) as usize);
        r = offset - fabs(r);
        r = r * r;
        sum += r * amp * prev;
        prev = r;
        freq *= lac;
        amp *= gain;
    }
    from_f(sum)
}

/// Turbulence — `stb_perlin_turbulence_noise3`: octaves of |noise · amp|.
pub fn turbulence(
    x: Fx,
    y: Fx,
    z: Fx,
    lacunarity: Fx,
    gain: Fx,
    octaves: Fx,
    wraps: [i32; 3],
) -> Fx {
    let (x, y, z) = (to_f(x), to_f(y), to_f(z));
    let (lac, gain) = (to_f(lacunarity), to_f(gain));
    let mut freq = 1.0f32;
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    for i in 0..octaves_of(octaves) {
        let r = noise3(x * freq, y * freq, z * freq, wraps, (i & 255) as usize) * amp;
        sum += fabs(r);
        freq *= lac;
        amp *= gain;
    }
    from_f(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: [i32; 3] = [256; 3];

    fn fx(v: f64) -> Fx {
        Fx::from_f64(v)
    }

    #[test]
    fn lattice_points_and_range() {
        // deterministic and bounded roughly in [-1, 1]
        let mut min = 0.0f64;
        let mut max = 0.0f64;
        for i in 0..2000 {
            let p = fx(i as f64 * 0.037);
            let n = perlin(p, p * fx(0.7), p * fx(1.3), Fx::ZERO, W).to_f64();
            min = min.min(n);
            max = max.max(n);
        }
        assert!(min < -0.2 && max > 0.2, "no variation: {min}..{max}");
        assert!(min > -1.5 && max < 1.5, "out of range: {min}..{max}");
        // same input, same output
        assert_eq!(
            perlin(fx(1.5), fx(2.5), fx(3.5), fx(7.0), W),
            perlin(fx(1.5), fx(2.5), fx(3.5), fx(7.0), W)
        );
        // seed changes the field
        assert_ne!(
            perlin(fx(1.5), fx(2.5), fx(3.5), fx(7.0), W),
            perlin(fx(1.5), fx(2.5), fx(3.5), fx(8.0), W)
        );
    }

    #[test]
    fn continuity() {
        // adjacent samples stay close (smoothness sanity check)
        let mut prev = perlin(Fx::ZERO, fx(0.5), fx(0.5), Fx::ZERO, W).to_f64();
        for i in 1..200 {
            let n = perlin(fx(i as f64 * 0.01), fx(0.5), fx(0.5), Fx::ZERO, W).to_f64();
            assert!((n - prev).abs() < 0.1, "jump at {i}: {prev} → {n}");
            prev = n;
        }
    }

    #[test]
    fn wrapping() {
        let w = [4, 4, 4];
        // lattice wraps: x and x+4 sample the same field
        assert_eq!(
            perlin(fx(0.5), fx(0.5), fx(0.5), Fx::ZERO, w),
            perlin(fx(4.5), fx(0.5), fx(0.5), Fx::ZERO, w)
        );
    }

    #[test]
    fn simplex_range_and_smoothness() {
        let mut min = 0.0f64;
        let mut max = 0.0f64;
        let mut prev2 = simplex2(Fx::ZERO, Fx::ZERO, Fx::ZERO).to_f64();
        let mut prev3 = simplex3(Fx::ZERO, Fx::ZERO, Fx::ZERO, Fx::ZERO).to_f64();
        for i in 1..3000 {
            let p = fx(i as f64 * 0.01);
            let n2 = simplex2(p, p * fx(0.7), Fx::ZERO).to_f64();
            let n3 = simplex3(p, p * fx(0.7), p * fx(1.3), Fx::ZERO).to_f64();
            for n in [n2, n3] {
                min = min.min(n);
                max = max.max(n);
            }
            // continuity: 0.01 steps never jump
            assert!((n2 - prev2).abs() < 0.15, "2D jump at {i}: {prev2} → {n2}");
            assert!((n3 - prev3).abs() < 0.15, "3D jump at {i}: {prev3} → {n3}");
            prev2 = n2;
            prev3 = n3;
        }
        assert!(min < -0.3 && max > 0.3, "no variation: {min}..{max}");
        assert!(min > -1.6 && max < 1.6, "out of range: {min}..{max}");
        // deterministic; seed changes the field
        assert_eq!(
            simplex2(fx(1.5), fx(2.5), fx(7.0)),
            simplex2(fx(1.5), fx(2.5), fx(7.0))
        );
        assert_ne!(
            simplex3(fx(1.5), fx(2.5), fx(3.5), fx(7.0)),
            simplex3(fx(1.5), fx(2.5), fx(3.5), fx(8.0))
        );
    }

    /// The `*_grad` variants must return exactly the noise value the plain
    /// entry points do — existing patterns cannot be allowed to shift by an
    /// LSB. Swept over negatives, cell boundaries and several seeds.
    #[test]
    fn simplex_grad_value_matches_plain() {
        for si in 0..4 {
            let seed = fx(si as f64 * 3.0 - 4.0);
            for i in -120i32..120 {
                let x = fx(i as f64 * 0.131);
                let y = fx(i as f64 * -0.077 + 0.5);
                let z = fx(i as f64 * 0.211 - 3.0);
                assert_eq!(simplex2_grad(x, y, seed).0, simplex2(x, y, seed));
                assert_eq!(simplex3_grad(x, y, z, seed).0, simplex3(x, y, z, seed));
                // exact multiples of the lattice too (the skew/floor edges)
                let (u, v) = (Fx::from_int(i / 7), Fx::from_int(-i / 5));
                assert_eq!(simplex2_grad(u, v, seed).0, simplex2(u, v, seed));
                assert_eq!(simplex3_grad(u, v, u, seed).0, simplex3(u, v, u, seed));
            }
        }
    }

    /// The analytic gradient against a central finite difference of the
    /// fixed-point noise itself. The FD is not a clean reference from
    /// either side: its own O(h²) truncation error blows up as h grows,
    /// while shrinking h amplifies the 16.16 quantization of `simplex2`
    /// by 1/2h. h = 1/32 sits at the crossover — measured max |error| over
    /// this sweep is **0.085** (2D) and **0.168** (3D) against gradients
    /// that peak at **6.39**, i.e. 1.3 % / 2.6 % of full scale. The error
    /// is FD noise, not derivative error: over h = 1/8 … 1/64 it falls
    /// as h² (0.94 → 0.25 → 0.085 in 2D) exactly as a correct analytic
    /// gradient predicts, and only turns back up at 1/128 where
    /// quantization takes over.
    #[test]
    fn simplex_grad_matches_finite_difference() {
        let h = 1.0 / 32.0;
        let hx = fx(h);
        let mut worst2 = 0.0f64;
        let mut worst3 = 0.0f64;
        let mut peak = 0.0f64;
        for i in -60i32..60 {
            for j in -20i32..20 {
                let (x, y) = (fx(i as f64 * 0.113), fx(j as f64 * 0.171 - 1.0));
                let z = fx(i as f64 * 0.037 + 0.25);
                let seed = fx(j as f64);

                let (_, gx, gy) = simplex2_grad(x, y, seed);
                let fdx = (simplex2(x + hx, y, seed).to_f64()
                    - simplex2(x - hx, y, seed).to_f64())
                    / (2.0 * h);
                let fdy = (simplex2(x, y + hx, seed).to_f64()
                    - simplex2(x, y - hx, seed).to_f64())
                    / (2.0 * h);
                worst2 = worst2.max((gx.to_f64() - fdx).abs());
                worst2 = worst2.max((gy.to_f64() - fdy).abs());
                peak = peak.max(gx.to_f64().abs()).max(gy.to_f64().abs());

                let (_, ax, ay, az) = simplex3_grad(x, y, z, seed);
                for (g, fd) in [
                    (ax, simplex3(x + hx, y, z, seed).to_f64() - simplex3(x - hx, y, z, seed).to_f64()),
                    (ay, simplex3(x, y + hx, z, seed).to_f64() - simplex3(x, y - hx, z, seed).to_f64()),
                    (az, simplex3(x, y, z + hx, seed).to_f64() - simplex3(x, y, z - hx, seed).to_f64()),
                ] {
                    worst3 = worst3.max((g.to_f64() - fd / (2.0 * h)).abs());
                }
            }
        }
        assert!(peak > 1.0, "gradient never got large: {peak}");
        assert!(worst2 < 0.12, "2D gradient vs FD: {worst2}");
        assert!(worst3 < 0.22, "3D gradient vs FD: {worst3}");
    }

    /// Curl noise's defining property: no sources or sinks. The analytic
    /// divergence ∂²n/∂x∂y − ∂²n/∂y∂x is identically zero, so what a
    /// finite difference measures here is purely its own error: max |div|
    /// over this sweep is **0.095** at h = 1/64, against components that
    /// reach 6.39. It too falls as h² (3.20 at 1/8, 0.95 at 1/16, 0.28 at
    /// 1/32, 0.095 at 1/64) and floors at 0.077 by 1/128.
    #[test]
    fn curl2_is_divergence_free() {
        let h = 1.0 / 64.0;
        let hx = fx(h);
        let mut worst = 0.0f64;
        let mut peak = 0.0f64;
        for i in -80i32..80 {
            for j in -30i32..30 {
                let (x, y) = (fx(i as f64 * 0.091), fx(j as f64 * 0.137 - 2.0));
                let seed = fx((i % 5) as f64);
                let dudx = (curl2(x + hx, y, seed).0.to_f64() - curl2(x - hx, y, seed).0.to_f64())
                    / (2.0 * h);
                let dvdy = (curl2(x, y + hx, seed).1.to_f64() - curl2(x, y - hx, seed).1.to_f64())
                    / (2.0 * h);
                worst = worst.max((dudx + dvdy).abs());
                let (u, v) = curl2(x, y, seed);
                peak = peak.max(u.to_f64().abs()).max(v.to_f64().abs());
            }
        }
        assert!(peak > 1.0, "curl2 never got large: {peak}");
        assert!(worst < 0.15, "curl2 divergence: {worst}");
    }

    /// Determinism, seed sensitivity and rough magnitude, mirroring
    /// `simplex_range_and_smoothness`.
    #[test]
    fn curl_determinism_and_range() {
        assert_eq!(curl2(fx(1.5), fx(2.5), fx(7.0)), curl2(fx(1.5), fx(2.5), fx(7.0)));
        assert_ne!(curl2(fx(1.5), fx(2.5), fx(7.0)), curl2(fx(1.5), fx(2.5), fx(8.0)));
        assert_eq!(
            curl3(fx(1.5), fx(2.5), fx(3.5), fx(7.0)),
            curl3(fx(1.5), fx(2.5), fx(3.5), fx(7.0))
        );
        assert_ne!(
            curl3(fx(1.5), fx(2.5), fx(3.5), fx(7.0)),
            curl3(fx(1.5), fx(2.5), fx(3.5), fx(8.0))
        );
        // curl3's three potentials must be genuinely different fields: an
        // off-by-one in the seed offsets would make components collapse
        let (a, b, c) = curl3(fx(0.37), fx(1.21), fx(2.03), Fx::ZERO);
        assert!(a != b && b != c, "curl3 components collapsed: {a:?} {b:?} {c:?}");

        // magnitude: the gradient of ~[-1,1] noise over a ~1-unit lattice
        let mut max2 = 0.0f64;
        let mut max3 = 0.0f64;
        let mut prev = curl2(Fx::ZERO, Fx::ZERO, Fx::ZERO).0.to_f64();
        for i in 1..3000 {
            let p = fx(i as f64 * 0.01);
            let (u, v) = curl2(p, p * fx(0.7), Fx::ZERO);
            max2 = max2.max(u.to_f64().abs()).max(v.to_f64().abs());
            let (a, b, c) = curl3(p, p * fx(0.7), p * fx(1.3), Fx::ZERO);
            for w in [a, b, c] {
                max3 = max3.max(w.to_f64().abs());
            }
            // continuity: no jumps at 0.01 steps
            let u = u.to_f64();
            assert!((u - prev).abs() < 1.0, "curl2 jump at {i}: {prev} → {u}");
            prev = u;
        }
        assert!((1.0..12.0).contains(&max2), "curl2 magnitude {max2}");
        assert!((1.0..12.0).contains(&max3), "curl3 magnitude {max3}");
    }

    #[test]
    fn fractal_variants_run() {
        let l = fx(2.0);
        let g = fx(0.5);
        let o = fx(3.0);
        assert_ne!(fbm(fx(1.3), fx(2.1), Fx::ZERO, l, g, o, W), Fx::ZERO);
        let r = ridge(fx(1.3), fx(2.1), Fx::ZERO, l, g, Fx::ONE, o, W);
        assert!(r.to_f64() > 0.0);
        let t = turbulence(fx(1.3), fx(2.1), Fx::ZERO, l, g, o, W);
        assert!(t.to_f64() >= 0.0);
    }

    // ---- Pixel Blaze oracle fixtures (Gitea #65) ----
    //
    // Raw 16.16 outputs captured from a real Pixel Blaze on fw 3.67 and
    // stored in tools/oracle/sweeps/*.json; the arrays below are evenly
    // spaced subsamples of those files, so a failure here means we drifted
    // off the hardware, not off a model. Tolerance is ONE raw LSB
    // (1/65536): PB evaluates in f32 as well, but the last-bit rounding of
    // a few intermediate products differs from ours on ~0.5% of samples.
    //
    // `Y03`/`Z07`/`X037` are the 16.15 literal raws the PB compiler emits
    // for the source constants 0.3/0.7/0.37 — the sweep patterns passed
    // literals, so the fixtures must feed exactly those, not
    // `Fx::from_f64(0.3)` (which rounds up and shifts every sample).
    const Y03: Fx = Fx::from_raw(19660);
    const Z07: Fx = Fx::from_raw(45874);
    const X037: Fx = Fx::from_raw(24248);
    /// setPerlinWrap's default (vm.rs seeds `perlin_wrap` with this).
    const W256: [i32; 3] = [256; 3];

    fn assert_matches_pb(label: &str, rows: &[(i32, i32)], f: impl Fn(Fx) -> Fx) {
        let mut worst = 0i32;
        let mut worst_at = 0i32;
        for &(x, pb) in rows {
            let e = (f(Fx::from_raw(x)).raw() - pb).abs();
            if e > worst {
                worst = e;
                worst_at = x;
            }
        }
        assert!(
            worst <= 1,
            "{label}: off Pixel Blaze by {worst} raw units at x={} (max 1 allowed)",
            worst_at as f64 / 65536.0
        );
    }

    const PB_PERLIN1D: &[(i32, i32)] = &[
        (-131072, 21170),
        (-118589, 27096),
        (-105449, 24531),
        (-92966, 9636),
        (-80483, -9877),
        (-67343, -22576),
        (-54860, -30389),
        (-42377, -39704),
        (-29237, -45261),
        (-16754, -43865),
        (-4271, -39942),
        (8870, -36246),
        (21353, -28421),
        (33836, -13905),
        (46976, 4414),
        (59459, 17759),
        (71942, 26228),
        (84425, 29354),
        (97565, 21778),
        (110048, 8557),
        (122531, -327),
        (146880, 4712),
        (196937, 22485),
        (246995, 2576),
        (299687, 47462),
        (349745, 9399),
        (399803, 27133),
        (452495, 3999),
        (502552, -20196),
        (552610, -11755),
        (605302, -15034),
        (655360, 12305),
    ];
    const PB_PERLIN_FINE: &[(i32, i32)] = &[
        (0, -38844),
        (2792, -38143),
        (5749, -37314),
        (8541, -36371),
        (11333, -35202),
        (14290, -33655),
        (17082, -31859),
        (19874, -29708),
        (22831, -27033),
        (25623, -24142),
        (28415, -20922),
        (31372, -17201),
        (34164, -13456),
        (37121, -9325),
        (39913, -5353),
        (42705, -1400),
        (45662, 2669),
        (48454, 6315),
        (51246, 9700),
        (54203, 12940),
        (56995, 15648),
        (59787, 18022),
        (62744, 20233),
        (65536, 22146),
    ];
    const PB_PERLIN_WRAP4: &[(i32, i32)] = &[
        (0, -38844),
        (25130, -24677),
        (51739, 10266),
        (76869, 28510),
        (102000, 17256),
        (128608, -1250),
        (153739, 11591),
        (178869, 30701),
        (205478, 15115),
        (230608, -25458),
        (255738, -40248),
        (282347, -29431),
        (307477, 2225),
        (334086, 26228),
        (359216, 22240),
        (384346, -216),
        (410955, 6456),
        (436085, 29743),
        (461216, 20952),
        (487824, -17475),
        (512955, -40601),
        (538085, -33938),
        (564694, -4651),
        (589824, 22146),
    ];
    const PB_PERLIN_SEED: &[(i32, i32)] = &[
        (0, -2428),
        (85625, 1528),
        (171250, 7877),
        (256875, -11410),
        (342500, -25609),
        (428125, -5009),
        (513750, 21125),
        (599374, 9735),
        (684999, 5800),
        (770624, 498),
        (856249, 2430),
        (941874, 16823),
        (1027499, 21472),
        (1113124, 19385),
        (1198749, -15001),
        (1284374, 18030),
        (5296, -2428),
        (13902, -2428),
        (22507, -2428),
        (31113, -2428),
        (39719, -2428),
        (48325, -2428),
        (56930, -2428),
        (65536, 1528),
    ];
    const PB_FBM1D: &[(i32, i32)] = &[
        (-131072, -24564),
        (-114318, -34698),
        (-96579, -10013),
        (-79826, -6616),
        (-63072, -4537),
        (-45333, -9672),
        (-28580, -6348),
        (-11826, -453),
        (5913, -18905),
        (22667, -26703),
        (39420, -11918),
        (57159, -2751),
        (73913, -23295),
        (91652, -15946),
        (108405, -17631),
        (125159, -20514),
        (142898, -17343),
        (159652, -26939),
        (176405, -20413),
        (194144, -24876),
        (210898, -15638),
        (227651, -12314),
        (245390, -19614),
        (262144, -30659),
    ];
    const PB_FBM_LAC: &[(i32, i32)] = &[
        (16384, 7827),
        (41675, 20385),
        (66965, -1710),
        (92256, -16338),
        (117547, -28008),
        (142838, -11047),
        (168128, 6135),
        (193419, -15262),
        (216181, -12221),
        (241472, -4006),
        (266762, -588),
        (292053, 6407),
        (317344, 15583),
        (342635, 15043),
        (367925, -2329),
        (393216, -12326),
    ];
    const PB_FBM_GAIN: &[(i32, i32)] = &[
        (0, -2428),
        (8797, -6673),
        (17594, -12548),
        (26390, -20051),
        (35187, -29183),
        (43984, -39944),
        (52781, -52335),
        (61577, -66352),
        (69495, -80364),
        (78291, -97476),
        (87088, -116219),
        (95885, -136591),
        (104682, -158592),
        (113478, -182219),
        (122275, -207478),
        (131072, -234366),
    ];
    const PB_FBM_OCT: &[(i32, i32)] = &[
        (65536, -2428),
        (96376, -2428),
        (127217, -2428),
        (158057, -15209),
        (188898, -15209),
        (219738, -26509),
        (250579, -26509),
        (281419, -28852),
        (308405, -28852),
        (339245, -29571),
        (370086, -29571),
        (400926, -29616),
        (431767, -29616),
        (462607, -29846),
        (493448, -29846),
        (524288, -29595),
    ];
    const PB_RIDGE1D: &[(i32, i32)] = &[
        (-131072, 31852),
        (-114318, 27046),
        (-96579, 35068),
        (-79826, 38919),
        (-63072, 40255),
        (-45333, 44359),
        (-28580, 35392),
        (-11826, 40652),
        (5913, 37537),
        (22667, 35908),
        (39420, 42924),
        (57159, 34583),
        (73913, 30022),
        (91652, 31403),
        (108405, 23067),
        (125159, 23622),
        (142898, 18938),
        (159652, 20380),
        (176405, 30175),
        (194144, 28580),
        (210898, 37716),
        (227651, 42634),
        (245390, 38012),
        (262144, 28604),
    ];
    const PB_TURB1D: &[(i32, i32)] = &[
        (-131072, 24564),
        (-114318, 34698),
        (-96579, 20615),
        (-79826, 17195),
        (-63072, 16485),
        (-45333, 9672),
        (-28580, 21356),
        (-11826, 13109),
        (5913, 18905),
        (22667, 26703),
        (39420, 11918),
        (57159, 19669),
        (73913, 23295),
        (91652, 20777),
        (108405, 30552),
        (125159, 29014),
        (142898, 35706),
        (159652, 33935),
        (176405, 21876),
        (194144, 24876),
        (210898, 16729),
        (227651, 12314),
        (245390, 19614),
        (262144, 30659),
    ];

    #[test]
    fn matches_pixelblaze_perlin() {
        // perlin(x, 0.3, 0.7, 5)
        assert_matches_pb("perlin1d", PB_PERLIN1D, |x| {
            perlin(x, Y03, Z07, fx(5.0), W256)
        });
        assert_matches_pb("perlin1d_fine", PB_PERLIN_FINE, |x| {
            perlin(x, Y03, Z07, fx(5.0), W256)
        });
        // setPerlinWrap(4, 4, 4): the field repeats every 4 lattice cells
        assert_matches_pb("perlin_wrap4", PB_PERLIN_WRAP4, |x| {
            perlin(x, Y03, Z07, fx(5.0), [4; 3])
        });
        // perlin(0.37, 0.3, 0.7, seed) — the seed truncates to an integer
        assert_matches_pb("perlin_seed", PB_PERLIN_SEED, |s| {
            perlin(X037, Y03, Z07, s, W256)
        });
    }

    #[test]
    fn matches_pixelblaze_fractals() {
        let (l, g, o) = (fx(2.0), fx(0.5), fx(3.0));
        assert_matches_pb("fbm1d", PB_FBM1D, |x| fbm(x, Y03, Z07, l, g, o, W256));
        assert_matches_pb("ridge1d", PB_RIDGE1D, |x| {
            ridge(x, Y03, Z07, l, g, Fx::ONE, o, W256)
        });
        assert_matches_pb("turb1d", PB_TURB1D, |x| {
            turbulence(x, Y03, Z07, l, g, o, W256)
        });
        // each tail argument swept on its own at a fixed sample point
        assert_matches_pb("fbm lacunarity", PB_FBM_LAC, |lac| {
            fbm(X037, Y03, Z07, lac, g, o, W256)
        });
        assert_matches_pb("fbm gain", PB_FBM_GAIN, |gain| {
            fbm(X037, Y03, Z07, l, gain, o, W256)
        });
        assert_matches_pb("fbm octaves", PB_FBM_OCT, |oct| {
            fbm(X037, Y03, Z07, l, g, oct, W256)
        });
    }

    #[test]
    fn octave_count_truncates_and_caps() {
        let (l, g) = (fx(2.0), fx(0.5));
        // fractional octave counts truncate, like stb/PB's `(int)` cast
        assert_eq!(
            fbm(X037, Y03, Z07, l, g, fx(3.99), W256),
            fbm(X037, Y03, Z07, l, g, fx(3.0), W256)
        );
        // no octaves = empty sum
        assert_eq!(fbm(X037, Y03, Z07, l, g, Fx::ZERO, W256), Fx::ZERO);
        assert_eq!(fbm(X037, Y03, Z07, l, g, fx(-4.0), W256), Fx::ZERO);
        // absurd counts are capped rather than stalling a frame
        assert_eq!(
            fbm(X037, Y03, Z07, l, g, fx(4000.0), W256),
            fbm(X037, Y03, Z07, l, g, fx(MAX_OCTAVES as f64), W256)
        );
    }

    #[test]
    fn wraps_seamlessly() {
        // setPerlinWrap(4,4,4) makes the lattice repeat with period 4 on
        // every axis, including across negative coordinates.
        for i in 0..40 {
            let x = fx(i as f64 * 0.1);
            let a = perlin(x, Y03, Z07, fx(5.0), [4; 3]);
            let b = perlin(x + fx(4.0), Y03, Z07, fx(5.0), [4; 3]);
            let c = perlin(x - fx(8.0), Y03, Z07, fx(5.0), [4; 3]);
            assert_eq!(a, b, "no +4 wrap at {}", x.to_f64());
            assert_eq!(a, c, "no -8 wrap at {}", x.to_f64());
        }
        // a non-power-of-two interval wraps too (PB documents 2..256)
        for i in 0..40 {
            let x = fx(i as f64 * 0.1);
            assert_eq!(
                perlin(x, Y03, Z07, fx(5.0), [5, 5, 5]),
                perlin(x + fx(5.0), Y03, Z07, fx(5.0), [5, 5, 5])
            );
        }
    }
}

