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
