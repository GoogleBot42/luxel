//! The anti-aliased, blended index-space bulk ops: `splatIndex`,
//! `fillRangeAA`, `gradientRange` (docs/lang.md "Index space").
//!
//! Each op is checked byte for byte against a reference loop written here
//! from the documented formula — over EVERY pixel of the strip, so the ops'
//! own run bounds are checked too — on 60, 300 and 512 px strips, across
//! fractional, negative, past-the-end and swapped positions and all three
//! blend modes, on a non-black background so the modes are visible.

use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::vm::hsv_to_rgb;

const HALF: Fx = Fx::from_raw(1 << 15);

/// The engine's channel quantizer: clamp 0..1, × 255, floor.
fn q(v: Fx) -> u8 {
    ((v.clamp(Fx::ZERO, Fx::ONE).raw() * 255) >> 16) as u8
}

fn put(dst: &mut [u8; 3], src: [u8; 3], mode: i32) {
    for c in 0..3 {
        dst[c] = match mode {
            1 => dst[c].saturating_add(src[c]),
            2 => dst[c].max(src[c]),
            _ => src[c],
        };
    }
}

/// Background, then the brush, then one op call whose arguments are the
/// exported vars `a0..a8` (set raw by the test, so no decimal parsing).
fn engine(call: &str, n: u32) -> Engine {
    let src = format!(
        "export var a0 = 0\nexport var a1 = 0\nexport var a2 = 0\nexport var a3 = 0\n\
         export var a4 = 0\nexport var a5 = 0\nexport var a6 = 0\nexport var a7 = 0\n\
         export var a8 = 0\n\
         export function renderBulk() {{ rgb(0.3, 0.1, 0.6)\n fill()\n rgb(0.875, 0.5, 0.25)\n {call} }}"
    );
    Engine::new(&src, n, 1).unwrap()
}

fn render(e: &mut Engine, args: &[Fx]) -> Vec<[u8; 3]> {
    for (k, v) in args.iter().enumerate() {
        assert!(e.set_var(&format!("a{k}"), *v));
    }
    let out = e.frame(Fx::from_int(16)).to_vec();
    assert!(e.take_error().is_none());
    out
}

/// The background exactly as the engine paints it (the compiler's own
/// reading of 0.3/0.1/0.6), from the same pattern with no op.
fn background(n: u32) -> Vec<[u8; 3]> {
    render(&mut engine("", n), &[])
}

/// Binary fractions, so the brush has no decimal-parsing ulp to disagree on.
fn brush() -> [Fx; 3] {
    [Fx::from_f64(0.875), Fx::from_f64(0.5), Fx::from_f64(0.25)]
}

fn scaled(c: [Fx; 3], k: Fx) -> [u8; 3] {
    [q(c[0] * k), q(c[1] * k), q(c[2] * k)]
}

fn coverage(i: Fx, p0: Fx, p1: Fx) -> Fx {
    ((i + Fx::ONE).min(p1) - i.max(p0)).clamp(Fx::ZERO, Fx::ONE)
}

/// Positions worth trying on an `n`-pixel strip: integers, fractions,
/// both ends, off both ends.
fn positions(n: u32) -> Vec<Fx> {
    let n = n as f64;
    [-7.25, -0.5, 0.0, 0.3, 1.0, 2.5, 3.75, n / 3.0, n / 2.0 + 0.125, n - 1.0, n - 0.4, n, n + 2.6]
        .iter()
        .map(|&v| Fx::from_f64(v))
        .collect()
}

const STRIPS: [u32; 3] = [60, 300, 512];

#[test]
fn splat_index_matches_the_reference() {
    for n in STRIPS {
        let mut e = engine("splatIndex(a0, a1, a2)", n);
        for pos in positions(n) {
            for r in [0.1, 0.5, 1.0, 2.75, 9.0] {
                for mode in 0..3 {
                    let r = Fx::from_f64(r);
                    let got = render(&mut e, &[pos, r, Fx::from_int(mode)]);
                    let mut want = background(n);
                    let inv = Fx::ONE / r.max(HALF);
                    for (i, px) in want.iter_mut().enumerate() {
                        let w = Fx::ONE - (Fx::from_int(i as i32) - pos).abs() * inv;
                        if w > Fx::ZERO {
                            put(px, scaled(brush(), w), mode);
                        }
                    }
                    assert_eq!(got, want, "{n} px, pos {pos:?}, r {r:?}, mode {mode}");
                }
            }
        }
    }
}

#[test]
fn fill_range_aa_matches_the_reference() {
    for n in STRIPS {
        let mut e = engine("fillRangeAA(a0, a1, a2)", n);
        let ps = positions(n);
        for &a in &ps {
            for &b in &ps {
                for mode in 0..3 {
                    let got = render(&mut e, &[a, b, Fx::from_int(mode)]);
                    let (p0, p1) = if b < a { (b, a) } else { (a, b) };
                    let mut want = background(n);
                    for (i, px) in want.iter_mut().enumerate() {
                        let k = coverage(Fx::from_int(i as i32), p0, p1);
                        if k > Fx::ZERO {
                            put(px, scaled(brush(), k), mode);
                        }
                    }
                    assert_eq!(got, want, "{n} px, [{a:?}, {b:?}), mode {mode}");
                }
            }
        }
    }
}

#[test]
fn integer_ends_are_fill_range() {
    for n in STRIPS {
        let mut aa = engine("fillRangeAA(a0, a1)", n);
        let mut plain = engine("fillRange(a0, a1)", n);
        for a in [-3, 0, 1, 7, n as i32 / 2, n as i32 - 1, n as i32 + 4] {
            for b in [-1, 0, 2, 9, n as i32 / 2 + 3, n as i32, n as i32 + 9] {
                let args = [Fx::from_int(a), Fx::from_int(b)];
                let want = if b >= a {
                    render(&mut plain, &args)
                } else {
                    // fillRange paints nothing inverted; fillRangeAA swaps
                    render(&mut plain, &[args[1], args[0]])
                };
                assert_eq!(render(&mut aa, &args), want, "{n} px, [{a}, {b})");
            }
        }
    }
}

#[test]
fn gradient_range_matches_the_reference() {
    let ca = [Fx::from_f64(0.95), Fx::from_f64(1.0), Fx::from_f64(0.8)];
    let cb = [Fx::from_f64(1.4), Fx::from_f64(0.6), Fx::from_f64(0.3)];
    for n in STRIPS {
        let mut e = engine("gradientRange(a0, a1, a2, a3, a4, a5, a6, a7, a8)", n);
        let ps = positions(n);
        for &a in &ps {
            for &b in &ps {
                for mode in 0..3 {
                    let args = [a, b, ca[0], ca[1], ca[2], cb[0], cb[1], cb[2], Fx::from_int(mode)];
                    let got = render(&mut e, &args);
                    let (p0, p1, x, y) = if b < a { (b, a, cb, ca) } else { (a, b, ca, cb) };
                    let inv = Fx::ONE / (p1 - p0);
                    let mut want = background(n);
                    for (i, px) in want.iter_mut().enumerate() {
                        let fi = Fx::from_int(i as i32);
                        let k = coverage(fi, p0, p1);
                        if k > Fx::ZERO {
                            let t = ((fi + HALF - p0) * inv).clamp(Fx::ZERO, Fx::ONE);
                            let rgb = hsv_to_rgb(
                                x[0] + (y[0] - x[0]) * t,
                                x[1] + (y[1] - x[1]) * t,
                                (x[2] + (y[2] - x[2]) * t) * k,
                            );
                            put(px, [q(rgb[0]), q(rgb[1]), q(rgb[2])], mode);
                        }
                    }
                    assert_eq!(got, want, "{n} px, [{a:?}, {b:?}), mode {mode}");
                }
            }
        }
    }
}

#[test]
fn naming_them_installs_no_grid_on_a_mapless_strip() {
    for call in ["splatIndex(3.5, 2, 1)", "fillRangeAA(1.5, 9.25, 2)", "gradientRange(0, 60, 0, 1, 1, 1, 1, 1)"] {
        let src = format!("export function renderBulk() {{ clear()\n rgb(1, 1, 1)\n {call} }}");
        let mut e = Engine::new(&src, 60, 1).unwrap();
        e.frame(Fx::from_int(16));
        assert!(e.installed_map().is_none(), "{call} conjured a map");
        let g = e.effective_geometry();
        assert_eq!((g.w, g.h, g.pattern_dims), (0, 0, 0), "{call}");
        assert!(e.pixels().iter().any(|p| *p != [0, 0, 0]), "{call} painted");
    }
}

#[test]
fn a_comet_in_one_line() {
    // the docs example: a soft head at a fractional position, additively
    let src = "export var t = 0\nexport function beforeRender(d) { t += d / 1000 }\n\
        export function renderBulk() { fade(0.85)\n hsv(0.6, 0.7, 1)\n \
        splatIndex(triangle(t * 0.2) * (pixelCount - 1), 2.5, 1) }";
    let mut e = Engine::new(src, 300, 1).unwrap();
    for _ in 0..40 {
        e.frame(Fx::from_int(16));
    }
    assert!(e.take_error().is_none());
    let lit = e.pixels().iter().filter(|p| p[2] > 0).count();
    assert!(lit > 5 && lit < 300, "{lit}");
}

/// Pinned bytes; tools/wasm-smoke.mjs asserts the same 36 through the FFI.
const SMOKE_SRC: &str = "export function renderBulk() { clear()\n \
    gradientRange(0.5, 7.75, 0, 1, 1, 0.5, 1, 0.5)\n rgb(1, 0.5, 0)\n \
    splatIndex(8.25, 2.5, 1)\n rgb(0, 0, 1)\n fillRangeAA(9.5, 12, 2) }";

#[test]
fn smoke_golden() {
    let mut e = Engine::new(SMOKE_SRC, 12, 1).unwrap();
    let got: Vec<u8> = e.frame(Fx::from_int(16)).iter().flatten().copied().collect();
    assert_eq!(
        got,
        [
            127, 0, 0, 237, 98, 0, 219, 181, 0, 153, 202, 0, 63, 184, 0, 0, 167, 11, //
            25, 161, 72, 127, 161, 88, 229, 114, 0, 178, 89, 127, 76, 38, 255, 0, 0, 255,
        ]
    );
}
