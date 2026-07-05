//! `Fx`: the single scalar type of the Luxel VM — 16.16 two's-complement
//! fixed point, semantics-compatible with the Pixel Blaze pattern language.
//!
//! Documented behaviors this type must preserve (patterns observe them):
//! - Range ±32768 with resolution 1/65536; **overflow wraps** (two's
//!   complement), it does not saturate. `182 * 182` goes negative.
//! - Bitwise `| & ^ << >>` operate on the **full 32-bit word** including the
//!   16 fraction bits (so `x | 0` does NOT truncate, `x << 1 == x * 2`).
//! - `~` is the exception: it zeros the low 16 bits of the result.
//! - `%` is truncated remainder (sign of dividend): `-3.5 % 3 == -0.5`.
//!   The `mod()` builtin is floored (sign of divisor): `mod(-3.5, 3) == 2.5`.
//! - `floor(-5.1) == -6`, `ceil(-5.9) == -5`, but `frac(-5.5) == -0.5` and
//!   `trunc(-5.9) == -5` (toward zero).
//!
//! Literals are 31-bit (16.15): the raw value is truncated toward zero and
//! its least-significant fraction bit cleared, because the PB VM steals that
//! bit as an instruction flag. Confirmed against real hardware (fw 3.67,
//! oracle vectors lit_epsilon/multrunc/tri6 and the constants table) — we
//! match exactly so that patterns tuned on PB render identically.

use core::fmt;
use core::ops::{Add, BitAnd, BitOr, BitXor, Div, Mul, Neg, Not, Rem, Shl, Shr, Sub};

pub const FRAC_BITS: u32 = 16;
const FRAC_MASK: i32 = 0xFFFF;
const ONE_RAW: i32 = 1 << FRAC_BITS;

/// 16.16 fixed-point number. Ordering/equality on the raw word is numeric
/// ordering (two's complement), so the derives are correct.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fx(i32);

impl Fx {
    pub const ZERO: Fx = Fx(0);
    pub const ONE: Fx = Fx(ONE_RAW);
    pub const MIN: Fx = Fx(i32::MIN); // -32768.0
    pub const MAX: Fx = Fx(i32::MAX); // 32767.99998...
    /// Smallest representable step, 1/65536.
    pub const EPSILON: Fx = Fx(1);

    #[inline]
    pub const fn from_raw(raw: i32) -> Fx {
        Fx(raw)
    }

    #[inline]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Integer → fixed, wrapping like every other overflow (`from_int(32768)`
    /// is `-32768.0`).
    #[inline]
    pub const fn from_int(v: i32) -> Fx {
        Fx(v.wrapping_shl(FRAC_BITS))
    }

    /// Host-boundary conversion (websocket vars, preview, tests). Rounds to
    /// nearest; saturates at the ends. Not reachable from pattern code.
    pub fn from_f64(v: f64) -> Fx {
        let scaled = v * 65536.0;
        let rounded = if scaled >= 0.0 {
            scaled + 0.5
        } else {
            scaled - 0.5
        };
        Fx(rounded as i32) // `as` saturates on overflow/NaN→0
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64 / 65536.0
    }

    #[inline]
    pub const fn is_truthy(self) -> bool {
        self.0 != 0
    }

    /// True floor: `floor(-5.1) == -6`.
    #[inline]
    pub const fn floor(self) -> Fx {
        Fx(self.0 & !FRAC_MASK)
    }

    /// True ceil: `ceil(-5.9) == -5`. Wraps near MAX like everything else.
    #[inline]
    pub const fn ceil(self) -> Fx {
        Fx(self.0.wrapping_add(FRAC_MASK) & !FRAC_MASK)
    }

    /// Toward zero: `trunc(-5.9) == -5`.
    #[inline]
    pub const fn trunc(self) -> Fx {
        Fx(self.0.wrapping_sub(self.0.wrapping_rem(ONE_RAW)))
    }

    /// Fractional part, sign of the value: `frac(-5.5) == -0.5`.
    #[inline]
    pub const fn frac(self) -> Fx {
        Fx(self.0.wrapping_rem(ONE_RAW))
    }

    /// Round half toward +∞: `floor(x + 0.5)`. Oracle-confirmed on fw 3.67:
    /// `round(-2.5) == -2`, `round(-0.5) == 0`.
    #[inline]
    pub const fn round(self) -> Fx {
        Fx(self.0.wrapping_add(ONE_RAW / 2) & !FRAC_MASK)
    }

    #[inline]
    pub const fn abs(self) -> Fx {
        Fx(self.0.wrapping_abs())
    }

    /// Integer part, truncated toward zero.
    #[inline]
    pub const fn to_int_trunc(self) -> i32 {
        self.0.wrapping_div(ONE_RAW)
    }

    /// Integer part, floored (arithmetic shift).
    #[inline]
    pub const fn to_int_floor(self) -> i32 {
        self.0 >> FRAC_BITS
    }

    /// The `mod()` builtin: floored modulo, result takes the sign of the
    /// divisor — this is the wrapping semantic the waveform functions use.
    /// `mod(x, 0)` is 0. TODO(oracle): verify the zero-divisor case.
    pub fn mod_floor(self, rhs: Fx) -> Fx {
        if rhs.0 == 0 {
            return Fx::ZERO;
        }
        let m = self.0.wrapping_rem(rhs.0);
        if m != 0 && (m < 0) != (rhs.0 < 0) {
            Fx(m.wrapping_add(rhs.0))
        } else {
            Fx(m)
        }
    }

    pub fn min(self, other: Fx) -> Fx {
        if self <= other {
            self
        } else {
            other
        }
    }

    pub fn max(self, other: Fx) -> Fx {
        if self >= other {
            self
        } else {
            other
        }
    }

    pub fn clamp(self, lo: Fx, hi: Fx) -> Fx {
        self.max(lo).min(hi)
    }

    /// Quantize a host float exactly the way pattern literals are quantized:
    /// truncate toward zero, clear the LSB (16.15). Used for the predefined
    /// constants, which PB encodes as literals (oracle: PI is 205886 raw,
    /// not the correctly-rounded 205887).
    pub fn from_f64_lit(v: f64) -> Fx {
        Fx(((v * 65536.0) as i64 as i32) & !1)
    }

    /// Parse a numeric literal: decimal (`3`, `3.5`, `.015`, `5.`), hex
    /// (`0xFDB9`), or binary (`0b101`). No sign — unary minus is an operator.
    /// Integer parts wrap into range like all other arithmetic (hex literals
    /// above 0x7FFF are the documented idiom for composing raw words).
    /// The fraction truncates and the raw LSB is cleared — 16.15 literals,
    /// exactly like PB (see module docs).
    pub fn parse_literal(s: &str) -> Option<Fx> {
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            if hex.is_empty() {
                return None;
            }
            let mut acc: u32 = 0;
            for c in hex.bytes() {
                acc = acc.wrapping_mul(16).wrapping_add((c as char).to_digit(16)?);
            }
            return Some(Fx::from_int(acc as i32));
        }
        if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
            if bin.is_empty() {
                return None;
            }
            let mut acc: u32 = 0;
            for c in bin.bytes() {
                acc = acc.wrapping_mul(2).wrapping_add((c as char).to_digit(2)?);
            }
            return Some(Fx::from_int(acc as i32));
        }

        let (int_str, frac_str) = match s.split_once('.') {
            Some((i, f)) => (i, f),
            None => (s, ""),
        };
        if int_str.is_empty() && frac_str.is_empty() {
            return None;
        }
        let mut int_acc: u32 = 0;
        for c in int_str.bytes() {
            if !c.is_ascii_digit() {
                return None;
            }
            int_acc = int_acc.wrapping_mul(10).wrapping_add((c - b'0') as u32);
        }
        // Fraction: numerator/10^n truncated to 1/65536. Nine digits is
        // already beyond representable precision.
        let mut num: u64 = 0;
        let mut den: u64 = 1;
        for c in frac_str.bytes() {
            if !c.is_ascii_digit() {
                return None;
            }
            if den < 1_000_000_000 {
                num = num * 10 + (c - b'0') as u64;
                den *= 10;
            }
        }
        let frac_raw = if den > 1 {
            ((num << FRAC_BITS) / den) as i32
        } else {
            0
        };
        // 16.15 literal: clear the LSB (no-op for integer literals)
        Some(Fx(
            Fx::from_int(int_acc as i32).0.wrapping_add(frac_raw) & !1
        ))
    }
}

// All arithmetic wraps — that IS the semantic, so the std operator traits
// implement the wrapping versions directly.

impl Add for Fx {
    type Output = Fx;
    #[inline]
    fn add(self, rhs: Fx) -> Fx {
        Fx(self.0.wrapping_add(rhs.0))
    }
}

impl Sub for Fx {
    type Output = Fx;
    #[inline]
    fn sub(self, rhs: Fx) -> Fx {
        Fx(self.0.wrapping_sub(rhs.0))
    }
}

impl Neg for Fx {
    type Output = Fx;
    #[inline]
    fn neg(self) -> Fx {
        Fx(self.0.wrapping_neg())
    }
}

impl Mul for Fx {
    type Output = Fx;
    /// Full 64-bit product, then take the middle 32 bits: truncation toward
    /// negative infinity on the discarded fraction bits, wrap on overflow.
    /// TODO(oracle): confirm PB truncates (vs rounds) sub-epsilon products.
    #[inline]
    fn mul(self, rhs: Fx) -> Fx {
        Fx(((self.0 as i64 * rhs.0 as i64) >> FRAC_BITS) as i32)
    }
}

impl Div for Fx {
    type Output = Fx;
    /// Truncated division; wraps on overflow (`MIN / -epsilon` etc.).
    /// Division by zero yields 0. TODO(oracle): verify PB's x/0 result.
    #[inline]
    fn div(self, rhs: Fx) -> Fx {
        if rhs.0 == 0 {
            return Fx::ZERO;
        }
        Fx((((self.0 as i64) << FRAC_BITS) / rhs.0 as i64) as i32)
    }
}

impl Rem for Fx {
    type Output = Fx;
    /// The `%` operator: truncated remainder, sign of the dividend.
    /// `x % 0` yields 0. TODO(oracle): verify PB's x % 0 result.
    #[inline]
    fn rem(self, rhs: Fx) -> Fx {
        if rhs.0 == 0 {
            return Fx::ZERO;
        }
        Fx(self.0.wrapping_rem(rhs.0))
    }
}

impl BitAnd for Fx {
    type Output = Fx;
    #[inline]
    fn bitand(self, rhs: Fx) -> Fx {
        Fx(self.0 & rhs.0)
    }
}

impl BitOr for Fx {
    type Output = Fx;
    #[inline]
    fn bitor(self, rhs: Fx) -> Fx {
        Fx(self.0 | rhs.0)
    }
}

impl BitXor for Fx {
    type Output = Fx;
    #[inline]
    fn bitxor(self, rhs: Fx) -> Fx {
        Fx(self.0 ^ rhs.0)
    }
}

impl Not for Fx {
    type Output = Fx;
    /// PB documents `~` as the one bitwise op that zeros the low 16 bits of
    /// its result.
    #[inline]
    fn not(self) -> Fx {
        Fx(!self.0 & !FRAC_MASK)
    }
}

impl Shl for Fx {
    type Output = Fx;
    /// Shifts the full 32-bit word (`x << 1 == x * 2`, fraction included).
    /// Count is the truncated integer part masked to 0..31.
    /// TODO(oracle): verify PB's handling of counts ≥ 32 and negative counts.
    #[inline]
    #[allow(clippy::suspicious_arithmetic_impl)] // the &31 mask is the semantic
    fn shl(self, rhs: Fx) -> Fx {
        Fx(self.0.wrapping_shl((rhs.to_int_trunc() & 31) as u32))
    }
}

impl Shr for Fx {
    type Output = Fx;
    /// Arithmetic (sign-preserving) shift of the full word.
    /// TODO(oracle): verify arithmetic-vs-logical on negative values.
    #[inline]
    #[allow(clippy::suspicious_arithmetic_impl)] // the &31 mask is the semantic
    fn shr(self, rhs: Fx) -> Fx {
        Fx(self.0 >> ((rhs.to_int_trunc() & 31) as u32))
    }
}

impl fmt::Debug for Fx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fx({})", self.to_f64())
    }
}

impl fmt::Display for Fx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(v: f64) -> Fx {
        Fx::from_f64(v)
    }

    #[test]
    fn range_and_wrap() {
        // from_int wraps at ±32768
        assert_eq!(Fx::from_int(32768), Fx::MIN);
        assert_eq!(Fx::from_int(32768).to_f64(), -32768.0);
        // 181² is the largest safe square; 182² wraps
        assert_eq!(Fx::from_int(181) * Fx::from_int(181), Fx::from_int(32761));
        assert_eq!(
            Fx::from_int(182) * Fx::from_int(182),
            Fx::from_int(33124) // wraps to 33124 - 65536 = -32412
        );
        assert_eq!((Fx::from_int(182) * Fx::from_int(182)).to_f64(), -32412.0);
        // additive wrap
        assert_eq!(Fx::MAX + Fx::EPSILON, Fx::MIN);
    }

    #[test]
    fn precision_quantization() {
        // 0.001 * 0.001 quantizes to zero (1e-6 < 1/65536)
        assert_eq!(fx(0.001) * fx(0.001), Fx::ZERO);
        assert_eq!(Fx::EPSILON.to_f64(), 1.0 / 65536.0);
    }

    #[test]
    fn rem_is_truncated_mod_is_floored() {
        // documented: -3.5 % 3 == -0.5 but mod(-3.5, 3) == 2.5
        assert_eq!(fx(-3.5) % fx(3.0), fx(-0.5));
        assert_eq!(fx(-3.5).mod_floor(fx(3.0)), fx(2.5));
        assert_eq!(fx(3.5) % fx(3.0), fx(0.5));
        assert_eq!(fx(3.5).mod_floor(fx(3.0)), fx(0.5));
        // sign of divisor for mod
        assert_eq!(fx(3.5).mod_floor(fx(-3.0)), fx(-2.5));
        // zero divisor defined as 0 (TODO(oracle))
        assert_eq!(fx(1.0) % Fx::ZERO, Fx::ZERO);
        assert_eq!(fx(1.0).mod_floor(Fx::ZERO), Fx::ZERO);
    }

    #[test]
    fn rounding_family() {
        assert_eq!(fx(-5.1).floor(), fx(-6.0)); // documented
        assert_eq!(fx(-5.9).ceil(), fx(-5.0)); // documented
        assert_eq!(fx(-5.5).frac(), fx(-0.5)); // documented
        assert_eq!(fx(-5.9).trunc(), fx(-5.0)); // documented
        assert_eq!(fx(5.1).floor(), fx(5.0));
        assert_eq!(fx(5.1).ceil(), fx(6.0));
        assert_eq!(fx(5.5).frac(), fx(0.5));
        // round is floor(x + 0.5) — oracle-confirmed
        assert_eq!(fx(2.5).round(), fx(3.0));
        assert_eq!(fx(-2.5).round(), fx(-2.0));
        assert_eq!(fx(0.5).round(), fx(1.0));
        assert_eq!(fx(-0.5).round(), fx(0.0));
    }

    #[test]
    fn bitwise_full_word() {
        // x | 0 keeps the fraction (unlike JS's truncating |0)
        assert_eq!(fx(1.5) | Fx::ZERO, fx(1.5));
        // x << 1 doubles, fraction included
        assert_eq!(fx(1.25) << Fx::ONE, fx(2.5));
        assert_eq!(fx(-1.25) << Fx::ONE, fx(-2.5));
        // >> halves (arithmetic)
        assert_eq!(fx(2.5) >> Fx::ONE, fx(1.25));
        assert_eq!(fx(-2.5) >> Fx::ONE, fx(-1.25));
        // ~ zeros the low 16 bits
        assert_eq!((!fx(1.5)).raw() & 0xFFFF, 0);
        assert_eq!(!Fx::ZERO, fx(-1.0).floor() * fx(1.0)); // !0 = 0xFFFF0000 = -65536 raw = -1.0
        assert_eq!((!Fx::ZERO).to_f64(), -1.0);
        // & aligns integer bits like JS for integers
        assert_eq!(Fx::from_int(5) & Fx::from_int(3), Fx::from_int(1));
    }

    #[test]
    fn division() {
        assert_eq!(Fx::from_int(1) / Fx::from_int(2), fx(0.5));
        assert_eq!(Fx::from_int(-1) / Fx::from_int(2), fx(-0.5));
        assert_eq!(Fx::from_int(1) / Fx::ZERO, Fx::ZERO); // TODO(oracle)
    }

    #[test]
    fn truthiness_and_logic_values() {
        assert!(!Fx::ZERO.is_truthy());
        assert!(fx(0.0001).is_truthy() || fx(0.0001) == Fx::ZERO); // sub-epsilon rounds
        assert!(Fx::EPSILON.is_truthy());
        assert!(fx(-1.0).is_truthy());
    }

    #[test]
    fn literals() {
        assert_eq!(Fx::parse_literal("1"), Some(Fx::ONE));
        assert_eq!(Fx::parse_literal("3.5"), Some(fx(3.5)));
        // 16.15 literal: trunc(0.015·65536)=983, LSB cleared → 982
        assert_eq!(Fx::parse_literal(".015"), Some(Fx::from_raw(982)));
        assert_eq!(Fx::parse_literal("0.7"), Some(Fx::from_raw(45874))); // oracle: multrunc
        assert_eq!(Fx::parse_literal("0.00005"), Some(Fx::from_raw(2))); // oracle: multrunc
        assert_eq!(Fx::parse_literal("5."), Some(Fx::from_int(5)));
        assert_eq!(Fx::parse_literal("0x10"), Some(Fx::from_int(16)));
        assert_eq!(Fx::parse_literal("0b101"), Some(Fx::from_int(5)));
        // hex above 0x7FFF wraps — documented idiom for composing raw words
        assert_eq!(
            Fx::parse_literal("0xFDB9"),
            Some(Fx::from_int(0xFDB9u32 as i32))
        );
        assert_eq!(
            Fx::parse_literal("0xFDB9").unwrap().to_f64(),
            64953.0 - 65536.0
        );
        // the epsilon literal's raw LSB is dropped — oracle: lit_epsilon
        assert_eq!(Fx::parse_literal("0.0000152587890625"), Some(Fx::ZERO));
        // constants quantize as literals — oracle: cPI is 205886, not 205887
        assert_eq!(Fx::from_f64_lit(core::f64::consts::PI).raw(), 205_886);
        assert_eq!(Fx::from_f64_lit(core::f64::consts::SQRT_2).raw(), 92_680);
        assert_eq!(Fx::parse_literal(""), None);
        assert_eq!(Fx::parse_literal("."), None);
        assert_eq!(Fx::parse_literal("1x"), None);
    }

    #[test]
    fn int_conversions() {
        assert_eq!(fx(-5.5).to_int_trunc(), -5);
        assert_eq!(fx(-5.5).to_int_floor(), -6);
        assert_eq!(fx(5.5).to_int_trunc(), 5);
        assert_eq!(fx(5.5).to_int_floor(), 5);
    }
}
