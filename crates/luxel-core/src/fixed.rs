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
    /// `mod(x, 0)` is 0 (oracle-verified, fw 3.67).
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

        // Decimal, possibly with an exponent (`1e4`, `2.5e-3`). PB's compiler
        // is JavaScript, so literals go through correctly-rounded f64 parsing
        // and then 16.15 quantization — core's f64 FromStr matches that
        // exactly and deterministically.
        if s.is_empty() || s == "." || s.starts_with(['+', '-']) {
            return None;
        }
        match s.parse::<f64>() {
            Ok(v) if v.is_finite() => Some(Fx::from_f64_lit(v)),
            _ => None,
        }
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
    /// Oracle-verified: PB truncates sub-epsilon products (`multrunc` vector).
    #[inline]
    fn mul(self, rhs: Fx) -> Fx {
        Fx(((self.0 as i64 * rhs.0 as i64) >> FRAC_BITS) as i32)
    }
}

impl Div for Fx {
    type Output = Fx;
    /// Truncated division; wraps on overflow (`MIN / -epsilon` etc.).
    /// Division by zero yields 0 (oracle-verified, fw 3.67).
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
    /// `x % 0` yields 0 (oracle-verified, fw 3.67).
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
    /// Oracle-verified incl. counts ≥ 32, negative and fractional counts
    /// (`shl32/shl33/shlneg/shlfrac` vectors).
    #[inline]
    #[allow(clippy::suspicious_arithmetic_impl)] // the &31 mask is the semantic
    fn shl(self, rhs: Fx) -> Fx {
        Fx(self.0.wrapping_shl((rhs.to_int_trunc() & 31) as u32))
    }
}

impl Shr for Fx {
    type Output = Fx;
    /// Arithmetic (sign-preserving) shift of the full word.
    /// Oracle-verified: PB's `>>` is arithmetic on negatives (`shrneg` vectors).
    #[inline]
    #[allow(clippy::suspicious_arithmetic_impl)] // the &31 mask is the semantic
    fn shr(self, rhs: Fx) -> Fx {
        Fx(self.0 >> ((rhs.to_int_trunc() & 31) as u32))
    }
}

impl fmt::Debug for Fx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fx({})", self)
    }
}

impl Fx {
    /// Prints the exact decimal value into `buf`, without going through f64
    /// *or* `core::fmt`: routing this through `{}` on f64 pulled ~8 KB of
    /// flt2dec machinery into the firmware, and every `format!` call site
    /// drags in fmt's own (measured; these strings only feed diagnostics and
    /// JSON bodies). 16.16 is exactly representable in ≤ 16 fractional
    /// decimal digits; trailing zeros are trimmed, so integers print as "3",
    /// halves as "3.5". At most 23 bytes: sign + 5 integer digits + '.' + 16
    /// fractional digits.
    pub fn dec_str(self, buf: &mut [u8; 24]) -> &str {
        let raw = self.0;
        let mut n = 0;
        if raw < 0 {
            buf[0] = b'-';
            n = 1;
        }
        // unsigned magnitude avoids i32::MIN overflow
        let mag = (raw as i64).unsigned_abs();
        let mut int = mag >> 16;
        let mut rev = [0u8; 5];
        let mut d = 0;
        loop {
            rev[d] = b'0' + (int % 10) as u8;
            int /= 10;
            d += 1;
            if int == 0 {
                break;
            }
        }
        while d > 0 {
            d -= 1;
            buf[n] = rev[d];
            n += 1;
        }
        let mut frac = mag & 0xFFFF;
        if frac != 0 {
            buf[n] = b'.';
            n += 1;
            while frac != 0 {
                frac *= 10;
                buf[n] = b'0' + (frac >> 16) as u8;
                frac &= 0xFFFF;
                n += 1;
            }
        }
        core::str::from_utf8(&buf[..n]).unwrap()
    }
}

impl fmt::Display for Fx {
    /// Exact decimal, shared with [`Fx::dec_str`] so the two can't diverge.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut b = [0u8; 24];
        f.write_str(self.dec_str(&mut b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(v: f64) -> Fx {
        Fx::from_f64(v)
    }

    #[test]
    fn display_exact_decimal() {
        assert_eq!(alloc::format!("{}", Fx::from_int(3)), "3");
        assert_eq!(alloc::format!("{}", fx(3.5)), "3.5");
        assert_eq!(alloc::format!("{}", fx(-0.25)), "-0.25");
        assert_eq!(alloc::format!("{}", Fx::EPSILON), "0.0000152587890625");
        assert_eq!(alloc::format!("{}", Fx::MIN), "-32768");
        assert_eq!(alloc::format!("{:?}", fx(1.5)), "Fx(1.5)");
        // exact digits round-trip through f64 parsing for any raw value
        for raw in [1, -1, 12345, -98765, 0x7fff_ffff, i32::MIN + 1] {
            let v = Fx::from_raw(raw);
            let s = alloc::format!("{}", v);
            assert_eq!(Fx::from_f64(s.parse::<f64>().unwrap()), v, "{s}");
        }
    }

    #[test]
    fn dec_str_matches_display() {
        let mut b = [0u8; 24];
        assert_eq!(Fx::ZERO.dec_str(&mut b), "0");
        assert_eq!(Fx::from_int(3).dec_str(&mut b), "3");
        assert_eq!(fx(3.5).dec_str(&mut b), "3.5");
        assert_eq!(fx(-3.5).dec_str(&mut b), "-3.5");
        assert_eq!(Fx::from_raw(i32::MIN).dec_str(&mut b), "-32768");
        assert_eq!(Fx::from_raw(1).dec_str(&mut b), "0.0000152587890625");
        for raw in [
            0,
            1,
            -1,
            i32::MIN,
            i32::MAX,
            12345,
            -98765,
            0x1_0000,
            -0x1_0000,
            0x7fff_0000u32 as i32,
        ] {
            let v = Fx::from_raw(raw);
            assert_eq!(v.dec_str(&mut b), alloc::format!("{}", v), "raw {raw}");
        }
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
        // zero divisor defined as 0 (oracle-verified)
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
        assert_eq!(Fx::from_int(1) / Fx::ZERO, Fx::ZERO); // oracle-verified
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
        // scientific notation (JS-style literals)
        assert_eq!(Fx::parse_literal("1e2"), Some(Fx::from_int(100)));
        assert_eq!(Fx::parse_literal("2.5e-3"), Some(Fx::from_raw(162))); // trunc(163.84)&!1
        assert_eq!(Fx::parse_literal("1E+1"), Some(Fx::from_int(10)));
        assert_eq!(Fx::parse_literal("1e-9"), Some(Fx::ZERO));
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
