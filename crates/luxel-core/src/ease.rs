//! Easing kernels, shared by the VM's `ease*` builtins and the scene
//! compositor's keyframe tracks (`M` lines, docs/spec/scenes.md §1).
//!
//! One body per curve, so a pattern's `easeOutBounce(t)` and a layer that
//! eases `bounce` into a key cannot round differently. Same contract as
//! the builtins have always had: polynomial/analytic forms on `t`
//! (typically 0..1), no clamping except where the reference pins the
//! endpoints (the elastics). The three one-line cubics are `#[inline]`, so
//! `call_builtin` compiles them as it did when they were spelled inline
//! there; the four larger curves are left to the optimiser, as
//! `ease_out_bounce` always was, so the keyframe path does not carry a
//! second copy of each.

use crate::fixed::Fx;
use crate::fmath;

/// One half, the in-out curves' switch-over.
const HALF: Fx = Fx::from_raw(1 << 15);

/// `t³`.
#[inline]
pub fn in_cubic(t: Fx) -> Fx {
    t * t * t
}

/// `(t − 1)³ + 1`.
#[inline]
pub fn out_cubic(t: Fx) -> Fx {
    let u = t - Fx::ONE;
    u * u * u + Fx::ONE
}

/// `4t³` below one half, the mirrored out-curve above it.
#[inline]
pub fn in_out_cubic(t: Fx) -> Fx {
    if t < HALF {
        Fx::from_int(4) * t * t * t
    } else {
        let u = t + t - Fx::from_int(2);
        u * u * u / Fx::from_int(2) + Fx::ONE
    }
}

/// `(1 − cos(t·π)) / 2`, in turns.
pub fn in_out_sine(t: Fx) -> Fx {
    let c = fmath::cos_turns(t / Fx::from_int(2));
    (Fx::ONE - c) / Fx::from_int(2)
}

/// `1 + c3·(t−1)³ + c1·(t−1)²` with c1 = 1.70158 (the classic ~10 %
/// overshoot constant), c3 = c1 + 1.
pub fn out_back(t: Fx) -> Fx {
    let u = t - Fx::ONE;
    let c1 = Fx::from_f64(1.70158);
    let c3 = c1 + Fx::ONE;
    Fx::ONE + c3 * u * u * u + c1 * u * u
}

/// `2^(−10t)·sin((10t − 0.75)·2π/3) + 1`, endpoints pinned exactly.
pub fn out_elastic(t: Fx) -> Fx {
    if t <= Fx::ZERO {
        Fx::ZERO
    } else if t >= Fx::ONE {
        Fx::ONE
    } else {
        let ten_t = Fx::from_int(10) * t;
        let decay = fmath::pow(Fx::from_int(2), -ten_t);
        // sin's argument in turns: (10t - 0.75) / 3
        let s = fmath::sin_turns((ten_t - Fx::from_f64(0.75)) / Fx::from_int(3));
        decay * s + Fx::ONE
    }
}

/// Shared by the three bounce easings: piecewise parabolas, n1 = 7.5625,
/// d1 = 2.75 (the standard fit). "in" and "in-out" are reflections of it.
pub fn out_bounce(t: Fx) -> Fx {
    let n1 = Fx::from_f64(7.5625);
    let d1 = Fx::from_f64(2.75);
    if t < Fx::ONE / d1 {
        n1 * t * t
    } else if t < Fx::from_int(2) / d1 {
        let u = t - Fx::from_f64(1.5) / d1;
        n1 * u * u + Fx::from_f64(0.75)
    } else if t < Fx::from_f64(2.5) / d1 {
        let u = t - Fx::from_f64(2.25) / d1;
        n1 * u * u + Fx::from_f64(0.9375)
    } else {
        let u = t - Fx::from_f64(2.625) / d1;
        n1 * u * u + Fx::from_f64(0.984375)
    }
}
