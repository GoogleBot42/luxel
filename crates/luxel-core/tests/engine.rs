//! Golden frame-pipeline tests: exact pixel bytes for known patterns.
//! These lock cross-host determinism — the same bytes must come out of the
//! native, wasm32, and ESP32 builds.

use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::vm::Value;

const RAINBOW: &str =
    "export function render(index) {\n  hsv(time(.1) + index / pixelCount, 1, 1)\n}";

#[test]
fn rainbow_golden_frame() {
    let mut e = Engine::new(RAINBOW, 4, 1).unwrap();
    // delta 0 → time() is 0 → hue is exactly index/4
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [255, 0, 0]); // h=0    red
    assert_eq!(px[1], [128, 255, 0]); // h=0.25 chartreuse
    assert_eq!(px[2], [0, 255, 255]); // h=0.5  cyan
    assert_eq!(px[3], [128, 0, 255]); // h=0.75 violet
    assert!(e.last_error.is_none());
}

#[test]
fn render_receives_normalized_x() {
    let mut e = Engine::new(
        "export var got\nexport function render(index, x) { got = x\n rgb(x, 0, 0) }",
        4,
        1,
    )
    .unwrap();
    let px = e.frame(Fx::ZERO);
    // last pixel: x = 3/4 → r = round(0.75·255) = 191
    assert_eq!(px[3], [191, 0, 0]);
    assert_eq!(e.var("got"), Some(Value::Num(Fx::from_f64(0.75))));
}

#[test]
fn time_advances_with_delta() {
    let src = "export var out\n\
               export function beforeRender(delta) { out = time(.1) }\n\
               export function render(i) { rgb(0, 0, 0) }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    // interval .1 → 16.15 literal raw 6552 → period 6552 ms
    e.frame(Fx::from_int(3276)); // half the period
    let Some(Value::Num(t)) = e.var("out") else {
        panic!()
    };
    assert_eq!(t.raw(), ((3276u64 << 16) / 6552) as i32); // exact, deterministic
    assert!((t.to_f64() - 0.5).abs() < 0.001);
    // wraps after a full period
    e.frame(Fx::from_int(3276));
    let Some(Value::Num(t)) = e.var("out") else {
        panic!()
    };
    assert_eq!(t, Fx::ZERO);
}

#[test]
fn delta_is_passed_in_ms() {
    let src = "export var d\n\
               export function beforeRender(delta) { d = delta }\n\
               export function render(i) { }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    e.frame(Fx::from_f64(16.5));
    assert_eq!(e.var("d"), Some(Value::Num(Fx::from_f64(16.5))));
}

#[test]
fn render3d_fallback_gets_midspace() {
    let src = "export var ys\nexport var zs\n\
               export function render3D(index, x, y, z) { ys = y\n zs = z\n rgb(x, y, z) }";
    let mut e = Engine::new(src, 2, 1).unwrap();
    e.frame(Fx::ZERO);
    let half = Fx::from_raw(1 << 15);
    assert_eq!(e.var("ys"), Some(Value::Num(half)));
    assert_eq!(e.var("zs"), Some(Value::Num(half)));
}

#[test]
fn render_priority_prefers_1d() {
    let src = "export var which\n\
               export function render(i) { which = 1\n rgb(0,0,0) }\n\
               export function render2D(i, x, y) { which = 2\n rgb(0,0,0) }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    e.frame(Fx::ZERO);
    assert_eq!(e.var("which"), Some(Value::Num(Fx::ONE)));
}

#[test]
fn blinkfade_soaks_clean() {
    let src = include_str!("../../../examples/blinkfade.js");
    let mut e = Engine::new(src, 60, 7).unwrap();
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
    let delta = Fx::from_f64(1000.0 / 60.0);
    let mut lit = false;
    for _ in 0..120 {
        let px = e.frame(delta);
        lit |= px.iter().any(|p| p.iter().any(|&c| c > 0));
        assert!(e.last_error.is_none(), "{:?}", e.last_error);
    }
    assert!(lit, "blinkfade never lit a pixel in 120 frames");
}

#[test]
fn determinism_same_seed_same_bytes() {
    let src = include_str!("../../../examples/blinkfade.js");
    let delta = Fx::from_f64(16.0);
    let run = |seed| {
        let mut e = Engine::new(src, 30, seed).unwrap();
        let mut all = Vec::new();
        for _ in 0..50 {
            all.extend_from_slice(e.frame(delta));
        }
        all
    };
    assert_eq!(run(99), run(99));
    assert_ne!(run(99), run(100)); // and the seed actually matters
}

#[test]
fn no_render_function_is_dark_not_fatal() {
    let mut e = Engine::new("x = 1", 3, 1).unwrap();
    let px = e.frame(Fx::ZERO);
    assert_eq!(px, &[[0, 0, 0]; 3]);
}

#[test]
fn render_error_blanks_frame_and_records() {
    let src = "export function render(i) { if (i == 1) badCall()\n hsv(0, 0, 1) }";
    // badCall is unknown → compile error; use a runtime one instead
    assert!(Engine::new(src, 2, 1).is_err());
    let src = "f = 0\nexport function render(i) { if (i == 1) f()\n hsv(0, 0, 1) }";
    let mut e = Engine::new(src, 3, 1).unwrap();
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [255, 255, 255]); // rendered before the error
    assert_eq!(px[1], [0, 0, 0]); // error pixel blanked
    assert_eq!(px[2], [0, 0, 0]); // rest of frame blanked
    assert!(e.take_error().is_some());
    assert!(e.last_error.is_none()); // taken
}
