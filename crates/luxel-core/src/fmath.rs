//! Deterministic fixed-point transcendentals for the Luxel VM.
//!
//! Everything here is integer-only on 16-frac raws, so results are
//! bit-identical on ESP32, wasm32, and native — that determinism is a
//! project requirement. Accuracy targets LED work (errors well under one
//! 8-bit color step), not scientific computing.
//!
//! On the device targets the arithmetic is deliberately kept in **32-bit**
//! operands. Xtensa
//! (ESP32 / ESP32-S3) has a native 32x32→64 widening multiply (`mull` +
//! `mulsh`/`muluh`) but no 64-bit ALU: every `i64 * i64`, `i64 / const` or
//! `i128` intermediate turns into a ROM libcall (`__divdi3`, `__udivdi3`,
//! `__multi3`) costing hundreds of cycles. The narrowings below are all
//! bit-exact rewrites — each carries the bound that proves it, and
//! `mod tests` sweeps every one of them against a verbatim copy of the
//! original 64-bit form (Gitea #312).
//!
//! Two of them (`div_shift16`, `sq16`) trade one wide machine instruction
//! for 32-bit work, which is a *loss* on hosts that do have a 64-bit ALU —
//! including wasm32, which the web playground runs on. Those keep both
//! forms and pick at compile time on [`NARROW_WORD`]; both are compiled
//! everywhere and the tests assert `narrow == wide` directly, so a host
//! test run still proves the device path bit-exact. `isqrt48` is one
//! Newton-based form on every target since Gitea #938 (a floor root is a
//! unique integer, so there is nothing to pick between); the two loop forms
//! it replaced stay as test references. The radian reduction in `sin` is
//! likewise a single multiply-and-correct on every target (`rad_to_turns`).
//!
//! Pixel Blaze's exact algorithms for these functions are not public. All
//! of them have been differential-tested against real hardware via dense
//! sweeps (docs/research/04-oracle-findings.md): ours are equal or closer
//! to true math everywhere measured; remaining diffs are PB-side
//! approximation error and its documented seam/endpoint bugs — deliberate
//! divergences, not open questions.

use crate::fixed::Fx;

// Raw 16.16 constants (value * 65536, rounded).
const PI_RAW: i32 = 205_887; // π
const PI2_RAW: i32 = 411_775; // 2π
const HALF_PI_RAW: i32 = 102_944; // π/2
const LOG2E_RAW: i32 = 94_548; // 1/ln 2
const LN2_RAW: i32 = 45_426; // ln 2

/// 16-frac multiply on i64 raws (truncating like the VM's `*`).
///
/// Kept for any caller that genuinely needs a >32-bit result; nothing in
/// this module does any more, because on Xtensa a true 64x64 multiply is
/// several times the cost of the widening 32x32 one in [`fmul32`].
#[allow(dead_code)]
#[inline]
fn fmul(a: i64, b: i64) -> i64 {
    (a * b) >> 16
}

/// 16-frac multiply on i32 raws, truncating like the VM's `*`.
///
/// Bit-identical to `fmul(a as i64, b as i64) as i32` for *every* input
/// pair (same product, same arithmetic shift, same truncation), but the
/// operands are 32-bit so Xtensa forms the 64-bit product with one `mull` +
/// `mulsh` and extracts bits 16..48 with one `src` — no libcall.
#[inline]
fn fmul32(a: i32, b: i32) -> i32 {
    (((a as i64) * (b as i64)) >> 16) as i32
}

/// True on targets with **no 64-bit ALU**, where an `i64` divide or a wide
/// `i64` multiply is a ROM / compiler-rt libcall (`__divdi3`, `__udivdi3`,
/// `__multi3`) rather than a machine instruction: Xtensa (ESP32,
/// ESP32-S3) and riscv32 (ESP32-C3 `riscv32imc`, ESP32-C6 `riscv32imac`) —
/// every target in `firmware/board-target.sh`.
///
/// Everywhere else — x86-64 and aarch64 natively, and wasm32 whose
/// `i64.div_u` / `i64.mul` the host engine lowers to one native
/// instruction — the wide form is a single op and the 32-bit loops below
/// would be pure loss. Measured 2026-09-06 with `luxel bench`: forcing the
/// narrow path on the host cost −31 % on `dist`-heavy patterns.
///
/// This is a `cfg!()` *value*, not a `#[cfg]` on the definitions: both
/// implementations are compiled and type-checked on every target (and both
/// are swept against each other by the tests below), while the branch
/// const-folds away in release so only the selected one is emitted.
const NARROW_WORD: bool = cfg!(any(target_arch = "xtensa", target_arch = "riscv32"));

/// `floor(num · 2^16 / d)` for `0 < d <= 2^31` and `num <= d`.
/// Result is `<= 2^16`. See [`NARROW_WORD`] for why there are two of these.
#[inline]
fn div_shift16(num: u32, d: u32) -> u32 {
    if NARROW_WORD {
        div_shift16_narrow(num, d)
    } else {
        div_shift16_wide(num, d)
    }
}

/// Wide form: `num << 16` is up to 48 bits, so this is one 64-bit divide.
/// The straight translation of the original code, kept for every host that
/// has such an instruction.
#[inline]
fn div_shift16_wide(num: u32, d: u32) -> u32 {
    debug_assert!(d != 0 && num <= d && d <= 1u32 << 31);
    (((num as u64) << 16) / d as u64) as u32
}

/// Narrow form: the hardware 32-bit divider (`quou` on Xtensa, `divu` on
/// RISC-V), never a 64-bit libcall and never the 17-step restoring loop
/// this was until Gitea #938 (~90 instructions and as many branches,
/// ~240 cycles of every `sin()` call on the S3).
///
/// `d < 2^16` — every normalised coordinate ratio `atan2` sees — is one
/// divide: `num <= d` keeps `num << 16` inside u32. Larger divisors are
/// normalised so the top bit of `d` is bit 31 and estimated from its top
/// 16 bits: with `dn = dh·2^16 + dl`, `nn·2^16/dn <= nn/dh`, and
/// `nn/dh − nn·2^16/dn = nn·dl/(dh·dn) < 2·nn/dn <= 2`, so the estimate
/// is never low and at most 3 high after both floors. The correction walks
/// it down against the exact 64-bit product — at most three steps, usually
/// none.
#[inline]
fn div_shift16_narrow(num: u32, d: u32) -> u32 {
    debug_assert!(d != 0 && num <= d && d <= 1u32 << 31);
    if d < 1 << 16 {
        return (num << 16) / d;
    }
    let s = d.leading_zeros(); // 0..=15
    let dn = d << s;
    let nn = num << s; // num <= d, so nn <= dn < 2^32
    let mut q = nn / (dn >> 16); // <= 2^32 / 2^15 = 2^17
    let target = (nn as u64) << 16;
    let mut prod = (q as u64) * (dn as u64);
    while prod > target {
        q -= 1;
        prod -= dn as u64;
    }
    q
}

/// `floor(x · 2^16 / 2π_raw) mod 2^16`: the phase of `x` radians (a raw
/// 16.16 word) in 16-frac turns. Bit-identical to the pre-#938 two-step
/// form — `r = x.mod_floor(2π)`, then `floor(r·2^16/2π)` — because
/// `x = k·2π + r` makes `floor(x·2^16/2π) = k·2^16 + floor(r·2^16/2π)`,
/// whose low 16 bits are the second term. One signed widening multiply
/// and a 32-bit fix-up replace a hardware remainder plus the 17-step
/// restoring division of `div_shift16_narrow` (Gitea #938).
///
/// With `M = floor(2^48/2π_raw)` (30 bits) the high word of `x·M` is
/// `floor(x·M/2^32)`, and `x·2^16/2π − x·M/2^32 = x·ε/2^32` for some
/// `ε ∈ [0, 1)`, so `|x| <= 2^31` bounds the error below 0.5 and the
/// candidate is within one of the true floor. Its remainder
/// `x·2^16 − q·2π_raw` lies in `(−2π_raw, 2·2π_raw)`, which fits i32, so
/// the wrapped 32-bit difference IS the remainder and one compare each way
/// settles the floor. Swept exhaustively over a period and randomly over
/// the whole word against the i64 form in `tests`.
#[inline]
fn rad_to_turns(x: i32) -> i32 {
    const M: i64 = (1i64 << 48) / PI2_RAW as i64;
    const _: () = assert!(M < i32::MAX as i64);
    let q = ((x as i64 * M) >> 32) as i32;
    let r = (x << 16).wrapping_sub(q.wrapping_mul(PI2_RAW));
    let q = if r < 0 {
        q - 1
    } else if r >= PI2_RAW {
        q + 1
    } else {
        q
    };
    q & 0xFFFF
}

/// sin of a phase in *turns* (1.0 = full cycle). The waveform functions are
/// cos in turns: cos(t) = sin(t + 1/4).
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn cos_turns(t: Fx) -> Fx {
    sin_turns(t + Fx::from_raw(1 << 14))
}

/// specified in turns, so this is the core primitive; radian `sin` reduces
/// into it.
///
/// A 256-step quarter-wave table with linear interpolation (Gitea #941),
/// replacing a 9th-order Taylor series of five widening multiplies and four
/// constant divides. The phase wraps to 16 bits, folds to a quarter wave
/// `t ∈ [0, 16384]`, and splits into a table step `i = t >> 6` and a
/// 6-bit fraction. [`SIN_DEV`] holds `sin` MINUS its chord `4t` (the
/// straight line from 0 to 1.0 over the quarter) in quarter-LSB units,
/// which fits a `u16` with two extra fraction bits where the bare sine
/// (which reaches 65536) would not. The chord is exact in integers, so
/// interpolating the deviation is interpolating the sine itself.
///
/// Accuracy: within 0.91 LSB (1.4e-5) of the true sine everywhere and
/// within 2 LSB of the pre-#941 Taylor form (itself up to 1.84 LSB off),
/// both pinned exhaustively over the 16-bit phase in `tests`. Exact at the
/// quarter points (0, ±1.0), odd (`sin(−t) == −sin(t)`) and monotone on
/// every quarter, because the table, fold and rounding are symmetric.
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn sin_turns(t: Fx) -> Fx {
    // wrap to [0, 1): the low 16 bits ARE the floored unit modulo
    // (`Fx::wrap_unit`, bit-identical to `mod_floor(Fx::ONE)` and no
    // hardware remainder).
    let t = t.wrap_unit().raw() as u32;
    // fold to a quarter wave: t ∈ [0, 16384]
    let neg = t >= 32_768;
    let t = t & 0x7FFF;
    let t = if t >= 16_384 { 32_768 - t } else { t };
    // i ∈ [0, 256] (256 only at exactly the quarter, where f == 0 and the
    // padding entry 257 is read with weight zero)
    let i = (t >> 6) as usize;
    let f = (t & 63) as i32;
    let (d0, d1) = (SIN_DEV[i] as i32, SIN_DEV[i + 1] as i32);
    // 4t + round((d0 + (d1 − d0)·f/64) / 4): d0·64 <= 55184·64 < 2^22
    let s = (t << 2) as i32 + ((d0 * 64 + (d1 - d0) * f + 128) >> 8);
    Fx::from_raw(if neg { -s } else { s })
}

/// `round(4 · 65536 · sin(i·π/512)) − 1024·i` for `i ∈ 0..=256`, plus one
/// zero of padding: the quarter-wave sine minus its chord, in quarter-LSB
/// units of 16.16. 516 bytes of `const` data in flash rodata — never a
/// `static`, which would cost DRAM `.stack` (#484). Regenerated and
/// checked against `f64::sin` in `tests::sin_dev_table_matches_its_formula`.
#[rustfmt::skip]
const SIN_DEV: [u16; 258] = [
0, 584, 1169, 1753, 2337, 2921, 3505, 4088, 4671, 5253, 5835, 6416,
    6997, 7576, 8155, 8733, 9311, 9887, 10462, 11036, 11609, 12181, 12752, 13321,
    13889, 14455, 15020, 15583, 16145, 16705, 17263, 17819, 18374, 18926, 19477, 20026,
    20572, 21116, 21658, 22198, 22736, 23271, 23804, 24334, 24861, 25386, 25908, 26428,
    26944, 27458, 27969, 28477, 28982, 29484, 29982, 30478, 30970, 31458, 31944, 32426,
    32904, 33379, 33851, 34318, 34782, 35242, 35699, 36151, 36600, 37044, 37485, 37921,
    38353, 38781, 39205, 39624, 40039, 40449, 40855, 41257, 41654, 42046, 42434, 42816,
    43194, 43567, 43936, 44299, 44657, 45010, 45358, 45701, 46038, 46371, 46698, 47019,
    47335, 47646, 47951, 48251, 48545, 48833, 49115, 49392, 49663, 49928, 50187, 50440,
    50687, 50928, 51163, 51392, 51614, 51831, 52041, 52244, 52441, 52632, 52816, 52994,
    53165, 53330, 53487, 53639, 53783, 53920, 54051, 54175, 54292, 54402, 54505, 54600,
    54689, 54771, 54845, 54912, 54972, 55024, 55070, 55107, 55138, 55161, 55176, 55184,
    55184, 55177, 55162, 55139, 55108, 55070, 55024, 54970, 54908, 54838, 54760, 54675,
    54581, 54479, 54369, 54251, 54125, 53990, 53848, 53697, 53537, 53370, 53194, 53009,
    52816, 52615, 52405, 52187, 51960, 51725, 51481, 51228, 50966, 50696, 50417, 50130,
    49833, 49528, 49214, 48891, 48559, 48219, 47869, 47510, 47143, 46766, 46380, 45985,
    45581, 45168, 44746, 44315, 43874, 43425, 42966, 42498, 42020, 41533, 41037, 40532,
    40017, 39493, 38959, 38417, 37864, 37302, 36731, 36150, 35560, 34960, 34351, 33732,
    33104, 32466, 31818, 31161, 30494, 29818, 29132, 28436, 27731, 27016, 26291, 25557,
    24813, 24059, 23295, 22522, 21739, 20946, 20143, 19331, 18509, 17677, 16835, 15983,
    15122, 14250, 13369, 12478, 11578, 10667, 9747, 8816, 7876, 6926, 5966, 4997,
    4017, 3028, 2028, 1019, 0, 0,
];

/// sin(x), x in radians.
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn sin(x: Fx) -> Fx {
    // Reduce mod 2π and convert to turns in one step — `rad_to_turns` is
    // the floored `x mod 2π`, scaled by `2^16/2π`, as one widening multiply.
    sin_turns(Fx::from_raw(rad_to_turns(x.raw())))
}

/// cos(x), x in radians.
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn cos(x: Fx) -> Fx {
    sin(x + Fx::from_raw(HALF_PI_RAW))
}

/// tan(x) = sin/cos; where cos is 0 the VM's x/0 = 0 rule applies.
pub fn tan(x: Fx) -> Fx {
    sin(x) / cos(x)
}

/// Floor square root, sign-preserving: `sqrt(-4) == -2`. Oracle-confirmed
/// on fw 3.67 (this is PB's documented "square root returns negative" quirk).
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn sqrt(x: Fx) -> Fx {
    // |raw| <= 2^31, so the argument is < 2^47 and isqrt48 applies.
    let mag = isqrt48((x.raw().unsigned_abs() as u64) << 16) as i32;
    Fx::from_raw(if x.raw() < 0 { -mag } else { mag })
}

/// `round(sqrt(i + 64.5) · 2^12)` for `i ∈ 0..192`: the seed of
/// [`isqrt48`]'s Newton iteration, indexed by the top eight bits of the
/// normalised radicand (which start at 64 because the top bit is set).
/// 384 bytes of `const` data, read once per root.
#[rustfmt::skip]
const SQRT_SEED: [u16; 192] = [
    32896, 33150, 33402, 33652, 33900, 34147, 34392, 34635, 34876, 35116, 35354, 35590,
    35825, 36059, 36291, 36521, 36750, 36978, 37204, 37429, 37652, 37874, 38095, 38315,
    38533, 38750, 38966, 39181, 39394, 39606, 39818, 40028, 40237, 40445, 40652, 40857,
    41062, 41266, 41469, 41671, 41871, 42071, 42270, 42468, 42665, 42861, 43057, 43251,
    43445, 43637, 43829, 44020, 44210, 44400, 44588, 44776, 44963, 45149, 45334, 45519,
    45703, 45886, 46069, 46250, 46431, 46612, 46791, 46970, 47149, 47326, 47503, 47679,
    47855, 48030, 48204, 48378, 48551, 48723, 48895, 49067, 49237, 49407, 49577, 49746,
    49914, 50082, 50249, 50416, 50582, 50747, 50912, 51077, 51241, 51404, 51567, 51730,
    51892, 52053, 52214, 52374, 52534, 52694, 52853, 53011, 53169, 53327, 53484, 53640,
    53797, 53952, 54108, 54262, 54417, 54571, 54724, 54877, 55030, 55182, 55334, 55485,
    55636, 55787, 55937, 56087, 56236, 56385, 56534, 56682, 56830, 56977, 57124, 57271,
    57417, 57563, 57709, 57854, 57999, 58143, 58287, 58431, 58574, 58717, 58860, 59002,
    59144, 59286, 59427, 59568, 59709, 59849, 59989, 60129, 60268, 60407, 60546, 60684,
    60822, 60960, 61098, 61235, 61372, 61508, 61644, 61780, 61916, 62051, 62186, 62321,
    62456, 62590, 62724, 62857, 62991, 63124, 63256, 63389, 63521, 63653, 63785, 63916,
    64047, 64178, 64309, 64439, 64569, 64699, 64828, 64957, 65086, 65215, 65344, 65472,
];

/// Exact `floor(sqrt(n))` for `n < 2^48` (both call sites are bounded:
/// `sqrt` feeds `|raw| << 16 < 2^47`, `asin` feeds `<= 2^32`).
///
/// A floor root is a unique integer, so any correct algorithm is
/// bit-identical to any other; this one is chosen for the LX7 (Gitea
/// #938). The digit-by-digit forms below ran 24 iterations — ~310
/// straight-line instructions once unrolled, ~350 cycles per `sqrt()` on
/// the S3 — where two Newton steps and a floor fix-up need two hardware
/// divides and a dozen multiplies:
///
/// 1. normalise by an EVEN shift so the top set bit is bit 46 or 47 —
///    `sqrt(m) ∈ [2^23, 2^24)`, and `floor(sqrt(n)) = floor(sqrt(m)) >> sh/2`
///    exactly (floor commutes with dividing by a power of two);
/// 2. seed from [`SQRT_SEED`] on the top 8 bits (within ~0.4 %);
/// 3. one Newton step on the top 32 bits — `m/r0 ≈ (hi / (r0 >> 8)) << 8`,
///    a 32-bit divide — brings the error under ~400 units;
/// 4. one Newton step on the EXACT 64-bit remainder, `(m − r1²)/(2·r1)`
///    as `(rem >> 8) / (r1 >> 7)` in 32 bits (`|rem| < 2^35`), brings it
///    under about one;
/// 5. the floor is then settled on the 32-bit remainder `m − r²` (which
///    fits, r being within a couple of units), a loop of at most two steps
///    either way with no 64-bit square per step.
///
/// Deliberately NOT `#[inline]`, on any of the three functions here.
/// Marking them inline lets `sqrt` be pulled into `hypot`/`hypot3`/`asin`
/// and the VM's builtin dispatch, and the resulting code growth cost −17 %
/// on `dire-spider-2d` (6 sin + 3 hypot + 2 cos + 1 atan2 per pixel) while
/// buying nothing on `crosstown-traffic-2d` (24 `dist` per pixel) — both
/// measured with `luxel bench`, 512 px x 400 frames, best of 9-12
/// interleaved rounds, 2026-09-06.
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
fn isqrt48(n: u64) -> u32 {
    debug_assert!(n < 1u64 << 48);
    if n == 0 {
        return 0;
    }
    // `u64::leading_zeros` is a ~30-instruction software sequence on
    // Xtensa; the u32 form is one `nsau`.
    let nh = (n >> 32) as u32;
    let lz = if nh != 0 {
        nh.leading_zeros()
    } else {
        32 + (n as u32).leading_zeros()
    };
    let sh = (lz - 16) & !1; // even, 0..=30
    let m = n << sh; // [2^46, 2^48)
    let hi = (m >> 16) as u32; // [2^30, 2^32)
    let r0 = (SQRT_SEED[(hi >> 24) as usize - 64] as u32) << 8; // ≈ sqrt(m)
    let r1 = (r0 + ((hi / (r0 >> 8)) << 8)) >> 1;
    let rem = m.wrapping_sub((r1 as u64) * (r1 as u64)) as i64;
    let delta = ((rem >> 8) as i32) / ((r1 >> 7) as i32);
    let mut r = r1.wrapping_add(delta as u32);
    // The remainder of the corrected root, `m − r²`, is `rem − δ·(2·r1 + δ)`
    // and is within a few multiples of `r` of zero (r is within a couple of
    // units of the root, see above), so it fits i32 and every step below is
    // 32-bit: stepping r down adds `2r − 1` to it, stepping up takes
    // `2r + 1` away. No 64-bit square per step.
    let mut rem = (rem as i32).wrapping_sub(delta.wrapping_mul(((r1 << 1) as i32).wrapping_add(delta)));
    while rem < 0 {
        rem += ((r << 1) - 1) as i32;
        r -= 1;
    }
    while rem >= ((r << 1) + 1) as i32 {
        rem -= ((r << 1) + 1) as i32;
        r += 1;
    }
    r >> (sh / 2)
}

/// Wide form: the classic bitwise integer square root on 64-bit words —
/// one compare / add / subtract per iteration on any target with a 64-bit
/// ALU, and it skips all the leading zero digit-pairs up front. This is the
/// original implementation, unchanged. Since Gitea #938 it is a reference
/// the tests hold [`isqrt48`] to, not a code path.
#[allow(dead_code)]
fn isqrt48_wide(n: u64) -> u32 {
    let mut x = n;
    let mut c: u64 = 0;
    let mut d: u64 = 1 << 62;
    while d > n {
        d >>= 2;
    }
    while d != 0 {
        if x >= c + d {
            x -= c + d;
            c = (c >> 1) + d;
        } else {
            c >>= 1;
        }
        d >>= 2;
    }
    c as u32
}

/// Narrow form: digit-by-digit (two input bits per step), most-significant
/// first. The classic invariant `rem <= 2·root` bounds every value:
/// `root < 2^24` and `rem <= 2^25`, so `rem << 2 <= 2^27` — all of it in
/// 32-bit registers, where [`isqrt48_wide`] does 64-bit compares, adds and
/// subtracts (3–4 Xtensa instructions each) for ~24 iterations. The device
/// path from #312 until #938; now a second reference for the tests.
#[allow(dead_code)]
fn isqrt48_narrow(n: u64) -> u32 {
    debug_assert!(n < 1u64 << 48);
    let hi = (n >> 32) as u32; // bits 32..48
    let lo = n as u32;
    let mut rem: u32 = 0;
    let mut root: u32 = 0;
    // Leading zero digit-pairs leave rem and root at 0 (rem<<2 == 0 and the
    // test value 1 always exceeds it), so skipping the whole high word when
    // it is zero is exactly equivalent — and that is the common case.
    if hi != 0 {
        let mut w = hi << 16; // 16 significant bits, top-aligned
        let mut i = 0;
        while i < 8 {
            rem = (rem << 2) | (w >> 30);
            w <<= 2;
            root <<= 1;
            let test = (root << 1) | 1;
            if rem >= test {
                rem -= test;
                root |= 1;
            }
            i += 1;
        }
    }
    let mut w = lo;
    let mut i = 0;
    while i < 16 {
        rem = (rem << 2) | (w >> 30);
        w <<= 2;
        root <<= 1;
        let test = (root << 1) | 1;
        if rem >= test {
            rem -= test;
            root |= 1;
        }
        i += 1;
    }
    root
}

/// hypot: squares summed at full precision, then the SUM wraps into the
/// 16.16 domain before the (sign-preserving) sqrt. Oracle-confirmed:
/// `hypot(200, 200) == 120.266…` on real hardware (80000 wraps to 14464).
pub fn hypot(x: Fx, y: Fx) -> Fx {
    hypot_raw(&[x, y])
}

pub fn hypot3(x: Fx, y: Fx, z: Fx) -> Fx {
    hypot_raw(&[x, y, z])
}

#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
fn hypot_raw(vs: &[Fx]) -> Fx {
    // Only the low 32 bits of the old i64 accumulator ever reached
    // `Fx::from_raw`, and truncation to 32 bits is a ring homomorphism
    // (it commutes with `+`), so wrapping-accumulating the already
    // truncated terms is bit-identical — and each term is one widening
    // 32x32 multiply instead of a 64x64 one.
    let mut sum: i32 = 0;
    for v in vs {
        let r = v.raw();
        sum = sum.wrapping_add(fmul32(r, r));
    }
    sqrt(Fx::from_raw(sum))
}

/// 2^x. Integer part is an exact shift; fraction via series on f·ln2,
/// evaluated in 32-frac so the 16-frac mantissa comes out correctly rounded
/// (the sweep comparison put the old 16-frac evaluation at ~7e-5 relative
/// error, dominated by truncation in the series terms).
///
/// Overflow SATURATES to `Fx::MAX` — unlike ordinary arithmetic, which
/// wraps. PB-exact: oracle-probed 2026-08-23 (fw 3.67), pow(2,16),
/// pow(2,20), pow(2,15), pow(2,15.5) and pow(10,10) all return raw
/// 0x7FFFFFFF exactly (pinned via raw-wrap subtraction, not display
/// rounding). The old wrap made pow(2,16) = 0, which zeroed `% pow(2,16)`
/// idioms in corpus PRNGs (Gitea #112).
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn exp2(x: Fx) -> Fx {
    let n = x.to_int_floor();
    let f = (x - Fx::from_int(n)).raw() as u32; // [0, 65536) as 16-frac
    // y = (f · round(ln2·2^32)) >> 16, in 32-frac, <= ln2·2^32 < 2^32.
    //
    // round(ln2·2^32) = 2_977_044_472 = 45_426·2^16 + 6_136 exactly, and
    // floor(f·(a·2^16 + b) / 2^16) == f·a + floor(f·b / 2^16) because
    // f·a·2^16 is already a multiple of 2^16. Both halves stay in u32:
    // f <= 65535 gives f·45426 <= 2_976_992_910 and (f·6136)>>16 <= 6_135,
    // summing to at most 2_976_999_045 < 2^32. No 64-bit intermediate.
    let y = f * 45_426 + ((f * 6_136) >> 16);
    // `((a·b) >> 32)` for a, b < 2^32 is precisely the high word of the
    // widening 32x32 product — a single `muluh`, where the old i128 form
    // risked `__multi3`. Every y_k is nonnegative and < y < 2^32 (each step
    // multiplies by y/2^32 <= ln2 < 1 and truncates down).
    let mul32 = |a: u32, b: u32| (((a as u64) * (b as u64)) >> 32) as u32;
    let y2 = mul32(y, y);
    let y3 = mul32(y2, y);
    let y4 = mul32(y3, y);
    let y5 = mul32(y4, y);
    let y6 = mul32(y5, y);
    let y7 = mul32(y6, y);
    // Series tail without the leading 2^32 term. This is 2^32·(2^f − 1)
    // minus the truncation losses, so it sits just under 2^32: swept over
    // all 65536 reachable f, the maximum is 4_294_870_294, i.e. 97_002
    // below 2^32 — comfortably more than the 32_768 rounding term added
    // below, so neither the sum nor the rounding can overflow u32.
    // The divisors are u32 constants now, so each is one magic multiply
    // instead of a `__udivdi3` ROM call.
    let s = y + y2 / 2 + y3 / 6 + y4 / 24 + y5 / 120 + y6 / 720 + y7 / 5_040;
    // Old: m = ((2^32 + s + 2^15) >> 16). Splitting off the exact 2^32 term
    // leaves a pure 32-bit shift. m ∈ [65536, 131070], i.e. [1, 2) in 16-frac.
    let m = 65_536i32 + ((s + (1 << 15)) >> 16) as i32;
    if n >= 0 {
        // m >= 2^16, so any n >= 15 lands at or past 2^31; check the rest
        // in 32 bits. Saturate, per the doc comment above.
        if n >= 15 {
            return Fx::MAX;
        }
        // m <= 131_070 < 2^17 and n <= 14, so m << n <= 2_147_246_080 —
        // it fits u32 (and in fact i32), no i64 shift needed.
        let r = (m as u32) << (n as u32);
        if r > i32::MAX as u32 {
            Fx::MAX
        } else {
            Fx::from_raw(r as i32)
        }
    } else {
        let s = (-n) as u32;
        Fx::from_raw(if s >= 32 { 0 } else { m >> s })
    }
}

/// One `m ← (m·m) >> 16` step of the `log2` mantissa loop, for
/// `m ∈ [2^16, 2^17)`. See [`NARROW_WORD`].
#[inline]
fn sq16(m: u32) -> u32 {
    if NARROW_WORD {
        sq16_narrow(m)
    } else {
        sq16_wide(m)
    }
}

/// Wide form: one 64-bit multiply and shift.
#[inline]
fn sq16_wide(m: u32) -> u32 {
    (((m as u64) * (m as u64)) >> 16) as u32
}

/// Narrow form: with `m = 2^16 + d` and `d ∈ [0, 2^16)`,
///   `(m·m) >> 16 = (2^32 + 2^17·d + d²) >> 16 = 2^16 + 2d + (d² >> 16)`
/// exactly, because the first two terms are multiples of 2^16. `d²` is at
/// most `(2^16−1)² = 4_294_836_225 < 2^32`, so this is a plain 32x32→32
/// `mull` with no high half at all.
#[inline]
fn sq16_narrow(m: u32) -> u32 {
    debug_assert!((65_536..131_072).contains(&m));
    let d = m - 65_536;
    65_536 + 2 * d + ((d * d) >> 16)
}

/// log2(x); x ≤ 0 yields the most-negative value (oracle-verified exact:
/// log2_0/log2_neg both return raw i32::MIN on the PB).
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn log2(x: Fx) -> Fx {
    if x.raw() <= 0 {
        return Fx::MIN;
    }
    let raw = x.raw() as u32; // > 0, so < 2^31
    // 31 - lz(u32) == 63 - lz(u64) for the same value; msb ∈ [0, 30].
    let msb = 31 - raw.leading_zeros() as i32;
    let int_part = msb - 16;
    // normalize mantissa to [1, 2) as 16-frac: m ∈ [2^16, 2^17)
    let mut m = if msb > 16 {
        raw >> (msb - 16)
    } else {
        raw << (16 - msb)
    };
    // 16 fraction bits by repeated squaring
    let mut frac: i32 = 0;
    let mut i = 0;
    while i < 16 {
        frac <<= 1;
        m = sq16(m);
        // m <= 65536 + 131070 + 65534 = 262140 < 2^18 here.
        if m >= 131_072 {
            frac |= 1;
            m >>= 1;
        }
        // ...and m is back in [2^16, 2^17), restoring the invariant.
        i += 1;
    }
    Fx::from_raw((int_part << 16).wrapping_add(frac))
}

/// Natural log via log2.
pub fn ln(x: Fx) -> Fx {
    if x.raw() <= 0 {
        return Fx::MIN;
    }
    Fx::from_raw(fmul32(log2(x).raw(), LN2_RAW))
}

/// e^x.
pub fn exp(x: Fx) -> Fx {
    exp2(Fx::from_raw(fmul32(x.raw(), LOG2E_RAW)))
}

/// pow(base, exp) = 2^(exp·log2 base). Oracle-confirmed: pow(x, 0) == 1
/// (including 0^0), and negative bases work with integer exponents with the
/// usual sign rule (pow(-2, 3) == -8). Negative base with a fractional
/// exponent yields Fx::MIN — the PB does log2(negative) = MIN and lets it
/// propagate (oracle-verified 2026-07-07, pow_neg2_half/pow_neg2_15).
///
/// **A positive integer exponent up to [`POW_INT_MAX`] is repeated 16.16
/// multiplication instead** (Gitea #938): `pow(x, 1) == x`, `pow(x, 2) ==
/// x * x`, `pow(2, 10) == 1024` exactly, where the log/exp route was off by
/// its 16-bit `log2` fraction (`pow(2, 10)` read 1023.9) and cost ~480
/// cycles on the S3 against ~10 per multiply. The magnitude is multiplied
/// and the sign restored from the exponent's parity, so `pow(-x, n) ==
/// ±pow(x, n)` holds exactly as it did on the log/exp route (an operator
/// chain `x*x*x` on a negative `x` can differ from this by one LSB — it
/// floors each signed product toward −∞). Overflow saturates to `Fx::MAX`,
/// or `Fx::MIN` for a negative result, as before. This is a deliberate
/// change of bits for the integer-exponent case: more accurate, not
/// bit-identical to the pre-#938 output.
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn pow(base: Fx, e: Fx) -> Fx {
    if e == Fx::ZERO {
        return Fx::ONE;
    }
    if base.raw() == 0 {
        return Fx::ZERO;
    }
    if e.raw() & 0xFFFF == 0 {
        let n = e.raw() >> 16;
        if n >= 1 && n <= POW_INT_MAX {
            return pow_int(base, n as u32);
        }
    }
    if base.raw() < 0 {
        if e.frac() != Fx::ZERO {
            return Fx::MIN;
        }
        let mag = exp2(e * log2(-base));
        // Saturated magnitude with an odd exponent lands on Fx::MIN, not
        // -Fx::MAX: oracle-pinned (2026-08-23) pow(-2, 17) == raw
        // 0x80000000 exactly.
        if mag == Fx::MAX && e.to_int_trunc() & 1 == 1 {
            return Fx::MIN;
        }
        return if e.to_int_trunc() & 1 == 1 { -mag } else { mag };
    }
    exp2(e * log2(base))
}

/// The largest integer exponent [`pow`] evaluates by repeated
/// multiplication. Above it the log/exp route is both cheaper and, with
/// `(n−1)` truncations of a sub-unit base no longer negligible, about as
/// accurate.
pub const POW_INT_MAX: i32 = 16;

/// `base^n` for `n ∈ 1..=POW_INT_MAX` by repeated multiplication of the
/// magnitude; see [`pow`]. Each step is one widening 32x32 multiply and a
/// saturation test on the high word.
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
fn pow_int(base: Fx, n: u32) -> Fx {
    let b = base.raw().unsigned_abs(); // <= 2^31
    let neg = base.raw() < 0 && n & 1 == 1;
    let mut acc: u32 = b;
    let mut i = 1;
    while i < n {
        let p = ((acc as u64) * (b as u64)) >> 16;
        if p > i32::MAX as u64 {
            return if neg { Fx::MIN } else { Fx::MAX };
        }
        acc = p as u32;
        i += 1;
    }
    // `acc` exceeds i32::MAX only for n == 1 and base == Fx::MIN, whose
    // negation IS Fx::MIN.
    Fx::from_raw(if neg { (acc as i32).wrapping_neg() } else { acc as i32 })
}

/// atan(x) via odd minimax polynomial (error ≈ 1e-4 rad).
pub fn atan(x: Fx) -> Fx {
    let raw = x.raw();
    // `raw.abs() <= 65536` in i64 == `unsigned_abs() <= 65536` (the only
    // difference, i32::MIN, is 2^31 either way).
    if raw.unsigned_abs() <= 65_536 {
        atan_unit(raw)
    } else {
        // atan(x) = sign·π/2 − atan(1/x)
        //
        // |raw| >= 65537, so floor(2^32 / |raw|) <= 65535 and the old
        // truncating `(1i64 << 32) / raw` equals sign(raw) · that floor —
        // which is `div_shift16(2^16, |raw|)` (num = 2^16 <= |raw| = d, and
        // d <= 2^31, so the precondition holds). 16-frac 1/x, |x|>1 so
        // |recip| <= 1.
        let d = raw.unsigned_abs();
        let mag = div_shift16(1 << 16, d) as i32;
        let recip = if raw > 0 { mag } else { -mag };
        let base = atan_unit(recip);
        let half = if raw > 0 { HALF_PI_RAW } else { -HALF_PI_RAW };
        // |base| <= 65527 (see atan_unit), so this sum cannot overflow i32.
        Fx::from_raw(half - base.raw())
    }
}

/// atan on |z| ≤ 1 (raw 16-frac in, Fx out).
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
fn atan_unit(z: i32) -> Fx {
    // Hastings: atan(z) ≈ z(A + B z² + C z⁴ + D z⁶ + E z⁸), err ≈ 1e-4 rad
    const A: i32 = 65_527; // 0.9998660
    const B: i32 = -21_647; // -0.3302995
    const C: i32 = 11_807; // 0.1801410
    const D: i32 = -5_580; // -0.0851330
    const E: i32 = 1_366; // 0.0208351
    // |z| <= 65536 so z2 = (z²)>>16 ∈ [0, 65536]. Every Horner stage is
    // `(z2 · c) >> 16` with |c| <= 65527, so |stage| <= 65527 and the
    // running coefficient stays within [-21647, 65527] — all far inside
    // i32, and the products are formed in i64 by fmul32 anyway. The final
    // p ∈ [51473, 65527] and (z·p)>>16 <= 65527 fits i32, so this is
    // bit-identical to the i64 version's `fmul(z, p) as i32`.
    let z2 = fmul32(z, z);
    let p = A + fmul32(z2, B + fmul32(z2, C + fmul32(z2, D + fmul32(z2, E))));
    Fx::from_raw(fmul32(z, p))
}

/// atan2(y, x) with the usual quadrant conventions; atan2(0, 0) = 0.
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn atan2(y: Fx, x: Fx) -> Fx {
    let (yr, xr) = (y.raw(), x.raw());
    if xr == 0 && yr == 0 {
        return Fx::ZERO;
    }
    if xr == 0 {
        return Fx::from_raw(if yr > 0 { HALF_PI_RAW } else { -HALF_PI_RAW });
    }
    if yr == 0 {
        return Fx::from_raw(if xr > 0 { 0 } else { PI_RAW });
    }
    // pick the ratio with |·| ≤ 1 to stay in the polynomial's sweet spot.
    //
    // Rust's `/` truncates toward zero, so for the chosen pair (|a| <= |b|)
    // `(a << 16) / b` == sign(a·b) · floor(|a|·2^16 / |b|), and the floor is
    // exactly div_shift16(|a|, |b|) — precondition |a| <= |b| <= 2^31 holds
    // by construction. The result magnitude is <= 65536, so it fits i32 and
    // atan_unit's domain. This replaces a `__divdi3` on a 48-bit dividend.
    let (ya, xa) = (yr.unsigned_abs(), xr.unsigned_abs());
    let neg = (yr < 0) != (xr < 0);
    // |atan_unit| <= 65527 and PI_RAW = 205887, so every sum below is at
    // most 271414 in magnitude — no i32 overflow, unlike the raw squares.
    let a = if ya <= xa {
        let m = div_shift16(ya, xa) as i32;
        let base = atan_unit(if neg { -m } else { m }).raw();
        if xr > 0 {
            base
        } else if yr > 0 {
            base + PI_RAW
        } else {
            base - PI_RAW
        }
    } else {
        let m = div_shift16(xa, ya) as i32;
        let base = atan_unit(if neg { -m } else { m }).raw();
        if yr > 0 {
            HALF_PI_RAW - base
        } else {
            -HALF_PI_RAW - base
        }
    };
    Fx::from_raw(a)
}

/// asin(x), inputs clamped to [-1, 1].
pub fn asin(x: Fx) -> Fx {
    let x = x.clamp(-Fx::ONE, Fx::ONE);
    // asin(x) = atan2(x, sqrt(1 − x²)); 1−x² in 32-frac to dodge the x² wrap.
    // |xr| <= 65536 so xr² <= 2^32 and the difference is in [0, 2^32] — it
    // needs the extra bit only for x == 0, which is why this one stays u64
    // (a single 64-bit subtract, no libcall). The square itself is a
    // widening 32x32 multiply.
    let xr = x.raw();
    let sq = ((xr as i64) * (xr as i64)) as u64;
    let root = isqrt48((1u64 << 32) - sq) as i32; // back to 16-frac
    atan2(x, Fx::from_raw(root))
}

/// acos(x) = π/2 − asin(x).
pub fn acos(x: Fx) -> Fx {
    Fx::from_raw(HALF_PI_RAW) - asin(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn assert_close(actual: Fx, expected: f64, tol: f64) {
        let a = actual.to_f64();
        assert!(
            (a - expected).abs() < tol,
            "expected ≈{expected}, got {a} (tol {tol})"
        );
    }

    // ---------------------------------------------------------------
    // Verbatim copies of the pre-#312 64-bit implementations. These are
    // the bit-exactness oracle for the 32-bit rewrites above; do not
    // "modernize" them.
    // ---------------------------------------------------------------

    const R_PI_RAW: i64 = 205_887;
    const R_PI2_RAW: i64 = 411_775;
    const R_HALF_PI_RAW: i64 = 102_944;
    const R_LOG2E_RAW: i64 = 94_548;
    const R_LN2_RAW: i64 = 45_426;

    fn rfmul(a: i64, b: i64) -> i64 {
        (a * b) >> 16
    }

    fn reference_sin_turns(t: Fx) -> Fx {
        let t = t.mod_floor(Fx::ONE).raw() as i64;
        let (t, neg) = if t >= 32_768 {
            (t - 32_768, true)
        } else {
            (t, false)
        };
        let t = if t >= 16_384 { 32_768 - t } else { t };
        let z = rfmul(t, R_PI2_RAW);
        let z2 = rfmul(z, z);
        let z3 = rfmul(z2, z);
        let z5 = rfmul(z3, z2);
        let z7 = rfmul(z5, z2);
        let z9 = rfmul(z7, z2);
        let s = (z - z3 / 6 + z5 / 120 - z7 / 5040 + z9 / 362_880).min(65_536);
        Fx::from_raw(if neg { -s } else { s } as i32)
    }

    fn reference_turns(x: Fx) -> Fx {
        let r = x.mod_floor(Fx::from_raw(R_PI2_RAW as i32)).raw() as i64;
        Fx::from_raw(((r << 16) / R_PI2_RAW) as i32)
    }

    fn reference_sin(x: Fx) -> Fx {
        reference_sin_turns(reference_turns(x))
    }

    /// The table `sin_turns` (Gitea #941) behind the i64 radian reduction:
    /// what `sin` must equal bit for bit now that only the reduction is
    /// still pinned to the pre-#312 form.
    fn table_sin(x: Fx) -> Fx {
        sin_turns(reference_turns(x))
    }

    fn reference_sqrt(x: Fx) -> Fx {
        let mag = reference_isqrt64((x.raw().unsigned_abs() as u64) << 16) as i32;
        Fx::from_raw(if x.raw() < 0 { -mag } else { mag })
    }

    fn reference_isqrt64(n: u64) -> u32 {
        let mut x = n;
        let mut c: u64 = 0;
        let mut d: u64 = 1 << 62;
        while d > n {
            d >>= 2;
        }
        while d != 0 {
            if x >= c + d {
                x -= c + d;
                c = (c >> 1) + d;
            } else {
                c >>= 1;
            }
            d >>= 2;
        }
        c as u32
    }

    fn reference_hypot_raw(vs: &[Fx]) -> Fx {
        let mut sum: i64 = 0;
        for v in vs {
            let r = v.raw() as i64;
            sum = sum.wrapping_add((r * r) >> 16);
        }
        reference_sqrt(Fx::from_raw(sum as i32))
    }

    fn reference_exp2(x: Fx) -> Fx {
        let n = x.to_int_floor();
        let f = (x - Fx::from_int(n)).raw() as i64;
        const LN2_32: i64 = 2_977_044_472;
        let y = (f * LN2_32) >> 16;
        let mul32 = |a: i64, b: i64| ((a as i128 * b as i128) >> 32) as i64;
        let y2 = mul32(y, y);
        let y3 = mul32(y2, y);
        let y4 = mul32(y3, y);
        let y5 = mul32(y4, y);
        let y6 = mul32(y5, y);
        let y7 = mul32(y6, y);
        let m32 = (1i64 << 32) + y + y2 / 2 + y3 / 6 + y4 / 24 + y5 / 120 + y6 / 720 + y7 / 5040;
        let m = ((m32 + (1 << 15)) >> 16) as i32;
        if n >= 0 {
            if n >= 15 {
                return Fx::MAX;
            }
            let r = (m as i64) << n;
            if r > i32::MAX as i64 {
                Fx::MAX
            } else {
                Fx::from_raw(r as i32)
            }
        } else {
            let s = (-n) as u32;
            Fx::from_raw(if s >= 32 { 0 } else { m >> s })
        }
    }

    fn reference_log2(x: Fx) -> Fx {
        if x.raw() <= 0 {
            return Fx::MIN;
        }
        let raw = x.raw() as u64;
        let msb = 63 - raw.leading_zeros() as i32;
        let int_part = msb - 16;
        let mut m = if msb > 16 {
            raw >> (msb - 16)
        } else {
            raw << (16 - msb)
        };
        let mut frac: i32 = 0;
        for _ in 0..16 {
            frac <<= 1;
            m = (m * m) >> 16;
            if m >= 2 << 16 {
                frac |= 1;
                m >>= 1;
            }
        }
        Fx::from_raw((int_part << 16).wrapping_add(frac))
    }

    fn reference_ln(x: Fx) -> Fx {
        if x.raw() <= 0 {
            return Fx::MIN;
        }
        Fx::from_raw(rfmul(reference_log2(x).raw() as i64, R_LN2_RAW) as i32)
    }

    fn reference_exp(x: Fx) -> Fx {
        reference_exp2(Fx::from_raw(rfmul(x.raw() as i64, R_LOG2E_RAW) as i32))
    }

    fn reference_atan_unit(z: i64) -> Fx {
        const A: i64 = 65_527;
        const B: i64 = -21_647;
        const C: i64 = 11_807;
        const D: i64 = -5_580;
        const E: i64 = 1_366;
        let z2 = rfmul(z, z);
        let p = A + rfmul(z2, B + rfmul(z2, C + rfmul(z2, D + rfmul(z2, E))));
        Fx::from_raw(rfmul(z, p) as i32)
    }

    fn reference_atan(x: Fx) -> Fx {
        let raw = x.raw() as i64;
        if raw.abs() <= 65_536 {
            reference_atan_unit(raw)
        } else {
            let recip = (1i64 << 32) / raw;
            let base = reference_atan_unit(recip);
            let half = if raw > 0 { R_HALF_PI_RAW } else { -R_HALF_PI_RAW };
            Fx::from_raw((half - base.raw() as i64) as i32)
        }
    }

    fn reference_atan2(y: Fx, x: Fx) -> Fx {
        let (yr, xr) = (y.raw() as i64, x.raw() as i64);
        if xr == 0 && yr == 0 {
            return Fx::ZERO;
        }
        if xr == 0 {
            return Fx::from_raw(if yr > 0 { R_HALF_PI_RAW } else { -R_HALF_PI_RAW } as i32);
        }
        if yr == 0 {
            return Fx::from_raw(if xr > 0 { 0 } else { R_PI_RAW } as i32);
        }
        let a = if yr.abs() <= xr.abs() {
            let base = reference_atan_unit((yr << 16) / xr);
            if xr > 0 {
                base.raw() as i64
            } else if yr > 0 {
                base.raw() as i64 + R_PI_RAW
            } else {
                base.raw() as i64 - R_PI_RAW
            }
        } else {
            let base = reference_atan_unit((xr << 16) / yr);
            if yr > 0 {
                R_HALF_PI_RAW - base.raw() as i64
            } else {
                -R_HALF_PI_RAW - base.raw() as i64
            }
        };
        Fx::from_raw(a as i32)
    }

    fn reference_asin(x: Fx) -> Fx {
        let x = x.clamp(-Fx::ONE, Fx::ONE);
        let xr = x.raw() as i64;
        let one_minus = (1i64 << 32) - xr * xr;
        let root = reference_isqrt64(one_minus as u64) as i32;
        reference_atan2(x, Fx::from_raw(root))
    }

    // ---------------------------------------------------------------
    // Shared input generators.
    // ---------------------------------------------------------------

    struct Lcg(u32);
    impl Lcg {
        fn new() -> Lcg {
            Lcg(0x9E37_79B9)
        }
        fn next(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            self.0
        }
    }

    /// Every documented edge of the 16.16 word plus power-of-two seams.
    fn edge_raws() -> Vec<i32> {
        let mut v: Vec<i32> = Vec::new();
        for &e in &[
            0i32,
            1,
            -1,
            2,
            -2,
            0x3FFF,
            0x4000,
            0x4001,
            0x7FFF,
            0x8000,
            0x8001,
            -0x8000,
            -0x8001,
            0xFFFF,
            0x1_0000,
            0x1_0001,
            -0x1_0000,
            -0x1_0001,
            0x2_0000,
            -0x2_0000,
            PI_RAW,
            -PI_RAW,
            PI2_RAW - 1,
            PI2_RAW,
            PI2_RAW + 1,
            -PI2_RAW,
            HALF_PI_RAW,
            -HALF_PI_RAW,
            i32::MAX,
            i32::MIN,
            i32::MAX - 1,
            i32::MIN + 1,
        ] {
            v.push(e);
        }
        // every power-of-two seam, ±1
        for k in 0..31 {
            let p = 1i32 << k;
            v.push(p);
            v.push(p - 1);
            v.push(p + 1);
            v.push(-p);
            v.push(-p + 1);
            v.push(-p - 1);
        }
        v
    }

    fn random_raws(n: usize) -> Vec<i32> {
        let mut lcg = Lcg::new();
        let mut v = Vec::with_capacity(n * 3);
        for _ in 0..n {
            let s = lcg.next();
            v.push(s as i32); // full range
            v.push((s >> 16) as i32 - 0x8000); // sub-unit magnitudes
            v.push(((s >> 20) as i32) << 16); // whole numbers
        }
        v
    }

    // ---------------------------------------------------------------
    // Bit-exactness sweeps (Gitea #312 32-bit narrowing).
    // ---------------------------------------------------------------

    /// Three-way: the wide form, the narrow form and the plain i64
    /// division must all agree, on every target — so a host test run pins
    /// the Xtensa/riscv32 path too (which is the whole point of keeping
    /// both compiled behind `NARROW_WORD` rather than `#[cfg]`).
    fn check_div_shift16(num: u32, d: u32) {
        let want = (((num as u64) << 16) / d as u64) as u32;
        assert_eq!(div_shift16_wide(num, d), want, "wide {num} <<16 / {d}");
        assert_eq!(div_shift16_narrow(num, d), want, "narrow {num} <<16 / {d}");
        assert_eq!(div_shift16(num, d), want, "selected {num} <<16 / {d}");
    }

    /// `sq16`'s domain is only 65536 values wide, so this is exhaustive:
    /// both forms against each other and against the u64 square the
    /// original `log2` loop used.
    #[test]
    fn sq16_narrow_matches_wide_exhaustively() {
        for m in 65_536u32..131_072 {
            let want = (((m as u64) * (m as u64)) >> 16) as u32;
            assert_eq!(sq16_wide(m), want, "wide sq16({m})");
            assert_eq!(sq16_narrow(m), want, "narrow sq16({m})");
            assert_eq!(sq16(m), want, "selected sq16({m})");
        }
    }

    #[test]
    fn div_shift16_matches_the_i64_division() {
        // The two shapes both call sites use.
        for d in [
            1u32,
            2,
            3,
            65_536,
            65_537,
            411_775,
            0x7FFF_FFFF,
            1 << 31,
            1 << 20,
            (1 << 31) - 1,
        ] {
            for num in [0u32, 1, 2, d / 3, d / 2, d - 1, d] {
                if num > d {
                    continue;
                }
                check_div_shift16(num, d);
            }
        }
        let mut lcg = Lcg::new();
        for _ in 0..20_000 {
            let d = (lcg.next() & 0x7FFF_FFFF).max(1);
            let num = lcg.next() % (d + 1);
            check_div_shift16(num, d);
        }
        // Gitea #938: the narrow form's two paths and their seams — small
        // divisors (one divide), and every normalisation shift of the big
        // ones with numerators at and just under the divisor, where the
        // estimate's overshoot is largest.
        for _ in 0..200_000 {
            let bits = 1 + lcg.next() % 31; // d ∈ [1, 2^31]
            let d = ((lcg.next() | (1 << 31)) >> (32 - bits)).max(1);
            let num = match lcg.next() % 4 {
                0 => d,
                1 => d - 1,
                2 => d / 2,
                _ => lcg.next() % (d + 1),
            };
            check_div_shift16(num, d);
        }
        for d in [1u32 << 16, (1 << 16) - 1, (1 << 16) + 1, 1 << 30, (1 << 31) - 1, 1 << 31] {
            for num in [0, 1, d / 3, d / 2, d - 1, d] {
                check_div_shift16(num, d);
            }
        }
        // Exhaustive over the whole domain `sin` uses: r ∈ [0, 2π_raw).
        for r in 0..411_775u32 {
            check_div_shift16(r, 411_775);
        }
    }

    #[test]
    fn isqrt48_matches_the_bitwise_reference() {
        // Every value `sqrt()` can actually feed it: raw.unsigned_abs()<<16
        // for a dense low sweep plus the seams and randoms.
        let mut ns: Vec<u64> = Vec::new();
        for r in 0..40_000u32 {
            ns.push((r as u64) << 16);
        }
        for &e in &edge_raws() {
            ns.push((e.unsigned_abs() as u64) << 16);
        }
        // asin's domain: [0, 2^32].
        for k in 0..=32u32 {
            ns.push(1u64 << k);
            ns.push((1u64 << k) - 1);
            ns.push((1u64 << k) + 1);
        }
        ns.push(1u64 << 32);
        // perfect squares and their neighbours (the floor boundary)
        for r in 0..4_000u64 {
            let sq = (r * 1_009) * (r * 1_009);
            if sq < (1 << 48) {
                ns.push(sq);
                ns.push(sq.saturating_sub(1));
                ns.push(sq + 1);
            }
        }
        let mut lcg = Lcg::new();
        for _ in 0..40_000 {
            let hi = (lcg.next() as u64) & 0xFFFF;
            let lo = lcg.next() as u64;
            ns.push((hi << 32) | lo);
        }
        // Gitea #938: the Newton form's seams — every even normalisation
        // shift, every seed-table row, and the floor boundary from both
        // sides at every magnitude.
        for _ in 0..300_000 {
            let bits = 1 + lcg.next() % 48;
            let w = ((lcg.next() as u64) << 32) | lcg.next() as u64;
            let n = (w | (1 << 63)) >> (64 - bits);
            ns.push(n);
            let r = reference_isqrt64(n) as u64;
            ns.push(r * r);
            ns.push(r * r + 2 * r); // (r+1)² − 1
            ns.push((r * r + 2 * r + 1).min((1 << 48) - 1));
        }
        for &n in &ns {
            let n = n & ((1u64 << 48) - 1);
            let got = isqrt48(n);
            assert_eq!(got, reference_isqrt64(n), "isqrt48({n})");
            // both compiled forms, on every target
            assert_eq!(isqrt48_wide(n), reference_isqrt64(n), "isqrt48_wide({n})");
            assert_eq!(isqrt48_narrow(n), reference_isqrt64(n), "isqrt48_narrow({n})");
            // and independently: it really is the floor of the root
            assert!((got as u64) * (got as u64) <= n);
            assert!((got as u64 + 1) * (got as u64 + 1) > n);
        }
    }

    #[test]
    fn exp2_matches_the_i128_reference() {
        // The fractional path depends only on the low 16 bits, and n = 0
        // reaches every one of them — so this is exhaustive over `f`.
        for f in 0..65_536i32 {
            let x = Fx::from_raw(f);
            assert_eq!(exp2(x).raw(), reference_exp2(x).raw(), "exp2 raw {f}");
        }
        // ...and sweep the integer part across every branch (n<0, n in
        // [0,15), the n>=15 saturation, and the s>=32 underflow).
        for n in -40i32..=40 {
            for f in [0i32, 1, 0x4000, 0x8000, 0xC000, 0xFFFF, 32_768, 65_535] {
                let raw = (n.wrapping_shl(16)).wrapping_add(f);
                let x = Fx::from_raw(raw);
                assert_eq!(exp2(x).raw(), reference_exp2(x).raw(), "exp2 raw {raw}");
            }
        }
        let mut vals = edge_raws();
        vals.extend(random_raws(6_000));
        for &r in &vals {
            let x = Fx::from_raw(r);
            assert_eq!(exp2(x).raw(), reference_exp2(x).raw(), "exp2 raw {r}");
        }
    }

    #[test]
    fn log2_matches_the_u64_reference() {
        // Dense low sweep covers every mantissa normalization shift for
        // msb <= 17 exhaustively.
        for r in 0..300_000i32 {
            let x = Fx::from_raw(r);
            assert_eq!(log2(x).raw(), reference_log2(x).raw(), "log2 raw {r}");
        }
        let mut vals = edge_raws();
        vals.extend(random_raws(20_000));
        // exercise every msb position
        let mut lcg = Lcg::new();
        for k in 0..31u32 {
            for _ in 0..200 {
                let m = if k == 0 { 1 } else { lcg.next() % (1 << k) };
                vals.push(((1u32 << k) | m) as i32);
            }
        }
        for &r in &vals {
            let x = Fx::from_raw(r);
            assert_eq!(log2(x).raw(), reference_log2(x).raw(), "log2 raw {r}");
            assert_eq!(ln(x).raw(), reference_ln(x).raw(), "ln raw {r}");
        }
    }

    /// The table `sin_turns` (Gitea #941) against the pre-#941 Taylor form
    /// it replaced, which stays the accuracy oracle: within 2 LSB of it and
    /// within 0.91 LSB of the true sine over the WHOLE 16-bit phase space.
    /// `sin_turns` only ever sees the low 16 bits (`wrap_unit`), so the
    /// exhaustive sweep is the whole domain; the random sweep across the
    /// word pins that wrap.
    #[test]
    fn sin_turns_within_2_lsb_of_the_taylor_reference() {
        let mut worst_ref = (0i32, 0i32);
        let mut worst_true = (0f64, 0i32);
        for r in 0..65_536i32 {
            let x = Fx::from_raw(r);
            let got = sin_turns(x).raw();
            let d = (got - reference_sin_turns(x).raw()).abs();
            if d > worst_ref.0 {
                worst_ref = (d, r);
            }
            let exact = (r as f64 * core::f64::consts::TAU / 65_536.0).sin() * 65_536.0;
            let e = (got as f64 - exact).abs();
            if e > worst_true.0 {
                worst_true = (e, r);
            }
            // the negative phase wraps onto the same word
            assert_eq!(
                sin_turns(Fx::from_raw(-r)).raw(),
                sin_turns(Fx::from_raw(65_536 - r)).raw()
            );
        }
        assert!(
            worst_ref.0 <= 2,
            "|table - taylor| = {} LSB at raw {}",
            worst_ref.0,
            worst_ref.1
        );
        assert!(
            worst_true.0 < 0.91,
            "|table - sin| = {} LSB at raw {}",
            worst_true.0,
            worst_true.1
        );
        let mut vals = edge_raws();
        vals.extend(random_raws(20_000));
        for &r in &vals {
            let x = Fx::from_raw(r);
            assert_eq!(
                sin_turns(x).raw(),
                sin_turns(Fx::from_raw(r & 0xFFFF)).raw(),
                "wrap {r}"
            );
            let d = (sin_turns(x).raw() - reference_sin_turns(x).raw()).abs();
            assert!(d <= 2, "sin_turns raw {r}: {d} LSB off the reference");
            assert_eq!(
                cos_turns(x).raw(),
                sin_turns(x + Fx::from_raw(1 << 14)).raw()
            );
        }
    }

    /// The guarantees patterns lean on: exact zeros and ±1.0 at the quarter
    /// points, odd symmetry, `cos` exactly `sin` a quarter on, monotone and
    /// in `[-1, 1]` on every quarter.
    #[test]
    fn sin_turns_symmetry_and_exact_points() {
        assert_eq!(sin_turns(Fx::from_raw(0)).raw(), 0);
        assert_eq!(sin_turns(Fx::from_raw(16_384)).raw(), 65_536);
        assert_eq!(sin_turns(Fx::from_raw(32_768)).raw(), 0);
        assert_eq!(sin_turns(Fx::from_raw(49_152)).raw(), -65_536);
        assert_eq!(cos_turns(Fx::ZERO).raw(), 65_536);
        assert_eq!(cos_turns(Fx::from_raw(32_768)).raw(), -65_536);
        let mut prev = 0;
        for r in 0..65_536i32 {
            let s = sin_turns(Fx::from_raw(r)).raw();
            assert!((-65_536..=65_536).contains(&s), "raw {r}: {s}");
            assert_eq!(sin_turns(Fx::from_raw(-r)).raw(), -s, "odd at raw {r}");
            assert_eq!(
                sin_turns(Fx::from_raw(32_768 - r)).raw(),
                s,
                "mirror at raw {r}"
            );
            if (1..=16_384).contains(&r) {
                assert!(s >= prev, "not monotone at raw {r}");
            }
            prev = s;
        }
    }

    /// `SIN_DEV` is exactly its documented formula.
    #[test]
    fn sin_dev_table_matches_its_formula() {
        for (i, &d) in SIN_DEV.iter().enumerate() {
            let want = if i > 256 {
                0
            } else {
                let s = 4.0 * 65_536.0 * (i as f64 * core::f64::consts::PI / 512.0).sin();
                s.round() as i64 - 1024 * i as i64
            };
            assert_eq!(d as i64, want, "SIN_DEV[{i}]");
        }
    }

    /// `rad_to_turns` against the two-step i64 reduction it replaced
    /// (Gitea #938): exhaustive over one period and its neighbours at both
    /// ends of the word, then random over the whole word.
    #[test]
    fn rad_to_turns_matches_the_i64_reduction() {
        let want = |x: i32| -> i32 {
            let r = Fx::from_raw(x).mod_floor(Fx::from_raw(R_PI2_RAW as i32)).raw() as i64;
            ((r << 16) / R_PI2_RAW) as i32
        };
        let d = R_PI2_RAW as i32;
        for r in 0..d {
            assert_eq!(rad_to_turns(r), want(r), "turns of {r}");
            // the same phase a long way up and down the word
            for k in [-5215i32, -1, 1, 2, 5215] {
                let x = r.wrapping_add(k.wrapping_mul(d));
                assert_eq!(rad_to_turns(x), want(x), "turns of {x} (r {r}, k {k})");
            }
        }
        for &x in &edge_raws() {
            assert_eq!(rad_to_turns(x), want(x), "turns of {x}");
        }
        for &x in &random_raws(400_000) {
            assert_eq!(rad_to_turns(x), want(x), "turns of {x}");
        }
    }

    #[test]
    fn sin_matches_the_i64_reference() {
        for r in 0..200_000i32 {
            let x = Fx::from_raw(r);
            assert_eq!(sin(x).raw(), table_sin(x).raw(), "sin raw {r}");
            assert!(
                (sin(x).raw() - reference_sin(x).raw()).abs() <= 2,
                "sin raw {r}"
            );
            let xn = Fx::from_raw(-r);
            assert_eq!(sin(xn).raw(), table_sin(xn).raw(), "sin raw {}", -r);
        }
        let mut vals = edge_raws();
        vals.extend(random_raws(20_000));
        for &r in &vals {
            let x = Fx::from_raw(r);
            assert_eq!(sin(x).raw(), table_sin(x).raw(), "sin raw {r}");
            assert_eq!(
                cos(x).raw(),
                table_sin(x + Fx::from_raw(HALF_PI_RAW)).raw(),
                "cos raw {r}"
            );
            assert_eq!(exp(x).raw(), reference_exp(x).raw(), "exp raw {r}");
        }
    }

    #[test]
    fn sqrt_and_hypot_match_the_i64_reference() {
        for r in -100_000i32..100_000 {
            let x = Fx::from_raw(r);
            assert_eq!(sqrt(x).raw(), reference_sqrt(x).raw(), "sqrt raw {r}");
        }
        let mut vals = edge_raws();
        vals.extend(random_raws(20_000));
        for &r in &vals {
            let x = Fx::from_raw(r);
            assert_eq!(sqrt(x).raw(), reference_sqrt(x).raw(), "sqrt raw {r}");
        }
        // hypot's accumulator is where the i64 wrap lived
        let mut lcg = Lcg::new();
        let mut pairs: Vec<(i32, i32)> = Vec::new();
        for &a in &edge_raws() {
            for &b in &[0i32, 1, -1, 65_536, -65_536, i32::MAX, i32::MIN, 13_107_200] {
                pairs.push((a, b));
            }
        }
        for _ in 0..20_000 {
            pairs.push((lcg.next() as i32, lcg.next() as i32));
        }
        for &(a, b) in &pairs {
            let (fa, fb) = (Fx::from_raw(a), Fx::from_raw(b));
            assert_eq!(
                hypot(fa, fb).raw(),
                reference_hypot_raw(&[fa, fb]).raw(),
                "hypot {a},{b}"
            );
            let fc = Fx::from_raw(a.wrapping_sub(b));
            assert_eq!(
                hypot3(fa, fb, fc).raw(),
                reference_hypot_raw(&[fa, fb, fc]).raw(),
                "hypot3 {a},{b}"
            );
        }
    }

    #[test]
    fn arctangents_match_the_i64_reference() {
        let mut vals = edge_raws();
        vals.extend(random_raws(3_000));
        for r in (-200_000i32..200_000).step_by(37) {
            vals.push(r);
        }
        for &r in &vals {
            let x = Fx::from_raw(r);
            assert_eq!(atan(x).raw(), reference_atan(x).raw(), "atan raw {r}");
            assert_eq!(asin(x).raw(), reference_asin(x).raw(), "asin raw {r}");
        }
        // atan2 needs both quadrant and |y|<=|x| / |y|>|x| coverage
        let mut pool: Vec<i32> = edge_raws();
        pool.extend(random_raws(60).into_iter());
        for k in 0..12 {
            pool.push(1 << (2 * k));
            pool.push(-(1 << (2 * k)));
        }
        for &a in &pool {
            for &b in &pool {
                let (fa, fb) = (Fx::from_raw(a), Fx::from_raw(b));
                assert_eq!(
                    atan2(fa, fb).raw(),
                    reference_atan2(fa, fb).raw(),
                    "atan2 {a},{b}"
                );
            }
        }
    }

    fn reference_pow(base: Fx, e: Fx) -> Fx {
        if e == Fx::ZERO {
            return Fx::ONE;
        }
        if base.raw() == 0 {
            return Fx::ZERO;
        }
        if base.raw() < 0 {
            if e.frac() != Fx::ZERO {
                return Fx::MIN;
            }
            let mag = reference_exp2(e * reference_log2(-base));
            if mag == Fx::MAX && e.to_int_trunc() & 1 == 1 {
                return Fx::MIN;
            }
            return if e.to_int_trunc() & 1 == 1 { -mag } else { mag };
        }
        reference_exp2(e * reference_log2(base))
    }

    /// The composed entry points (`pow`, `tan`, `acos`, `cos_turns`) on top
    /// of the rewritten primitives — this is the end-to-end pin, since
    /// `pow` is where the ten `__udivdi3` calls lived.
    #[test]
    fn composed_builtins_match_the_i64_reference() {
        let mut vals = edge_raws();
        vals.extend(random_raws(2_000));
        for r in (-400_000i32..400_000).step_by(911) {
            vals.push(r);
        }
        for &r in &vals {
            let x = Fx::from_raw(r);
            assert_eq!(
                tan(x).raw(),
                (table_sin(x) / table_sin(x + Fx::from_raw(HALF_PI_RAW))).raw(),
                "tan raw {r}"
            );
            assert_eq!(
                acos(x).raw(),
                (Fx::from_raw(HALF_PI_RAW) - reference_asin(x)).raw(),
                "acos raw {r}"
            );
            assert_eq!(
                cos_turns(x).raw(),
                sin_turns(x + Fx::from_raw(1 << 14)).raw(),
                "cos_turns raw {r}"
            );
        }
        // pow: bases and exponents across the sign/saturation branches
        let mut bases: Vec<i32> = edge_raws();
        bases.extend(random_raws(120));
        let mut exps: Vec<i32> = edge_raws();
        exps.extend(random_raws(60));
        for e in -40i32..=40 {
            exps.push(e << 16);
            exps.push((e << 16) | 0x8000);
            exps.push((e << 16) + 1);
        }
        for &b in &bases {
            for &e in &exps {
                let (fb, fe) = (Fx::from_raw(b), Fx::from_raw(e));
                // Integer exponents 1..=POW_INT_MAX are the repeated
                // multiplication of #938, pinned by `pow_int_is_repeated_
                // multiplication` below; every other exponent is still the
                // log/exp route and must match the i64 form.
                if e & 0xFFFF == 0 && (1..=POW_INT_MAX).contains(&(e >> 16)) {
                    continue;
                }
                assert_eq!(
                    pow(fb, fe).raw(),
                    reference_pow(fb, fe).raw(),
                    "pow {b},{e}"
                );
            }
        }
    }

    /// Gitea #938: a small positive integer exponent is the product chain,
    /// exactly — `pow(x, 1) == x`, `pow(x, 2) == x * x`, whole powers of
    /// whole numbers are whole — with the log/exp route's sign rule and
    /// saturation, and never further from the real power than that route
    /// was.
    #[test]
    fn pow_int_is_repeated_multiplication() {
        let mut bases: Vec<i32> = edge_raws();
        bases.extend(random_raws(2_000));
        for &b in &bases {
            let fb = Fx::from_raw(b);
            assert_eq!(pow(fb, Fx::ONE), fb, "pow({b}, 1)");
            let mag = b.unsigned_abs() as u64;
            // `x * x` wraps where `pow` saturates, so the identity holds
            // below the square's overflow (|x| < 181.02)
            if (mag * mag) >> 16 <= i32::MAX as u64 {
                assert_eq!(pow(fb, Fx::from_int(2)), fb * fb, "pow({b}, 2)");
            }
            // the chain of magnitudes, sign by parity, saturating
            for n in 1..=POW_INT_MAX as u32 {
                let mut acc = mag;
                let mut sat = false;
                for _ in 1..n {
                    acc = (acc * mag) >> 16;
                    if acc > i32::MAX as u64 {
                        sat = true;
                        break;
                    }
                }
                let neg = b < 0 && n & 1 == 1;
                let want = if sat {
                    if neg {
                        Fx::MIN
                    } else {
                        Fx::MAX
                    }
                } else if neg {
                    Fx::from_raw((acc as i32).wrapping_neg())
                } else {
                    Fx::from_raw(acc as i32)
                };
                assert_eq!(pow(fb, Fx::from_int(n as i32)), want, "pow({b}, {n})");
            }
        }
        // whole powers of small whole numbers are exact
        for b in -12i32..=12 {
            for n in 1..=POW_INT_MAX {
                let exact = (b as i64).pow(n as u32);
                let want = if exact > 32_767 {
                    Fx::MAX
                } else if exact < -32_768 {
                    Fx::MIN
                } else {
                    Fx::from_int(exact as i32)
                };
                assert_eq!(pow(Fx::from_int(b), Fx::from_int(n)), want, "pow({b}, {n})");
            }
        }
        // and on sub-unit bases the chain sits within its truncation bound
        // of the real power: every step floors, so the result is never
        // above it and at most (n−1) LSBs below (checked against f64)
        let mut lcg = Lcg::new();
        for _ in 0..20_000 {
            let b = (lcg.next() & 0xFFFF) as i32; // [0, 1)
            let n = 2 + (lcg.next() % (POW_INT_MAX as u32 - 1)) as i32;
            let (fb, fe) = (Fx::from_raw(b), Fx::from_int(n));
            let real = (b as f64 / 65536.0).powi(n);
            let got = pow(fb, fe).to_f64();
            let ulp = 1.0 / 65536.0;
            assert!(
                got <= real + 1e-12 && got >= real - (n as f64) * ulp,
                "pow({b}, {n}): chain {got} vs real {real}"
            );
        }
        // the exponent ONE past the cap is still the log/exp route
        let over = Fx::from_int(POW_INT_MAX + 1);
        assert_eq!(pow(Fx::from_f64(1.5), over), reference_pow(Fx::from_f64(1.5), over));
    }

    // ---------------------------------------------------------------
    // Behavioural tests (unchanged).
    // ---------------------------------------------------------------

    #[test]
    fn sin_cos_basics() {
        assert_eq!(sin(Fx::ZERO), Fx::ZERO);
        assert_close(sin(Fx::from_f64(core::f64::consts::FRAC_PI_2)), 1.0, 3e-4);
        assert_close(sin(Fx::from_f64(core::f64::consts::PI)), 0.0, 3e-4);
        assert_close(sin(Fx::from_f64(1.0)), 0.8414709848, 3e-4);
        assert_close(sin(Fx::from_f64(-1.0)), -0.8414709848, 3e-4);
        assert_close(cos(Fx::ZERO), 1.0, 3e-4);
        assert_close(cos(Fx::from_f64(1.0)), 0.5403023059, 3e-4);
    }

    #[test]
    fn sin_turns_quadrants() {
        assert_eq!(sin_turns(Fx::ZERO), Fx::ZERO);
        assert_close(sin_turns(Fx::from_f64(0.25)), 1.0, 3e-4);
        assert_close(sin_turns(Fx::from_f64(0.5)), 0.0, 3e-4);
        assert_close(sin_turns(Fx::from_f64(0.75)), -1.0, 3e-4);
        // negative phases wrap backward
        assert_close(sin_turns(Fx::from_f64(-0.25)), -1.0, 3e-4);
    }

    #[test]
    fn sqrt_and_hypot() {
        assert_eq!(sqrt(Fx::from_int(4)), Fx::from_int(2));
        assert_close(sqrt(Fx::from_int(2)), core::f64::consts::SQRT_2, 1e-4);
        assert_eq!(sqrt(Fx::from_int(-4)), Fx::from_int(-2)); // sign-preserving (oracle)
        assert_close(hypot(Fx::from_int(3), Fx::from_int(4)), 5.0, 1e-3);
        assert_close(
            hypot3(Fx::from_int(1), Fx::from_int(2), Fx::from_int(2)),
            3.0,
            1e-3,
        );
        // the sum of squares wraps into the 16.16 domain (oracle: hypotbig
        // reads ≈120.266 on hardware, not 282.84). Exact raw differs by
        // +3 ulps on PB — its sqrt has a small positive bias; reversing that
        // algorithm is pending the sweep probes.
        assert_eq!(hypot(Fx::from_int(200), Fx::from_int(200)).raw(), 7_881_776);
    }

    #[test]
    fn exp_log_pow() {
        assert_close(exp2(Fx::from_int(3)), 8.0, 1e-3);
        assert_close(exp2(Fx::from_f64(0.5)), core::f64::consts::SQRT_2, 1e-3);
        assert_close(exp2(Fx::from_int(-2)), 0.25, 1e-3);
        assert_close(log2(Fx::from_int(8)), 3.0, 1e-3);
        assert_close(log2(Fx::from_f64(0.5)), -1.0, 1e-3);
        assert_eq!(log2(Fx::ZERO), Fx::MIN);
        assert_close(ln(Fx::from_f64(core::f64::consts::E)), 1.0, 2e-3);
        assert_close(exp(Fx::ONE), core::f64::consts::E, 5e-3);
        assert_close(pow(Fx::from_int(2), Fx::from_int(10)), 1024.0, 0.5);
        assert_close(pow(Fx::from_int(9), Fx::from_f64(0.5)), 3.0, 5e-3);
        // negative bases: sign rule for integer exponents (oracle)
        assert_close(pow(Fx::from_int(-2), Fx::from_int(2)), 4.0, 1e-2);
        assert_close(pow(Fx::from_int(-2), Fx::from_int(3)), -8.0, 3e-2);
        // negative base, fractional exponent: raw 0x80000000, like the PB
        // (whose float path shows it as +32768; oracle pow_neg2_half)
        assert_eq!(pow(Fx::from_int(-2), Fx::from_f64(2.5)), Fx::MIN);
        // Overflow saturates, PB-exact (oracle 2026-08-23, fw 3.67):
        // positive to raw 0x7FFFFFFF, negative-odd to raw 0x80000000.
        assert_eq!(pow(Fx::from_int(2), Fx::from_int(16)), Fx::MAX);
        assert_eq!(pow(Fx::from_int(2), Fx::from_int(15)), Fx::MAX);
        assert_eq!(pow(Fx::from_int(2), Fx::from_f64(15.5)), Fx::MAX);
        assert_eq!(pow(Fx::from_int(10), Fx::from_int(10)), Fx::MAX);
        assert_eq!(exp2(Fx::from_int(20)), Fx::MAX);
        assert_eq!(pow(Fx::from_int(-2), Fx::from_int(17)), Fx::MIN);
        assert_eq!(pow(Fx::from_int(-2), Fx::from_int(16)), Fx::MAX);
        assert_eq!(pow(Fx::from_int(5), Fx::ZERO), Fx::ONE);
        assert_eq!(pow(Fx::ZERO, Fx::ZERO), Fx::ONE); // oracle: pow_0_0
    }

    #[test]
    fn arctangents() {
        assert_close(atan(Fx::ONE), core::f64::consts::FRAC_PI_4, 2e-3);
        assert_close(atan(Fx::from_int(100)), 1.5607966, 2e-3);
        assert_close(atan(Fx::from_int(-1)), -core::f64::consts::FRAC_PI_4, 2e-3);
        assert_close(atan2(Fx::ONE, Fx::ONE), core::f64::consts::FRAC_PI_4, 2e-3);
        use core::f64::consts::{FRAC_PI_2, FRAC_PI_3, FRAC_PI_6, PI};
        assert_close(atan2(Fx::ONE, -Fx::ONE), 3.0 * PI / 4.0, 3e-3);
        assert_close(atan2(-Fx::ONE, -Fx::ONE), -3.0 * PI / 4.0, 3e-3);
        assert_close(atan2(Fx::ONE, Fx::ZERO), FRAC_PI_2, 2e-3);
        assert_eq!(atan2(Fx::ZERO, Fx::ZERO), Fx::ZERO);
        assert_close(asin(Fx::ONE), FRAC_PI_2, 5e-3);
        assert_close(asin(Fx::from_f64(0.5)), FRAC_PI_6, 5e-3);
        assert_close(acos(Fx::from_f64(0.5)), FRAC_PI_3, 5e-3);
    }
}
