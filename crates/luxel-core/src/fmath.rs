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
//! Three of them (`div_shift16`, `isqrt48`, `sq16`) trade one wide machine
//! instruction for a 32-bit loop, which is a *loss* on hosts that do have a
//! 64-bit ALU — including wasm32, which the web playground runs on. Those
//! keep both forms and pick at compile time on [`NARROW_WORD`]; both are
//! compiled everywhere and the tests assert `narrow == wide` directly, so a
//! host test run still proves the device path bit-exact.
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

/// Narrow form: restoring binary division — one pre-step for the (at most
/// 1) integer part plus 16 quotient bits. The remainder is always
/// `< d <= 2^31`, so `r << 1` stays inside u32 and the whole thing runs in
/// 32-bit registers, where [`div_shift16_wide`] compiles to a
/// `__divdi3`/`__udivdi3` ROM call on a 35–48-bit dividend.
#[inline]
fn div_shift16_narrow(num: u32, d: u32) -> u32 {
    debug_assert!(d != 0 && num <= d && d <= 1u32 << 31);
    let mut r = num;
    let mut q: u32 = 0;
    if r >= d {
        r -= d;
        q = 1;
    }
    let mut i = 0;
    while i < 16 {
        // r < d <= 2^31, so this shift never drops a bit; the numerator's
        // low 16 bits are all zero, so nothing is shifted in.
        r <<= 1;
        q <<= 1;
        if r >= d {
            r -= d;
            q |= 1;
        }
        i += 1;
    }
    q
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
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn sin_turns(t: Fx) -> Fx {
    // wrap to [0, 1)
    let t = t.mod_floor(Fx::ONE).raw();
    // fold to a quarter wave: t ∈ [0, 16384]
    let (t, neg) = if t >= 32_768 {
        (t - 32_768, true)
    } else {
        (t, false)
    };
    let t = if t >= 16_384 { 32_768 - t } else { t };
    // z = t·2π ∈ [0, π/2]. t <= 16384 = 2^14 and PI2_RAW < 2^19, so the
    // widening product is < 2^33 and z = (t·2π)>>16 <= 102943 < 2^17.
    let z = fmul32(t, PI2_RAW);
    // Taylor: z - z³/6 + z⁵/120 - z⁷/5040 + z⁹/362880 (error < 3e-6 on [0,π/2])
    //
    // Every power is bounded by its real value times 2^16 (each fmul32 only
    // truncates downward): z2 <= 161709 (2.4675·2^16), z3 <= 254004,
    // z5 <= 626433, z7 <= 1545300, z9 <= 3811700 < 2^22. So all of them fit
    // i32 with ~9 bits to spare and the >>16 in fmul32 never truncates a
    // significant bit — the i64 form computed exactly these values.
    let z2 = fmul32(z, z);
    let z3 = fmul32(z2, z);
    let z5 = fmul32(z3, z2);
    let z7 = fmul32(z5, z2);
    let z9 = fmul32(z7, z2);
    // z >= 0 and every step is `>>16` of a nonnegative product, so all z_k
    // are nonnegative: the unsigned divides are bit-identical to the signed
    // i64 ones (which truncate toward zero == floor here) and compile to a
    // 32-bit magic multiply instead of `__divdi3`.
    // truncating fmuls can overshoot ±1.0 by an ulp or two near the peak
    let s = (z - (z3 as u32 / 6) as i32 + (z5 as u32 / 120) as i32
        - (z7 as u32 / 5_040) as i32
        + (z9 as u32 / 362_880) as i32)
        .min(65_536);
    Fx::from_raw(if neg { -s } else { s })
}

/// sin(x), x in radians.
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn sin(x: Fx) -> Fx {
    // reduce mod 2π first (better precision than multiplying large x by 1/2π)
    let r = x.mod_floor(Fx::from_raw(PI2_RAW)).raw();
    // to turns: r / 2π. `mod_floor` with a positive divisor gives
    // r ∈ [0, PI2_RAW), so `((r as i64) << 16) / PI2_RAW` is a nonnegative
    // quotient < 65536 — exactly what `div_shift16` computes, without the
    // 35-bit dividend that forced `__udivdi3`.
    let turns = div_shift16(r as u32, PI2_RAW as u32) as i32;
    sin_turns(Fx::from_raw(turns))
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

/// Exact `floor(sqrt(n))` for `n < 2^48` (both call sites are bounded:
/// `sqrt` feeds `|raw| << 16 < 2^47`, `asin` feeds `<= 2^32`).
///
/// Deliberately NOT `#[inline]`, on any of the three functions here.
/// Marking them inline lets `sqrt` be pulled into `hypot`/`hypot3`/`asin`
/// and the VM's builtin dispatch, and the resulting code growth cost −17 %
/// on `dire-spider-2d` (6 sin + 3 hypot + 2 cos + 1 atan2 per pixel) while
/// buying nothing on `crosstown-traffic-2d` (24 `dist` per pixel) — both
/// measured with `luxel bench`, 512 px x 400 frames, best of 9-12
/// interleaved rounds, 2026-09-06. See [`NARROW_WORD`].
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
fn isqrt48(n: u64) -> u32 {
    if NARROW_WORD {
        isqrt48_narrow(n)
    } else {
        isqrt48_wide(n)
    }
}

/// Wide form: the classic bitwise integer square root on 64-bit words —
/// one compare / add / subtract per iteration on any target with a 64-bit
/// ALU, and it skips all the leading zero digit-pairs up front. This is the
/// original implementation, unchanged.
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
/// subtracts (3–4 Xtensa instructions each) for ~24 iterations.
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
#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn pow(base: Fx, e: Fx) -> Fx {
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

    fn reference_sin(x: Fx) -> Fx {
        let r = x.mod_floor(Fx::from_raw(R_PI2_RAW as i32)).raw() as i64;
        let turns = ((r << 16) / R_PI2_RAW) as i32;
        reference_sin_turns(Fx::from_raw(turns))
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

    #[test]
    fn sin_turns_matches_the_i64_reference() {
        // sin_turns only sees the low 16 bits after mod_floor, so the whole
        // 65536-wide phase space is swept exhaustively here.
        for r in 0..65_536i32 {
            let x = Fx::from_raw(r);
            assert_eq!(
                sin_turns(x).raw(),
                reference_sin_turns(x).raw(),
                "sin_turns raw {r}"
            );
            let xn = Fx::from_raw(-r);
            assert_eq!(
                sin_turns(xn).raw(),
                reference_sin_turns(xn).raw(),
                "sin_turns raw {}",
                -r
            );
        }
        let mut vals = edge_raws();
        vals.extend(random_raws(20_000));
        for &r in &vals {
            let x = Fx::from_raw(r);
            assert_eq!(
                sin_turns(x).raw(),
                reference_sin_turns(x).raw(),
                "sin_turns raw {r}"
            );
            assert_eq!(cos_turns(x).raw(), reference_sin_turns(x + Fx::from_raw(1 << 14)).raw());
        }
    }

    #[test]
    fn sin_matches_the_i64_reference() {
        for r in 0..200_000i32 {
            let x = Fx::from_raw(r);
            assert_eq!(sin(x).raw(), reference_sin(x).raw(), "sin raw {r}");
            let xn = Fx::from_raw(-r);
            assert_eq!(sin(xn).raw(), reference_sin(xn).raw(), "sin raw {}", -r);
        }
        let mut vals = edge_raws();
        vals.extend(random_raws(20_000));
        for &r in &vals {
            let x = Fx::from_raw(r);
            assert_eq!(sin(x).raw(), reference_sin(x).raw(), "sin raw {r}");
            assert_eq!(
                cos(x).raw(),
                reference_sin(x + Fx::from_raw(HALF_PI_RAW)).raw(),
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
                (reference_sin(x) / reference_sin(x + Fx::from_raw(HALF_PI_RAW))).raw(),
                "tan raw {r}"
            );
            assert_eq!(
                acos(x).raw(),
                (Fx::from_raw(HALF_PI_RAW) - reference_asin(x)).raw(),
                "acos raw {r}"
            );
            assert_eq!(
                cos_turns(x).raw(),
                reference_sin_turns(x + Fx::from_raw(1 << 14)).raw(),
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
                assert_eq!(
                    pow(fb, fe).raw(),
                    reference_pow(fb, fe).raw(),
                    "pow {b},{e}"
                );
            }
        }
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
