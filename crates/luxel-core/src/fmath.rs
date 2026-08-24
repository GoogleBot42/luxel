//! Deterministic fixed-point transcendentals for the Luxel VM.
//!
//! Everything here is integer-only (i64 intermediates on 16-frac raws), so
//! results are bit-identical on ESP32, wasm32, and native — that determinism
//! is a project requirement. Accuracy targets LED work (errors well under
//! one 8-bit color step), not scientific computing.
//!
//! Pixel Blaze's exact algorithms for these functions are not public. All
//! of them have been differential-tested against real hardware via dense
//! sweeps (docs/research/04-oracle-findings.md): ours are equal or closer
//! to true math everywhere measured; remaining diffs are PB-side
//! approximation error and its documented seam/endpoint bugs — deliberate
//! divergences, not open questions.

use crate::fixed::Fx;

// Raw 16.16 constants (value * 65536, rounded).
const PI_RAW: i64 = 205_887; // π
const PI2_RAW: i64 = 411_775; // 2π
const HALF_PI_RAW: i64 = 102_944; // π/2
const LOG2E_RAW: i64 = 94_548; // 1/ln 2
const LN2_RAW: i64 = 45_426; // ln 2

/// 16-frac multiply on i64 raws (truncating like the VM's `*`).
#[inline]
fn fmul(a: i64, b: i64) -> i64 {
    (a * b) >> 16
}

/// sin of a phase in *turns* (1.0 = full cycle). The waveform functions are
/// cos in turns: cos(t) = sin(t + 1/4).
pub fn cos_turns(t: Fx) -> Fx {
    sin_turns(t + Fx::from_raw(1 << 14))
}

/// specified in turns, so this is the core primitive; radian `sin` reduces
/// into it.
pub fn sin_turns(t: Fx) -> Fx {
    // wrap to [0, 1)
    let t = t.mod_floor(Fx::ONE).raw() as i64;
    // fold to a quarter wave
    let (t, neg) = if t >= 32_768 {
        (t - 32_768, true)
    } else {
        (t, false)
    };
    let t = if t >= 16_384 { 32_768 - t } else { t };
    // z = t·2π ∈ [0, π/2]
    let z = fmul(t, PI2_RAW);
    // Taylor: z - z³/6 + z⁵/120 - z⁷/5040 + z⁹/362880 (error < 3e-6 on [0,π/2])
    let z2 = fmul(z, z);
    let z3 = fmul(z2, z);
    let z5 = fmul(z3, z2);
    let z7 = fmul(z5, z2);
    let z9 = fmul(z7, z2);
    // truncating fmuls can overshoot ±1.0 by an ulp or two near the peak
    let s = (z - z3 / 6 + z5 / 120 - z7 / 5040 + z9 / 362_880).min(65_536);
    Fx::from_raw(if neg { -s } else { s } as i32)
}

/// sin(x), x in radians.
pub fn sin(x: Fx) -> Fx {
    // reduce mod 2π first (better precision than multiplying large x by 1/2π)
    let r = x.mod_floor(Fx::from_raw(PI2_RAW as i32)).raw() as i64;
    // to turns: r / 2π
    let turns = ((r << 16) / PI2_RAW) as i32;
    sin_turns(Fx::from_raw(turns))
}

/// cos(x), x in radians.
pub fn cos(x: Fx) -> Fx {
    sin(x + Fx::from_raw(HALF_PI_RAW as i32))
}

/// tan(x) = sin/cos; where cos is 0 the VM's x/0 = 0 rule applies.
pub fn tan(x: Fx) -> Fx {
    sin(x) / cos(x)
}

/// Floor square root, sign-preserving: `sqrt(-4) == -2`. Oracle-confirmed
/// on fw 3.67 (this is PB's documented "square root returns negative" quirk).
pub fn sqrt(x: Fx) -> Fx {
    let mag = isqrt64((x.raw().unsigned_abs() as u64) << 16) as i32;
    Fx::from_raw(if x.raw() < 0 { -mag } else { mag })
}

fn isqrt64(n: u64) -> u32 {
    // classic bitwise integer square root
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

/// hypot: squares summed at full precision, then the SUM wraps into the
/// 16.16 domain before the (sign-preserving) sqrt. Oracle-confirmed:
/// `hypot(200, 200) == 120.266…` on real hardware (80000 wraps to 14464).
pub fn hypot(x: Fx, y: Fx) -> Fx {
    hypot_raw(&[x, y])
}

pub fn hypot3(x: Fx, y: Fx, z: Fx) -> Fx {
    hypot_raw(&[x, y, z])
}

fn hypot_raw(vs: &[Fx]) -> Fx {
    let mut sum: i64 = 0;
    for v in vs {
        let r = v.raw() as i64;
        sum = sum.wrapping_add((r * r) >> 16);
    }
    sqrt(Fx::from_raw(sum as i32))
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
pub fn exp2(x: Fx) -> Fx {
    let n = x.to_int_floor();
    let f = (x - Fx::from_int(n)).raw() as i64; // [0, 1) as 16-frac
    const LN2_32: i64 = 2_977_044_472; // round(ln2 · 2^32)
    let y = (f * LN2_32) >> 16; // 32-frac, ≤ ln2
    let mul32 = |a: i64, b: i64| ((a as i128 * b as i128) >> 32) as i64;
    let y2 = mul32(y, y);
    let y3 = mul32(y2, y);
    let y4 = mul32(y3, y);
    let y5 = mul32(y4, y);
    let y6 = mul32(y5, y);
    let y7 = mul32(y6, y);
    let m32 =
        (1i64 << 32) + y + y2 / 2 + y3 / 6 + y4 / 24 + y5 / 120 + y6 / 720 + y7 / 5040;
    let m = ((m32 + (1 << 15)) >> 16) as i32; // round to 16-frac, [1, 2]
    if n >= 0 {
        // m >= 2^16, so any n >= 15 lands at or past 2^31; check the rest
        // in i64. Saturate, per the doc comment above.
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

/// log2(x); x ≤ 0 yields the most-negative value (oracle-verified exact:
/// log2_0/log2_neg both return raw i32::MIN on the PB).
pub fn log2(x: Fx) -> Fx {
    if x.raw() <= 0 {
        return Fx::MIN;
    }
    let raw = x.raw() as u64;
    let msb = 63 - raw.leading_zeros() as i32;
    let int_part = msb - 16;
    // normalize mantissa to [1, 2) as 16-frac
    let mut m = if msb > 16 {
        raw >> (msb - 16)
    } else {
        raw << (16 - msb)
    };
    // 16 fraction bits by repeated squaring
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

/// Natural log via log2.
pub fn ln(x: Fx) -> Fx {
    if x.raw() <= 0 {
        return Fx::MIN;
    }
    Fx::from_raw(fmul(log2(x).raw() as i64, LN2_RAW) as i32)
}

/// e^x.
pub fn exp(x: Fx) -> Fx {
    exp2(Fx::from_raw(fmul(x.raw() as i64, LOG2E_RAW) as i32))
}

/// pow(base, exp) = 2^(exp·log2 base). Oracle-confirmed: pow(x, 0) == 1
/// (including 0^0), and negative bases work with integer exponents with the
/// usual sign rule (pow(-2, 3) == -8). Negative base with a fractional
/// exponent yields Fx::MIN — the PB does log2(negative) = MIN and lets it
/// propagate (oracle-verified 2026-07-07, pow_neg2_half/pow_neg2_15).
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
    let raw = x.raw() as i64;
    if raw.abs() <= 65_536 {
        atan_unit(raw)
    } else {
        // atan(x) = sign·π/2 − atan(1/x)
        let recip = (1i64 << 32) / raw; // 16-frac 1/x, |x|>1 so |recip|≤1
        let base = atan_unit(recip);
        let half = if raw > 0 { HALF_PI_RAW } else { -HALF_PI_RAW };
        Fx::from_raw((half - base.raw() as i64) as i32)
    }
}

/// atan on |z| ≤ 1 (raw 16-frac in, Fx out).
fn atan_unit(z: i64) -> Fx {
    // Hastings: atan(z) ≈ z(A + B z² + C z⁴ + D z⁶ + E z⁸), err ≈ 1e-4 rad
    const A: i64 = 65_527; // 0.9998660
    const B: i64 = -21_647; // -0.3302995
    const C: i64 = 11_807; // 0.1801410
    const D: i64 = -5_580; // -0.0851330
    const E: i64 = 1_366; // 0.0208351
    let z2 = fmul(z, z);
    let p = A + fmul(z2, B + fmul(z2, C + fmul(z2, D + fmul(z2, E))));
    Fx::from_raw(fmul(z, p) as i32)
}

/// atan2(y, x) with the usual quadrant conventions; atan2(0, 0) = 0.
pub fn atan2(y: Fx, x: Fx) -> Fx {
    let (yr, xr) = (y.raw() as i64, x.raw() as i64);
    if xr == 0 && yr == 0 {
        return Fx::ZERO;
    }
    if xr == 0 {
        return Fx::from_raw(if yr > 0 { HALF_PI_RAW } else { -HALF_PI_RAW } as i32);
    }
    if yr == 0 {
        return Fx::from_raw(if xr > 0 { 0 } else { PI_RAW } as i32);
    }
    // pick the ratio with |·| ≤ 1 to stay in the polynomial's sweet spot
    let a = if yr.abs() <= xr.abs() {
        let base = atan_unit((yr << 16) / xr);
        if xr > 0 {
            base.raw() as i64
        } else if yr > 0 {
            base.raw() as i64 + PI_RAW
        } else {
            base.raw() as i64 - PI_RAW
        }
    } else {
        let base = atan_unit((xr << 16) / yr);
        if yr > 0 {
            HALF_PI_RAW - base.raw() as i64
        } else {
            -HALF_PI_RAW - base.raw() as i64
        }
    };
    Fx::from_raw(a as i32)
}

/// asin(x), inputs clamped to [-1, 1].
pub fn asin(x: Fx) -> Fx {
    let x = x.clamp(-Fx::ONE, Fx::ONE);
    // asin(x) = atan2(x, sqrt(1 − x²)); 1−x² in i64 to dodge the x² wrap
    let xr = x.raw() as i64;
    let one_minus = (1i64 << 32) - xr * xr; // 32-frac
    let root = isqrt64(one_minus as u64) as i32; // back to 16-frac
    atan2(x, Fx::from_raw(root))
}

/// acos(x) = π/2 − asin(x).
pub fn acos(x: Fx) -> Fx {
    Fx::from_raw(HALF_PI_RAW as i32) - asin(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: Fx, expected: f64, tol: f64) {
        let a = actual.to_f64();
        assert!(
            (a - expected).abs() < tol,
            "expected ≈{expected}, got {a} (tol {tol})"
        );
    }

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
