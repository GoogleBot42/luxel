//! Perlin-style gradient noise in fixed point — deterministic, integer-only,
//! hash-based (no permutation tables to seed).
//!
//! TODO(oracle): PB's exact noise algorithm, output range, and fbm/ridge/
//! turbulence normalization are unknown — these are visually-plausible
//! stand-ins to be tuned via sweep probes against real hardware.

use crate::fixed::Fx;

const ONE: i64 = 1 << 16;

#[inline]
fn fmul(a: i64, b: i64) -> i64 {
    (a * b) >> 16
}

/// Quintic fade 6t⁵−15t⁴+10t³ on a 16-frac t ∈ [0,1].
#[inline]
fn fade(t: i64) -> i64 {
    let inner = fmul(t, 6 * t - 15 * ONE) + 10 * ONE;
    fmul(fmul(fmul(t, t), t), inner)
}

#[inline]
fn lerp(a: i64, b: i64, t: i64) -> i64 {
    a + fmul(b - a, t)
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

#[inline]
fn wrap(i: i32, w: i32) -> i32 {
    i.rem_euclid(w)
}

/// 3D gradient noise, roughly in [-1, 1]; lattice wraps at `wraps` (per axis).
pub fn perlin(x: Fx, y: Fx, z: Fx, seed: Fx, wraps: [i32; 3]) -> Fx {
    let (xi, yi, zi) = (x.to_int_floor(), y.to_int_floor(), z.to_int_floor());
    let fx = (x.raw() & 0xFFFF) as i64;
    let fy = (y.raw() & 0xFFFF) as i64;
    let fz = (z.raw() & 0xFFFF) as i64;
    let s = seed.raw();
    let (x0, x1) = (wrap(xi, wraps[0]), wrap(xi + 1, wraps[0]));
    let (y0, y1) = (wrap(yi, wraps[1]), wrap(yi + 1, wraps[1]));
    let (z0, z1) = (wrap(zi, wraps[2]), wrap(zi + 1, wraps[2]));

    let g = |xc: i32, yc: i32, zc: i32, dx: i64, dy: i64, dz: i64| {
        grad(hash(xc, yc, zc, s), dx, dy, dz)
    };
    let n000 = g(x0, y0, z0, fx, fy, fz);
    let n100 = g(x1, y0, z0, fx - ONE, fy, fz);
    let n010 = g(x0, y1, z0, fx, fy - ONE, fz);
    let n110 = g(x1, y1, z0, fx - ONE, fy - ONE, fz);
    let n001 = g(x0, y0, z1, fx, fy, fz - ONE);
    let n101 = g(x1, y0, z1, fx - ONE, fy, fz - ONE);
    let n011 = g(x0, y1, z1, fx, fy - ONE, fz - ONE);
    let n111 = g(x1, y1, z1, fx - ONE, fy - ONE, fz - ONE);

    let (u, v, w) = (fade(fx), fade(fy), fade(fz));
    let nx00 = lerp(n000, n100, u);
    let nx10 = lerp(n010, n110, u);
    let nx01 = lerp(n001, n101, u);
    let nx11 = lerp(n011, n111, u);
    let nxy0 = lerp(nx00, nx10, v);
    let nxy1 = lerp(nx01, nx11, v);
    Fx::from_raw(lerp(nxy0, nxy1, w) as i32)
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

/// One simplex corner's contribution: (t² · t²) · grad, t = cap − d².
#[inline]
fn corner(t: i64, g: i64) -> i64 {
    if t <= 0 {
        return 0;
    }
    let t2 = fmul(t, t);
    fmul(fmul(t2, t2), g)
}

/// 2D simplex noise, roughly in [-1, 1]. Deterministic (seed included in
/// the corner hash, like perlin's).
pub fn simplex2(x: Fx, y: Fx, seed: Fx) -> Fx {
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
    let n0 = corner(half - fmul(x0, x0) - fmul(y0, y0), grad2(hash(i, j, 0, sd), x0, y0));
    let n1 = corner(
        half - fmul(x1, x1) - fmul(y1, y1),
        grad2(hash(i + i1, j + j1, 0, sd), x1, y1),
    );
    let n2 = corner(
        half - fmul(x2, x2) - fmul(y2, y2),
        grad2(hash(i + 1, j + 1, 0, sd), x2, y2),
    );
    // 70× puts the sum in ~[-1, 1] (Gustavson's constant)
    Fx::from_raw((70 * (n0 + n1 + n2)) as i32)
}

/// 3D simplex noise, roughly in [-1, 1].
pub fn simplex3(x: Fx, y: Fx, z: Fx, seed: Fx) -> Fx {
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
    let n0 = corner(d(x0, y0, z0), grad(hash(i, j, k, sd), x0, y0, z0));
    let n1 = corner(
        d(x1, y1, z1),
        grad(hash(i + i1, j + j1, k + k1, sd), x1, y1, z1),
    );
    let n2 = corner(
        d(x2, y2, z2),
        grad(hash(i + i2, j + j2, k + k2, sd), x2, y2, z2),
    );
    let n3 = corner(
        d(x3, y3, z3),
        grad(hash(i + 1, j + 1, k + 1, sd), x3, y3, z3),
    );
    Fx::from_raw((32 * (n0 + n1 + n2 + n3)) as i32)
}

fn octaves_of(n: Fx) -> i32 {
    n.to_int_trunc().clamp(1, 8)
}

/// Fractal Brownian motion: octaves of perlin summed with gain falloff.
pub fn fbm(x: Fx, y: Fx, z: Fx, lacunarity: Fx, gain: Fx, octaves: Fx, wraps: [i32; 3]) -> Fx {
    let mut sum: i64 = 0;
    let mut freq = Fx::ONE;
    let mut amp = Fx::ONE;
    for _ in 0..octaves_of(octaves) {
        let n = perlin(x * freq, y * freq, z * freq, Fx::ZERO, wraps);
        sum += fmul(n.raw() as i64, amp.raw() as i64);
        freq = freq * lacunarity;
        amp = amp * gain;
    }
    Fx::from_raw(sum as i32)
}

/// Ridged multifractal: octaves of (offset − |noise|)².
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
    let mut sum: i64 = 0;
    let mut freq = Fx::ONE;
    let mut amp = Fx::ONE;
    for _ in 0..octaves_of(octaves) {
        let n = perlin(x * freq, y * freq, z * freq, Fx::ZERO, wraps);
        let r = (offset - n.abs()).raw() as i64;
        sum += fmul(fmul(r, r), amp.raw() as i64);
        freq = freq * lacunarity;
        amp = amp * gain;
    }
    Fx::from_raw(sum as i32)
}

/// Turbulence: octaves of |noise|.
pub fn turbulence(
    x: Fx,
    y: Fx,
    z: Fx,
    lacunarity: Fx,
    gain: Fx,
    octaves: Fx,
    wraps: [i32; 3],
) -> Fx {
    let mut sum: i64 = 0;
    let mut freq = Fx::ONE;
    let mut amp = Fx::ONE;
    for _ in 0..octaves_of(octaves) {
        let n = perlin(x * freq, y * freq, z * freq, Fx::ZERO, wraps);
        sum += fmul(n.abs().raw() as i64, amp.raw() as i64);
        freq = freq * lacunarity;
        amp = amp * gain;
    }
    Fx::from_raw(sum as i32)
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
}
