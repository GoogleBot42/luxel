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

// ── library patterns driven by injected events ───────────────────────────
// The `readEvent` surface is only useful if the shipped patterns that
// advertise it actually react. These lock that in: same seed, one engine
// poked and one left alone, and the poked cell must differ.

const GRID: u32 = 16;

/// A GRID×GRID 2D map, same construction as `luxel check --grid`.
fn grid_map(e: &mut Engine) {
    let coords: Vec<[Fx; 3]> = (0..GRID * GRID)
        .map(|i| {
            [
                Fx::from_int((i % GRID) as i32),
                Fx::from_int((i / GRID) as i32),
                Fx::ZERO,
            ]
        })
        .collect();
    e.set_map(2, &coords);
}

/// Normalized (x, y) of pixel `i` in the grid map above.
fn grid_xy(i: u32) -> (Fx, Fx) {
    let d = Fx::from_int((GRID - 1) as i32);
    (
        Fx::from_int((i % GRID) as i32) / d,
        Fx::from_int((i / GRID) as i32) / d,
    )
}

fn pointer_event(x: Fx, y: Fx) -> [Fx; 4] {
    [Fx::ONE, x, y, Fx::ONE]
}

#[test]
fn ripples_2d_event_splashes_at_the_poked_cell() {
    let src = include_str!("../../../library/ripples-2d.js");
    let delta = Fx::from_f64(1000.0 / 60.0);
    let target = 9 * GRID + 6; // an interior cell, away from the edges

    let mut poked = Engine::new(src, GRID * GRID, 7).unwrap();
    let mut control = Engine::new(src, GRID * GRID, 7).unwrap();
    grid_map(&mut poked);
    grid_map(&mut control);
    for _ in 0..30 {
        poked.frame(delta);
        control.frame(delta);
    }

    let (x, y) = grid_xy(target);
    poked.push_event(pointer_event(x, y));
    let after: [u8; 3] = poked.frame(delta)[target as usize];
    let before: [u8; 3] = control.frame(delta)[target as usize];

    assert!(poked.last_error.is_none(), "{:?}", poked.last_error);
    // A drop restarts at the poke: dist 0 with phase 0 is the ring's peak,
    // which renders near-white (hsv sat 0.4, value clamped to 1).
    let sum: u32 = after.iter().map(|&c| c as u32).sum();
    assert!(sum > 500, "poked cell should flash bright, got {after:?}");
    assert_ne!(after, before, "the event changed nothing");
}

#[test]
fn slime_mold_event_seeds_the_poked_cell() {
    let src = include_str!("../../../library/slime-mold-palette.js");
    let delta = Fx::from_f64(1000.0 / 60.0);

    let mut control = Engine::new(src, GRID * GRID, 7).unwrap();
    grid_map(&mut control);
    control.frame(delta);
    control.frame(delta);
    // A cell the growth has not reached by frame 2 — unpainted renders black.
    let target = control
        .pixels()
        .iter()
        .position(|p| *p == [0, 0, 0])
        .expect("early frames leave most of the canvas unpainted") as u32;

    let mut poked = Engine::new(src, GRID * GRID, 7).unwrap();
    grid_map(&mut poked);
    poked.frame(delta);
    let (x, y) = grid_xy(target);
    poked.push_event(pointer_event(x, y));
    let after: [u8; 3] = poked.frame(delta)[target as usize];

    assert!(poked.last_error.is_none(), "{:?}", poked.last_error);
    assert_ne!(after, [0, 0, 0], "the poked cell should have been seeded");
}

#[test]
fn saberdeploy_event_reverses_the_blade() {
    let src = include_str!("../../../library/saberdeploy-tutorial.js");
    let delta = Fx::from_f64(1000.0 / 60.0);
    let mut e = Engine::new(src, 60, 1).unwrap();
    for _ in 0..10 {
        e.frame(delta);
    }
    assert_eq!(e.var("dir"), Some(Value::Num(Fx::ONE)), "starts extending");

    e.push_event(pointer_event(Fx::ZERO, Fx::ZERO));
    e.frame(delta);
    assert_eq!(
        e.var("dir"),
        Some(Value::Num(-Fx::ONE)),
        "an injected press should reverse the blade"
    );
    assert!(e.last_error.is_none(), "{:?}", e.last_error);

    // A burst inside one frame is still one press, not a cancelling pair.
    e.push_event(pointer_event(Fx::ZERO, Fx::ZERO));
    e.push_event(pointer_event(Fx::ZERO, Fx::ZERO));
    e.frame(delta);
    assert_eq!(e.var("dir"), Some(Value::Num(Fx::ONE)));
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
fn render_error_records_and_continues() {
    let src = "export function render(i) { if (i == 1) badCall()\n hsv(0, 0, 1) }";
    // badCall is unknown → compile error; use a runtime one instead
    assert!(Engine::new(src, 2, 1).is_err());
    let src = "f = 0\nexport function render(i) { if (i == 1) f()\n hsv(0, 0, 1) }";
    let mut e = Engine::new(src, 3, 1).unwrap();
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [255, 255, 255]); // rendered before the error
    assert_eq!(px[1], [0, 0, 0]); // errored before its hsv → stays dark
    assert_eq!(px[2], [255, 255, 255]); // later pixels still render (#84)
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

// ---- Runtime-error blast radius (Gitea #84) ----
//
// Oracle fw 3.67 (tools/oracle/oob-probes.mjs, 2026-08-22): a pattern-level
// runtime error aborts only the current handler invocation. After a
// beforeRender abort the pixel pass still runs (writes made before the
// abort stick), and a render(i) error keeps that pixel's pre-error hsv/rgb
// while later pixels render normally. Only VM resource guards (and
// init-time assert()) end a frame early.

#[test]
fn before_render_error_still_runs_the_pixel_pass() {
    // Nano Orbital's shape: an OOB *write* in beforeRender every frame
    let mut e = Engine::new(
        "a = array(3)\n\
         export var before, after\n\
         export function beforeRender(delta) {\n  before = 7\n  a[5] = 1\n  after = 7\n}\n\
         export function render(index) { rgb(0, 1, 0) }",
        4,
        1,
    )
    .unwrap();
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [0, 255, 0], "render must still run after the abort");
    assert_eq!(px[3], [0, 255, 0]);
    // abort is mid-body: earlier writes stick, later ones never happen
    assert_eq!(e.var("before"), Some(Value::Num(Fx::from_int(7))));
    assert_eq!(e.var("after"), Some(Value::Num(Fx::ZERO)));
    let err = e.take_error().expect("the OOB is still reported");
    assert!(err.message.contains("out of bounds"), "{}", err.message);
}

#[test]
fn render_error_aborts_only_that_pixel() {
    let mut e = Engine::new(
        "a = array(3)\n\
         export function render(index) {\n\
           rgb(1, 0, 0)\n\
           if (index == 2) { x = a[9] }\n\
           rgb(0, 1, 0)\n\
         }",
        4,
        1,
    )
    .unwrap();
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [0, 255, 0]);
    assert_eq!(px[1], [0, 255, 0]);
    // the erroring pixel keeps what it set before the abort
    assert_eq!(px[2], [255, 0, 0]);
    // and later pixels still render
    assert_eq!(px[3], [0, 255, 0]);
    assert!(e.take_error().is_some());
}

#[test]
fn fractional_array_loop_renders_despite_oob() {
    // Orv - Christmas Tree's shape: array(pixelCount/20) is truncated (3
    // slots here, oracle-matched) but `i < 3.2` lets i reach 3 → an OOB
    // read every frame. The pattern must still render.
    let mut e = Engine::new(
        "n = pixelCount / 20\n\
         a = array(n)\n\
         export var out\n\
         export function beforeRender(delta) {\n\
           for (i = 0; i < n; i++) { out = a[i] }\n\
         }\n\
         export function render(index) { rgb(0, 1, 0) }",
        64,
        1,
    )
    .unwrap();
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [0, 255, 0]);
    assert_eq!(px[63], [0, 255, 0]);
    let err = e.take_error().expect("the OOB read is reported");
    assert!(err.message.contains("out of bounds"), "{}", err.message);
}

#[test]
fn first_error_of_a_frame_wins() {
    // beforeRender errors first; the per-pixel errors must not overwrite it
    let mut e = Engine::new(
        "a = array(3)\n\
         export function beforeRender(delta) { a[5] = 1 }\n\
         export function render(index) { x = a[7] }",
        4,
        1,
    )
    .unwrap();
    e.frame(Fx::ZERO);
    let err = e.take_error().expect("errors are recorded");
    assert_eq!(err.line, 2, "the beforeRender error is the one kept");
}

#[test]
fn resource_guards_still_end_the_frame() {
    // an infinite loop in beforeRender must not re-run per pixel — the
    // step limit ends the whole frame (watchdog economics on-device)
    let mut e = Engine::new(
        "export function beforeRender(delta) { while (1) { } }\n\
         export function render(index) { rgb(0, 1, 0) }",
        4,
        1,
    )
    .unwrap();
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [0, 0, 0], "render pass is skipped on a guard trip");
    let err = e.take_error().expect("guard trip is recorded");
    assert!(err.message.contains("execution limit"), "{}", err.message);
    assert!(err.is_resource_guard());
}

// ---- Out-of-range writes are rejected, not tolerated (Gitea #107) ----
//
// Oracle fw 3.67 (tools/oracle/oob-probes.mjs Q5-Q7, 2026-08-29): PB does
// not clamp, wrap or silently no-op an out-of-range write, the pattern
// survives an erroring frame, and calling a never-assigned array slot is an
// abort rather than a no-op. These pin the "tolerance" question closed:
// anything that made an out-of-range access quietly succeed here would be a
// divergence from the device, not a compatibility fix.

#[test]
fn rejected_out_of_range_write_leaves_the_array_untouched() {
    // array(4) written at index 6 separates the hypotheses: clamp would
    // land in slot 3, wrap (6 % 4) in slot 2. The oracle reports every slot
    // still 0. The slots are read back in the NEXT invocation because the
    // write aborts this one.
    let mut e = Engine::new(
        "a = array(4)\n\
         export var phase, s0, s1, s2, s3\n\
         export function beforeRender(delta) {\n\
           if (phase == 0) { phase = 1\n  a[6] = 1 }\n\
           else { s0 = a[0]\n  s1 = a[1]\n  s2 = a[2]\n  s3 = a[3] }\n\
         }\n\
         export function render(index) { rgb(0, 0, 0) }",
        4,
        1,
    )
    .unwrap();
    e.frame(Fx::ZERO);
    let err = e.take_error().expect("the out-of-range write is reported");
    assert!(err.message.contains("out of bounds"), "{}", err.message);
    e.frame(Fx::ZERO);
    assert!(e.take_error().is_none(), "the read-back frame is clean");
    for slot in ["s0", "s1", "s2", "s3"] {
        assert_eq!(
            e.var(slot),
            Some(Value::Num(Fx::ZERO)),
            "{slot} was mutated by the rejected write"
        );
    }
}

#[test]
fn rejected_replace_span_leaves_the_array_untouched() {
    // Oracle Q8f: an overrunning arrayReplaceAt writes NOTHING — not even
    // the elements that would have landed in bounds (here slots 2 and 3
    // before the one that runs off the end).
    let mut e = Engine::new(
        "b = array(4)\n\
         export var phase, s0, s1, s2, s3\n\
         export function beforeRender(delta) {\n\
           if (phase == 0) { phase = 1\n  arrayReplaceAt(b, 2, 7, 8, 9) }\n\
           else { s0 = b[0]\n  s1 = b[1]\n  s2 = b[2]\n  s3 = b[3] }\n\
         }\n\
         export function render(index) { rgb(0, 0, 0) }",
        4,
        1,
    )
    .unwrap();
    e.frame(Fx::ZERO);
    let err = e.take_error().expect("the overrun is reported");
    assert!(err.message.contains("out of bounds"), "{}", err.message);
    e.frame(Fx::ZERO);
    for slot in ["s0", "s1", "s2", "s3"] {
        assert_eq!(
            e.var(slot),
            Some(Value::Num(Fx::ZERO)),
            "{slot} was written by the rejected splat"
        );
    }
}

#[test]
fn pattern_survives_an_erroring_frame() {
    // rainbow-comet's shape: it goes out of range at exactly one frame and
    // must keep rendering afterwards. Oracle Q6: with an out-of-range write
    // every third invocation, the frame counter keeps advancing.
    let mut e = Engine::new(
        "a = array(3)\n\
         export var frames\n\
         export function beforeRender(delta) {\n\
           frames = frames + 1\n\
           if (frames % 3 == 0) { a[9] = 1 }\n\
         }\n\
         export function render(index) { rgb(0, 1, 0) }",
        4,
        1,
    )
    .unwrap();
    for f in 1..=9 {
        let px = e.frame(Fx::ZERO);
        assert_eq!(px[0], [0, 255, 0], "frame {f} must still render");
        assert_eq!(e.take_error().is_some(), f % 3 == 0, "frame {f} error");
    }
    assert_eq!(e.var("frames"), Some(Value::Num(Fx::from_int(9))));
}

#[test]
fn calling_an_unset_array_slot_aborts_only_that_invocation() {
    // tixy's shape: the pattern walks off the end of its table of function
    // values into a never-assigned slot, which holds 0, and calls it.
    // Oracle Q7: PB aborts — it does not treat the 0 as a no-op — so the
    // black frames such a pattern produces are faithful, and the pattern
    // renders again as soon as the index comes back in range.
    let mut e = Engine::new(
        "t = array(4)\n\
         t[0] = (v) => v\n\
         export var frames\n\
         export function beforeRender(delta) { frames = frames + 1 }\n\
         export function render(index) { rgb(0, t[frames % 2](1), 0) }",
        4,
        1,
    )
    .unwrap();
    let px = e.frame(Fx::ZERO); // frames == 1 → slot 1 is unset
    assert_eq!(px[0], [0, 0, 0], "the bad slot renders black");
    let err = e.take_error().expect("calling 0 is an error");
    assert!(err.message.contains("non-function"), "{}", err.message);
    let px = e.frame(Fx::ZERO); // frames == 2 → back to slot 0
    assert_eq!(px[0], [0, 255, 0], "the next frame renders normally");
    assert!(e.take_error().is_none());
}

// ---- PB array element ledger (oracle-bisected 2026-08-29, fw 3.67):
// 10,236-unit budget, every array costs its length + a 4-unit header.
// Boundary numbers below are the measured device answers, not derivations.

#[test]
fn array_budget_pb_boundaries() {
    let ok = |src: &str| {
        let e = Engine::new(src, 1, 1).unwrap();
        assert!(e.last_error.is_none(), "{src}: {:?}", e.last_error);
    };
    let over = |src: &str| {
        let e = Engine::new(src, 1, 1).unwrap();
        let m = &e.last_error.as_ref().expect("expected budget error").message;
        assert!(m.contains("array element budget"), "{src}: {m}");
    };
    // largest single array a real PB accepts is 10,232
    ok("a = array(10232)\nexport function render(i) { rgb(0,0,0) }");
    over("a = array(10233)\nexport function render(i) { rgb(0,0,0) }");
    // per-array headers: 5113+5113 fits while 5116+5116 aborts on PB,
    // even though both sums are under the single-array maximum
    ok("a = array(5113)\nb = array(5113)\nexport function render(i) { rgb(0,0,0) }");
    over("a = array(5116)\nb = array(5116)\nexport function render(i) { rgb(0,0,0) }");
}

#[test]
fn per_frame_allocation_exhausts_like_pb() {
    // array(100) per frame: the oracle survived exactly 98 frames
    // (98·104 = 10,192 ≤ 10,236 < 99·104); ours must die on the same frame
    let src = "export var frames\n\
               c = 0\n\
               export function beforeRender(delta) { t = array(100)\n c = c + 1\n frames = c }\n\
               export function render(i) { rgb(0,0,0) }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    for _ in 0..120 {
        e.frame(Fx::from_int(10));
    }
    assert_eq!(e.var("frames"), Some(Value::Num(Fx::from_int(98))));
    let err = e.take_error().expect("budget error recorded");
    assert!(err.message.contains("array element budget"), "{}", err.message);
}

// ---- zero-length arrays and the arena slot vector (Gitea #124) ----
//
// `array_byte_budget` is `usize::MAX` on hosts, so the ELEMENT ledger is the
// only thing standing between `while (1) t = array(0)` and host OOM. It
// holds because every array — length 0 included — charges
// ARRAY_HEADER_UNITS, capping the arena at
// DEFAULT_ARRAY_BUDGET / ARRAY_HEADER_UNITS = 2,559 slots.

/// Cap implied by the header charge; the tests below must not drift from it.
const ARENA_SLOT_CAP: usize =
    luxel_core::vm::DEFAULT_ARRAY_BUDGET / luxel_core::vm::ARRAY_HEADER_UNITS;

#[test]
fn array0_in_a_tight_loop_cannot_grow_the_arena_unboundedly() {
    // one frame, 10k attempted allocations: the arena must stop at the cap
    // and report the budget error rather than growing with the loop
    let src = "export var n\n\
               export function beforeRender(delta) {\n\
                 i = 0\n n = 0\n\
                 while (i < 10000) { t = array(0)\n i = i + 1\n n = i }\n\
               }\n\
               export function render(i) { rgb(0,0,0) }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    e.frame(Fx::from_int(10));
    let (slots, elems, bytes) = e.arena_stats();
    assert_eq!(slots, ARENA_SLOT_CAP, "arena slot vector must stop at the cap");
    assert_eq!(elems, ARENA_SLOT_CAP * luxel_core::vm::ARRAY_HEADER_UNITS);
    assert!(bytes <= 128 * 1024, "arena bytes unbounded: {bytes}");
    // the loop aborted at the cap, not after all 10,000 iterations
    assert_eq!(e.var("n"), Some(Value::Num(Fx::from_int(ARENA_SLOT_CAP as i32))));
    let err = e.take_error().expect("budget error recorded");
    assert!(err.message.contains("array element budget"), "{}", err.message);
}

#[test]
fn array0_per_frame_stops_growing_the_arena() {
    // the per-frame shape from the issue: running well past the cap's worth
    // of frames must leave the arena at the same cap a single frame reaches
    let src = "export function beforeRender(delta) { t = array(0) }\n\
               export function render(i) { rgb(0,0,0) }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    for _ in 0..ARENA_SLOT_CAP + 500 {
        e.frame(Fx::from_int(10));
    }
    assert_eq!(e.arena_stats().0, ARENA_SLOT_CAP);
    let err = e.take_error().expect("budget error recorded");
    assert!(err.message.contains("array element budget"), "{}", err.message);
}

#[test]
fn empty_array_literals_charge_the_header_too() {
    // `[]` takes the const-pool path (CONST_ARR), a separate allocator from
    // `array(0)`'s NEW_ARRAY — it must charge the same 4-unit header
    let src = "export function beforeRender(delta) { t = [] }\n\
               export function render(i) { rgb(0,0,0) }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    for _ in 0..ARENA_SLOT_CAP + 100 {
        e.frame(Fx::from_int(10));
    }
    assert_eq!(e.arena_stats().0, ARENA_SLOT_CAP);
    assert!(e
        .take_error()
        .expect("budget error recorded")
        .message
        .contains("array element budget"));
}

#[test]
fn a_fresh_engine_starts_with_an_empty_arena() {
    // pattern switch = new Vm: the arena a runaway pattern filled must not
    // follow the next one (the playground reloads through Engine::new)
    let src = "export function beforeRender(delta) { t = array(0) }\n\
               export function render(i) { rgb(0,0,0) }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    for _ in 0..ARENA_SLOT_CAP + 10 {
        e.frame(Fx::from_int(10));
    }
    assert_eq!(e.arena_stats().0, ARENA_SLOT_CAP);
    let fresh = Engine::new(src, 1, 1).unwrap();
    assert_eq!(fresh.arena_stats(), (0, 0, 0));
}

// ---- const→owned copy-on-write promotion and the byte ledger (Gitea #132) ----
//
// A `[…]` literal shares the program's data until the first write, so it only
// charges CONST_ENTRY_COST bytes. Materializing it adds the element bytes, and
// that delta has to clear `array_byte_budget` BEFORE the copy is made — on a
// device-budgeted VM the unchecked add used to push the ledger past its cap.

const COW_SRC: &str = "a = [1, 2, 3, 4, 5, 6, 7, 8]\n\
                       export var v\n\
                       export function beforeRender(delta) {\n  a[0] = 5\n  v = a[0]\n}\n\
                       export function render(index) { rgb(0, 1, 0) }";

fn cow_engine(array_byte_budget: usize) -> Engine {
    let prog = luxel_core::compile::compile(COW_SRC).unwrap();
    Engine::from_program_budgeted(prog, 2, 1, array_byte_budget)
}

#[test]
fn cow_promotion_within_budget_charges_the_delta() {
    let mut e = cow_engine(usize::MAX);
    let at_init = e.arena_stats().2;
    e.frame(Fx::from_int(10));
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
    let promoted = e.arena_stats().2;
    assert!(promoted > at_init, "the owned copy must join the byte ledger");
    assert_eq!(e.var("v"), Some(Value::Num(Fx::from_int(5))));

    // an exactly-sufficient budget behaves identically
    let mut tight = cow_engine(promoted);
    tight.frame(Fx::from_int(10));
    assert!(tight.last_error.is_none(), "{:?}", tight.last_error);
    assert_eq!(tight.arena_stats().2, promoted, "same delta charged");
    assert_eq!(tight.var("v"), Some(Value::Num(Fx::from_int(5))));
}

#[test]
fn cow_promotion_at_the_budget_edge_errors_instead_of_overshooting() {
    let mut probe = cow_engine(usize::MAX);
    let at_init = probe.arena_stats().2;
    probe.frame(Fx::from_int(10));
    let promoted = probe.arena_stats().2;

    // one byte short of what materializing the copy needs
    let budget = promoted - 1;
    let mut e = cow_engine(budget);
    assert_eq!(e.arena_stats().2, at_init, "the const entry itself still fits");
    let px0 = e.frame(Fx::from_int(10))[0];
    let err = e.take_error().expect("the refused promotion is reported");
    assert!(err.message.contains("array memory budget"), "{}", err.message);
    // the ledger stayed inside its cap — the bug was a silent overshoot
    let bytes = e.arena_stats().2;
    assert!(bytes <= budget, "byte ledger overshot the budget: {bytes} > {budget}");
    assert_eq!(bytes, at_init, "a refused promotion charges nothing");
    // the write never landed, and the blast radius is PB-shaped (#84): the
    // handler invocation aborts, the pixel pass still runs
    assert_eq!(e.var("v"), Some(Value::Num(Fx::ZERO)));
    assert_eq!(px0, [0, 255, 0]);
    assert!(!err.is_resource_guard());
}

// ---- setPalette live-aliasing (oracle-confirmed 2026-08-29,
// tools/oracle/alias-probes.mjs: in-place writes through the installed
// array changed paint()'s output with no second setPalette call) ----

#[test]
fn set_palette_live_aliases_the_array() {
    let src = "export var f\n\
               p = array(8)\n\
               p[1] = 1\n\
               p[4] = 1\n\
               p[5] = 1\n\
               setPalette(p)\n\
               f = 0\n\
               export function beforeRender(delta) {\n\
                 f = f + 1\n\
                 if (f > 1) {\n\
                   p[1] = 0\n\
                   p[3] = 1\n\
                   p[5] = 0\n\
                   p[7] = 1\n\
                 }\n\
               }\n\
               export function render(index) { paint(0.5, 1) }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    let px = e.frame(Fx::from_int(10));
    assert_eq!(px[0], [255, 0, 0], "initial palette is red");
    let px = e.frame(Fx::from_int(10));
    assert_eq!(px[0], [0, 0, 255], "in-place writes turn it blue, no re-call");
    assert!(e.last_error.is_none());
}

// ---- late-bound render dispatch (oracle-confirmed 2026-08-29, same
// probe battery: a pattern with NO exported render function renders via
// `export var render`, and re-assigning it swaps live) ----

#[test]
fn render_via_export_var_dispatches_and_reswaps() {
    let src = "export var f, render\n\
               function red(index) { rgb(1, 0, 0) }\n\
               function green(index) { rgb(0, 1, 0) }\n\
               f = 0\n\
               export function beforeRender(delta) {\n\
                 f = f + 1\n\
                 if (f % 2 == 1) { render = red } else { render = green }\n\
               }";
    let mut e = Engine::new(src, 2, 1).unwrap();
    let px = e.frame(Fx::from_int(10));
    assert_eq!(px[0], [255, 0, 0]);
    let px = e.frame(Fx::from_int(10));
    assert_eq!(px[0], [0, 255, 0], "re-assignment swaps the entry live");
    assert!(e.last_error.is_none());
}

#[test]
fn render2d_via_export_var_gets_default_grid_map() {
    // slime-mold-palette's shape: only a runtime-assigned render2D — it
    // must still get the default grid map a static render2D would get
    let src = "export var render2D\n\
               function pix(index, x, y) { rgb(1, 1, 1) }\n\
               export function beforeRender(delta) { render2D = pix }";
    let mut e = Engine::new(src, 4, 1).unwrap();
    let px = e.frame(Fx::from_int(10));
    assert_eq!(px[0], [255, 255, 255]);
    assert!(e.last_error.is_none());
}

// ---- pixelState / setPixelState: the engine-owned per-pixel state buffer ----
//
// Double-buffered feedback without hand-rolled arrays: reads see LAST frame's
// committed values (frame-consistent, neighbours included), writes land in
// next frame's buffer, unwritten pixels carry over. The buffer exists only
// after the first write — an untouched pattern pays nothing.

#[test]
fn pixel_state_costs_nothing_until_written() {
    let mut e = Engine::new(RAINBOW, 8, 1).unwrap();
    e.frame(Fx::from_int(10));
    assert_eq!(e.pixel_state_bytes(), 0, "no state → no buffer");

    // a read-only pattern gets 0 and still allocates nothing
    let mut r = Engine::new(
        "export var got\nexport function render(index) { got = pixelState(index) + pixelState(index, 3) }",
        8,
        1,
    )
    .unwrap();
    r.frame(Fx::from_int(10));
    assert_eq!(r.var("got"), Some(Value::Num(Fx::ZERO)));
    assert_eq!(r.pixel_state_bytes(), 0, "reads never allocate");
    assert!(r.last_error.is_none(), "{:?}", r.last_error);
}

#[test]
fn pixel_state_reads_last_frame_and_writes_next() {
    let src = "export var got\n\
               export function render(index) {\n\
                 got = pixelState(0)\n\
                 setPixelState(0, pixelState(0) + 1)\n\
               }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    for expect in 0..4 {
        e.frame(Fx::from_int(10));
        assert_eq!(e.var("got"), Some(Value::Num(Fx::from_int(expect))));
    }
    // 1 px × 1 channel × 4 B × 2 buffers, and it sits on the arena byte ledger
    assert_eq!(e.pixel_state_bytes(), 8);
    assert!(e.arena_stats().2 >= 8);
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
}

#[test]
fn pixel_state_reads_are_frame_consistent_across_neighbours() {
    // frame 1 writes state[i] = i + 1. In frame 2, render(1) runs AFTER
    // render(0) has already written a new value for pixel 0 — but must still
    // read pixel 0's frame-1 value (the front buffer is a snapshot).
    let src = "export var a, b\n\
               export function render(index) {\n\
                 if (index == 0) { a = pixelState(1) } else { b = pixelState(0) }\n\
                 setPixelState(index, index + 1 + 10 * pixelState(index))\n\
               }";
    let mut e = Engine::new(src, 2, 1).unwrap();
    e.frame(Fx::from_int(10)); // state → [1, 2]
    e.frame(Fx::from_int(10)); // reads see [1, 2]; state → [11, 22]
    assert_eq!(e.var("a"), Some(Value::Num(Fx::from_int(2))));
    assert_eq!(e.var("b"), Some(Value::Num(Fx::from_int(1))));
    e.frame(Fx::from_int(10));
    assert_eq!(e.var("a"), Some(Value::Num(Fx::from_int(22))));
    assert_eq!(e.var("b"), Some(Value::Num(Fx::from_int(11))));
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
}

#[test]
fn pixel_state_carries_over_unwritten_pixels() {
    // only the first frame writes (from beforeRender); later frames read the
    // same value back for as long as nobody overwrites it
    let src = "export var f = 0, got\n\
               export function beforeRender(delta) {\n\
                 f += 1\n  if (f == 1) setPixelState(1, 5)\n\
               }\n\
               export function render(index) { if (index == 1) got = pixelState(1) }";
    let mut e = Engine::new(src, 2, 1).unwrap();
    e.frame(Fx::from_int(10));
    for _ in 0..3 {
        e.frame(Fx::from_int(10));
        assert_eq!(e.var("got"), Some(Value::Num(Fx::from_int(5))));
    }
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
}

#[test]
fn pixel_state_seeded_at_init_is_visible_in_the_first_frame() {
    // top-level init runs before the engine sets vm.pixel_count — the buffer
    // must still be sized from pixelCount, and the seed must survive
    let src = "for (i = 0; i < pixelCount; i++) setPixelState(i, i * 2)\n\
               export var got\n\
               export function render(index) { if (index == 3) got = pixelState(3) }";
    let mut e = Engine::new(src, 4, 1).unwrap();
    assert_eq!(e.pixel_state_bytes(), 4 * 4 * 2);
    e.frame(Fx::from_int(10));
    assert_eq!(e.var("got"), Some(Value::Num(Fx::from_int(6))));
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
}

#[test]
fn pixel_state_channels_grow_on_demand_and_keep_existing_values() {
    let src = "export var f = 0, c0, c1\n\
               export function beforeRender(delta) {\n\
                 f += 1\n\
                 if (f == 1) setPixelState(0, 7)\n\
                 if (f == 2) setPixelState(0, 2, 9)\n\
                 c0 = pixelState(0)\n  c1 = pixelState(0, 2)\n\
               }\n\
               export function render(index) { rgb(0, 0, 0) }";
    let mut e = Engine::new(src, 3, 1).unwrap();
    e.frame(Fx::from_int(10));
    assert_eq!(e.pixel_state_bytes(), 3 * 1 * 4 * 2, "one channel");
    e.frame(Fx::from_int(10));
    assert_eq!(e.pixel_state_bytes(), 3 * 3 * 4 * 2, "grown to channels 0..2");
    e.frame(Fx::from_int(10));
    assert_eq!(e.var("c0"), Some(Value::Num(Fx::from_int(7))), "channel 0 survives growth");
    assert_eq!(e.var("c1"), Some(Value::Num(Fx::from_int(9))));
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
}

#[test]
fn pixel_state_out_of_range_index_is_harmless_but_bad_channel_errors() {
    let src = "export var lo, hi, ret\n\
               export function render(index) {\n\
                 ret = setPixelState(99, 3)\n  setPixelState(-1, 3)\n\
                 lo = pixelState(-1)\n  hi = pixelState(99)\n\
               }";
    let mut e = Engine::new(src, 2, 1).unwrap();
    e.frame(Fx::from_int(10));
    e.frame(Fx::from_int(10));
    assert_eq!(e.var("lo"), Some(Value::Num(Fx::ZERO)));
    assert_eq!(e.var("hi"), Some(Value::Num(Fx::ZERO)));
    assert_eq!(e.var("ret"), Some(Value::Num(Fx::from_int(3))), "returns v like an assignment");
    assert!(e.last_error.is_none(), "{:?}", e.last_error);

    let mut bad = Engine::new("export function render(index) { setPixelState(0, 4, 1) }", 2, 1)
        .unwrap();
    bad.frame(Fx::from_int(10));
    let msg = bad.last_error.as_ref().map(|e| e.message.clone()).unwrap_or_default();
    assert!(msg.contains("channel 4 out of range"), "{msg}");
    assert_eq!(bad.pixel_state_bytes(), 0, "a rejected channel allocates nothing");
}

#[test]
fn pixel_state_respects_the_device_byte_budget() {
    // 2048 px × 4 B × 2 = 16 KB; a 4 KB budget must reject with a recorded
    // error and allocate nothing — never panic (= reboot) on the device
    let src = "export function render(index) { setPixelState(index, 1) }";
    let prog = luxel_core::compile::compile(src).unwrap();
    let mut e = Engine::from_program_budgeted(prog, 2048, 1, 4 * 1024);
    e.frame(Fx::from_int(10));
    let msg = e.last_error.as_ref().map(|e| e.message.clone()).unwrap_or_default();
    assert!(msg.contains("budget"), "{msg}");
    assert_eq!(e.pixel_state_bytes(), 0);

    let prog = luxel_core::compile::compile(src).unwrap();
    let mut ok = Engine::from_program_budgeted(prog, 2048, 1, 16 * 1024);
    ok.frame(Fx::from_int(10));
    assert!(ok.last_error.is_none(), "{:?}", ok.last_error);
    assert_eq!(ok.pixel_state_bytes(), 16 * 1024);
}
