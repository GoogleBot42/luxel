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
    assert_eq!(px[1], [127, 255, 0]); // h=0.25 chartreuse (floor-quantized, PB-exact)
    assert_eq!(px[2], [0, 255, 255]); // h=0.5  cyan
    assert_eq!(px[3], [127, 0, 255]); // h=0.75 violet (floor-quantized, PB-exact)
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

const COUNTER: &str = "export var frames, d, t\n\
                       export function beforeRender(delta) { frames++\n d = delta\n t = time(.1) }\n\
                       export function render(i) { rgb(1, 1, 1) }";

fn num(e: &Engine, name: &str) -> f64 {
    match e.var(name) {
        Some(Value::Num(v)) => v.to_f64(),
        other => panic!("{name} = {other:?}"),
    }
}

#[test]
fn time_scale_scales_the_pattern_clock() {
    // timeScale(s) runs the pattern-visible clock at s × real time: the
    // engine clock, time()/beat(), and beforeRender's delta all follow.
    let mut e = Engine::new(&format!("timeScale(.25)\n{COUNTER}"), 1, 1).unwrap();
    for _ in 0..4 {
        e.frame(Fx::from_int(40));
    }
    assert_eq!(e.time_ms(), 40); // 160 real ms at quarter speed
    assert_eq!(num(&e, "d"), 10.0); // delta is scaled too
    assert_eq!(num(&e, "frames"), 4.0); // …but every frame still renders

    // 0 freezes the clock without stopping rendering
    let mut z = Engine::new(&format!("timeScale(0)\n{COUNTER}"), 1, 1).unwrap();
    for _ in 0..3 {
        z.frame(Fx::from_int(40));
    }
    assert_eq!(z.time_ms(), 0);
    assert_eq!(num(&z, "d"), 0.0);
    assert_eq!(num(&z, "frames"), 3.0);

    // negative clamps to frozen rather than running the clock backwards
    let mut n = Engine::new(&format!("timeScale(-2)\n{COUNTER}"), 1, 1).unwrap();
    n.frame(Fx::from_int(40));
    assert_eq!(n.time_ms(), 0);

    // >1 speeds up, and the call returns the previous scale
    let mut f = Engine::new(&format!("export var prev\n{COUNTER}\nprev = timeScale(3)"), 1, 1)
        .unwrap();
    f.frame(Fx::from_int(10));
    assert_eq!(f.time_ms(), 30);
    assert_eq!(num(&f, "prev"), 1.0);

    // untouched patterns are bit-identical to before the control existed
    let mut plain = Engine::new(COUNTER, 1, 1).unwrap();
    plain.frame(Fx::from_f64(16.5));
    assert_eq!(plain.time_ms(), 16);
    assert_eq!(num(&plain, "d"), 16.5);
}

#[test]
fn set_frame_rate_caps_pattern_evaluation() {
    // setFrameRate(fps) holds the last frame until 1000/fps real ms have
    // passed. The pattern runs less often; the clock does not slow down.
    let mut e = Engine::new(&format!("setFrameRate(10)\n{COUNTER}"), 1, 1).unwrap();
    e.frame(Fx::from_int(10)); // first frame always renders
    assert_eq!(num(&e, "frames"), 1.0);
    for _ in 0..9 {
        e.frame(Fx::from_int(10)); // 90 more ms: still under the 100 ms cap
    }
    assert_eq!(num(&e, "frames"), 1.0, "capped frames must not run");
    assert_eq!(e.time_ms(), 100, "the clock keeps running while capped");
    e.frame(Fx::from_int(10));
    assert_eq!(num(&e, "frames"), 2.0);
    // the frame that does run sees the WHOLE elapsed interval as its delta
    assert_eq!(num(&e, "d"), 100.0);
    // …so time-based animation lands in the same place as uncapped
    assert!((num(&e, "t") - 110.0 / 6552.0).abs() < 1e-3);

    // held frames return the previous pixels, not black
    let mut p = Engine::new(
        "setFrameRate(10)\nexport function render(i) { rgb(1, 1, 1) }",
        2,
        1,
    )
    .unwrap();
    assert_eq!(p.frame(Fx::from_int(10))[0], [255, 255, 255]);
    assert_eq!(p.frame(Fx::from_int(10))[0], [255, 255, 255]);

    // fps <= 0 removes the cap; the call returns the previous cap (0 = none)
    let mut off = Engine::new(
        &format!("export var a, b\n{COUNTER}\na = setFrameRate(10)\nb = setFrameRate(0)"),
        1,
        1,
    )
    .unwrap();
    for _ in 0..3 {
        off.frame(Fx::from_int(1));
    }
    assert_eq!(num(&off, "frames"), 3.0);
    assert_eq!(num(&off, "a"), 0.0);
    assert_eq!(num(&off, "b"), 10.0);

    // an absurdly slow rate clamps to a 60 s period rather than hanging
    let mut slow = Engine::new(&format!("setFrameRate(.0001)\n{COUNTER}"), 1, 1).unwrap();
    slow.frame(Fx::from_int(1));
    for _ in 0..60 {
        slow.frame(Fx::from_int(1000));
    }
    assert_eq!(num(&slow, "frames"), 2.0);

    // capping is real-time, so it survives a frozen clock (an interactive
    // pattern with timeScale(0) keeps being evaluated)
    let mut frozen = Engine::new(&format!("timeScale(0)\nsetFrameRate(50)\n{COUNTER}"), 1, 1)
        .unwrap();
    for _ in 0..5 {
        frozen.frame(Fx::from_int(10));
    }
    assert_eq!(num(&frozen, "frames"), 3.0); // t = 0, 20, 40 ms
    assert_eq!(frozen.time_ms(), 0);

    // a sync clock jump is not a frame delta
    let mut sync = Engine::new(COUNTER, 1, 1).unwrap();
    sync.frame(Fx::from_int(10));
    sync.set_time_ms(50_000);
    sync.frame(Fx::from_int(10));
    assert_eq!(num(&sync, "d"), 10.0);
}

#[test]
fn assert_gates_the_pattern() {
    // `assert(cond[, "message"])` runs inline in top-level init: a falsy
    // condition aborts init on the spot and blocks rendering, with the
    // message (or the condition's source text) as the error.
    let src = "export var before, after\n\
               before = 1\n\
               assert(floor(sqrt(pixelCount)) == sqrt(pixelCount), \"needs a square number of pixels\")\n\
               after = 1\n\
               export function render(i) { hsv(0, 0, 1) }";

    // satisfied (square count): runs, all of init happened
    let mut ok = Engine::new(src, 256, 1).unwrap();
    assert!(ok.take_error().is_none());
    assert!(!ok.requires_violated());
    ok.frame(Fx::ZERO);
    assert_eq!(ok.var("after"), Some(Value::Num(Fx::ONE)));

    // violated (non-square): init aborts AT the assert — statements above
    // it ran, statements below it did not — and the frame renders black
    let mut bad = Engine::new(src, 300, 1).unwrap();
    let err = bad.take_error().expect("invariant must fail at 300");
    assert!(err.is_assert);
    assert!(
        err.message.contains("needs a square number of pixels")
            && err.message.contains("pixelCount = 300"),
        "{}",
        err.message
    );
    assert!(bad.requires_violated());
    assert_eq!(bad.var("before"), Some(Value::Num(Fx::ONE)));
    assert_eq!(bad.var("after"), Some(Value::Num(Fx::ZERO)), "init must stop at the assert");
    let frame = bad.frame(Fx::ZERO).to_vec();
    assert!(frame.iter().all(|px| *px == [0, 0, 0]), "must render black");

    // without a custom message, the condition's source text is the message
    let src2 = "assert(pixelCount % 2 == 0)\nexport function render(i) { hsv(0,0,1) }";
    let mut odd = Engine::new(src2, 7, 1).unwrap();
    let err = odd.take_error().expect("odd count must fail");
    assert!(err.message.contains("pixelCount % 2 == 0"), "{}", err.message);
    // and the message survives the wire format's LEAN decode (devices)
    let blob = luxel_core::bytecode::serialize(
        &luxel_core::compile::compile(src2).unwrap(),
    )
    .unwrap();
    let mut e = Engine::from_program(
        luxel_core::bytecode::deserialize_lean(&blob).unwrap(),
        7,
        1,
    );
    let err = e.take_error().expect("invariant survives lean decode");
    assert!(err.is_assert);
    assert!(err.message.contains("pixelCount % 2 == 0"), "{}", err.message);
}

#[test]
fn assert_sees_vars_and_functions() {
    // assert is REAL init code: it runs in line, so it can use anything
    // initialized above it — vars, derived values, function calls.
    let src = "var w = sqrt(pixelCount)\n\
               function isInt(v) { return floor(v) == v }\n\
               assert(isInt(w), \"width must be a whole number\")\n\
               export function render(i) { hsv(0, 0, 1) }";
    let mut ok = Engine::new(src, 289, 1).unwrap();
    assert!(ok.take_error().is_none());
    let mut bad = Engine::new(src, 300, 1).unwrap();
    let err = bad.take_error().expect("must fail at 300");
    assert!(err.is_assert);
    assert!(err.message.contains("width must be a whole number"), "{}", err.message);
}

#[test]
fn assert_is_top_level_only() {
    // inside a function
    assert!(Engine::new(
        "export function render(i) { assert(pixelCount > 1) }",
        10,
        1
    )
    .is_err());
    // nested in a top-level block/branch
    assert!(Engine::new("if (pixelCount > 5) assert(pixelCount % 2 == 0)", 10, 1).is_err());
    assert!(Engine::new("{ assert(true) }", 10, 1).is_err());
    // strings exist ONLY as assert's message argument
    assert!(Engine::new("x = \"hello\"", 10, 1).is_err());
    // a runtime error INSIDE the condition is an ordinary vmerr, not a
    // violation — the engine stays usable (PB keeps going on init errors)
    let src = "a = array(3)\nassert(a[9] == 0)\nexport function render(i) { hsv(0,0,1) }";
    let mut e = Engine::new(src, 10, 1).unwrap();
    let err = e.take_error().expect("OOB in the condition");
    assert!(!err.is_assert);
    assert!(!e.requires_violated());
}

#[test]
fn map_only_pattern_gets_default_grid() {
    // A pattern that renders ONLY in 2D/3D gets a default ceil(√n) grid
    // map (PB-as-experienced: a real PB always has a map — oracle-verified
    // 2026-07-08); z (absent from a 2D map) still fills with midspace 0.5.
    let src = "export var ys\nexport var zs\n\
               export function render3D(index, x, y, z) { ys = y\n zs = z\n rgb(x, y, z) }";
    let mut e = Engine::new(src, 2, 1).unwrap();
    e.frame(Fx::ZERO);
    // 2 px → 2-wide grid, single row: y is 0 for every pixel
    assert_eq!(e.var("ys"), Some(Value::Num(Fx::ZERO)));
    assert_eq!(e.var("zs"), Some(Value::Num(Fx::from_raw(1 << 15))));
}

#[test]
fn default_grid_matches_pb_for_sqrt_matrix_patterns() {
    // The Breakout-2D class: grids sized by sqrt(pixelCount), indexed by
    // floor(coord · sqrt(pixelCount)). These are square-rig patterns: at a
    // square count they work; at a non-square count floor(1·√n) indexes one
    // past the truncated array — on a real PB just like here. The default
    // grid gives them PB-faithful coordinates; it does not (and must not)
    // paper over their square-count assumption.
    let src = "var w = sqrt(pixelCount)\n\
               var cells = array(w)\n\
               cells.mutate(() => array(w))\n\
               export function render2D(index, x, y) {\n\
                 v = cells[floor(y * w)][floor(x * w)]\n\
                 hsv(0, 0, v)\n\
               }";
    // square count: works
    let mut e = Engine::new(src, 256, 1).unwrap();
    e.frame(Fx::ZERO);
    assert!(e.take_error().is_none(), "square count must run clean");
    // non-square count: the same out-of-bounds a real PB reports
    let mut e = Engine::new(src, 300, 1).unwrap();
    e.frame(Fx::ZERO);
    let err = e.take_error().expect("non-square count OOBs, like PB");
    assert!(err.message.contains("out of bounds"), "{}", err.message);
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
    let src = include_str!("../../../library/blink-fade.js");
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
    let src = include_str!("../../../library/blink-fade.js");
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

// ---- map programs (map mode: per-pixel plot() → coordinate list) ----

#[test]
fn map_program_collects_2d_coords() {
    let mut e = Engine::new(
        "export function render(index) { plot(index, index * 2) }",
        4,
        1,
    )
    .unwrap();
    e.enable_map_mode();
    let paused = e.run_map();
    assert!(!paused, "no breakpoints → runs to completion");
    assert!(e.last_error.is_none());
    let (dims, coords) = e.map();
    assert_eq!(dims, 2);
    assert_eq!(coords.len(), 4);
    for i in 0..4 {
        assert_eq!(coords[i][0], Fx::from_int(i as i32));
        assert_eq!(coords[i][1], Fx::from_int(2 * i as i32));
        assert_eq!(coords[i][2], Fx::ZERO);
    }
}

#[test]
fn map_program_3d_when_plot_has_three_args() {
    let mut e = Engine::new(
        "export function render(index) { plot(index, 0, 1) }",
        3,
        1,
    )
    .unwrap();
    e.enable_map_mode();
    e.run_map();
    let (dims, coords) = e.map();
    assert_eq!(dims, 3);
    assert_eq!(coords[2][2], Fx::ONE);
}

#[test]
fn map_program_without_render_reports_error() {
    let mut e = Engine::new("x = 5", 4, 1).unwrap();
    e.enable_map_mode();
    let paused = e.run_map();
    assert!(!paused);
    assert!(e.take_error().is_some());
}

#[test]
fn map_program_is_debuggable() {
    // a breakpoint in the map program pauses the collection, just like a
    // pattern — same drive loop, so stepping works unchanged
    let mut e = Engine::new(
        "export function render(index) {\n  a = index\n  plot(a, 0)\n}",
        4,
        1,
    )
    .unwrap();
    e.enable_map_mode();
    e.debug_set_enabled(true);
    e.debug_set_breakpoints(&[3]); // the plot() line
    let paused = e.run_map();
    assert!(paused, "breakpoint should pause the map run");
    let (line, _, pixel) = e.debug_location().expect("a paused location");
    assert_eq!(line, 3);
    assert_eq!(pixel, Some(0));
}
