//! Color-space conversions beyond Pixel Blaze's HSV/RGB. OKLab/OKLCH gives
//! perceptually-uniform lightness and hue, so fades and gradients look far
//! smoother than HSV (no dark bands through blue, even brightness across
//! hues). All fixed-point; output is gamma-sRGB 0..1 per channel, matching
//! what `rgb()` expects.

use crate::fixed::Fx;
use crate::fmath;

fn f(v: f64) -> Fx {
    Fx::from_f64(v)
}

/// linear-light channel → gamma sRGB (the standard piecewise transfer).
fn linear_to_srgb(c: Fx) -> Fx {
    let c = c.clamp(Fx::ZERO, Fx::ONE);
    if c <= f(0.0031308) {
        c * f(12.92)
    } else {
        // 1.055·c^(1/2.4) − 0.055
        f(1.055) * fmath::pow(c, f(1.0 / 2.4)) - f(0.055)
    }
    .clamp(Fx::ZERO, Fx::ONE)
}

/// OKLab (L, a, b) → gamma-sRGB [r, g, b], each 0..1.
pub fn oklab_to_rgb(l: Fx, a: Fx, b: Fx) -> [Fx; 3] {
    // OKLab → LMS' (Björn Ottosson's matrices)
    let l_ = l + f(0.396_337_777_4) * a + f(0.215_803_757_3) * b;
    let m_ = l - f(0.105_561_345_8) * a - f(0.063_854_172_8) * b;
    let s_ = l - f(0.089_484_177_5) * a - f(1.291_485_548_0) * b;
    // cube (LMS' → LMS)
    let lc = l_ * l_ * l_;
    let mc = m_ * m_ * m_;
    let sc = s_ * s_ * s_;
    // LMS → linear sRGB
    let r = f(4.076_741_662_1) * lc - f(3.307_711_591_3) * mc + f(0.230_969_929_2) * sc;
    let g = -f(1.268_438_004_6) * lc + f(2.609_757_401_1) * mc - f(0.341_319_396_5) * sc;
    let bl = -f(0.004_196_086_3) * lc - f(0.703_418_614_7) * mc + f(1.707_614_701_0) * sc;
    [linear_to_srgb(r), linear_to_srgb(g), linear_to_srgb(bl)]
}

/// OKLCH → gamma-sRGB. `l` 0..1 lightness, `c` chroma (~0..0.4 in gamut),
/// `h` hue in **turns** (0..1), matching hsv()'s hue convention.
pub fn oklch_to_rgb(l: Fx, c: Fx, h: Fx) -> [Fx; 3] {
    let (sin, cos) = (fmath::sin_turns(h), fmath::cos_turns(h));
    oklab_to_rgb(l, c * cos, c * sin)
}

/// gamma sRGB channel → linear light (inverse of [linear_to_srgb]).
fn srgb_to_linear(c: Fx) -> Fx {
    let c = c.clamp(Fx::ZERO, Fx::ONE);
    if c <= f(0.04045) {
        c / f(12.92)
    } else {
        fmath::pow((c + f(0.055)) / f(1.055), f(2.4))
    }
}

/// Cube root for the (non-negative) LMS values on the OKLab forward path.
fn cbrt(x: Fx) -> Fx {
    if x.raw() <= 0 {
        Fx::ZERO
    } else {
        fmath::pow(x, f(1.0 / 3.0))
    }
}

/// gamma-sRGB [r, g, b] (0..1) → OKLab (L, a, b).
pub fn rgb_to_oklab(rgb: [Fx; 3]) -> [Fx; 3] {
    let r = srgb_to_linear(rgb[0]);
    let g = srgb_to_linear(rgb[1]);
    let b = srgb_to_linear(rgb[2]);
    // linear sRGB → LMS (Björn Ottosson's matrices)
    let l = f(0.412_221_470_8) * r + f(0.536_332_536_3) * g + f(0.051_445_992_9) * b;
    let m = f(0.211_903_498_2) * r + f(0.680_699_545_1) * g + f(0.107_396_956_6) * b;
    let s = f(0.088_302_461_9) * r + f(0.281_718_837_6) * g + f(0.629_978_700_5) * b;
    let l_ = cbrt(l);
    let m_ = cbrt(m);
    let s_ = cbrt(s);
    [
        f(0.210_454_255_3) * l_ + f(0.793_617_785_0) * m_ - f(0.004_072_046_8) * s_,
        f(1.977_998_495_1) * l_ - f(2.428_592_205_0) * m_ + f(0.450_593_709_9) * s_,
        f(0.025_904_037_1) * l_ + f(0.782_771_766_2) * m_ - f(0.808_675_766_0) * s_,
    ]
}

/// Mix two gamma-sRGB colors in OKLab — perceptually even blends with no
/// muddy midpoints (what `mixColors` exposes to patterns). `t` 0..1.
pub fn mix_oklab(c1: [Fx; 3], c2: [Fx; 3], t: Fx) -> [Fx; 3] {
    let a = rgb_to_oklab(c1);
    let b = rgb_to_oklab(c2);
    oklab_to_rgb(
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    )
}
