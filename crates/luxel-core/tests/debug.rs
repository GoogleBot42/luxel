//! Debugger behavior: breakpoints, stepping, stack/locals inspection, and
//! source locations on runtime errors.

use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::vm::{StepKind, Value};

// line numbers:                       1
const SRC: &str = "\
export function beforeRender(delta) {
  t = helper(5)
  u = t + 1
}
function helper(v) {
  w = v * 2
  return w
}
export function render(index) {
  hsv(0, 0, index / pixelCount)
}
";
// helper body: line 6; render hsv: line 10

fn debug_engine() -> Engine {
    let mut e = Engine::new(SRC, 4, 1).unwrap();
    assert!(e.last_error.is_none());
    e.debug_set_enabled(true);
    e
}

#[test]
fn breakpoint_pauses_each_pixel() {
    let mut e = debug_engine();
    assert_eq!(e.debug_set_breakpoints(&[10]), vec![10]);
    e.frame(Fx::ZERO);
    assert!(e.debug_paused());
    let (line, _col, pixel) = e.debug_location().unwrap();
    assert_eq!(line, 10);
    assert_eq!(pixel, Some(0));
    let stack = e.debug_stack();
    assert_eq!(stack.len(), 1);
    assert_eq!(stack[0].name, "render");
    assert_eq!(
        stack[0].locals.first(),
        Some(&("index".to_string(), Value::Num(Fx::ZERO)))
    );

    // continue → next pixel hits the same breakpoint
    assert!(e.debug_step(StepKind::Continue));
    assert_eq!(e.debug_location().unwrap().2, Some(1));
    let stack = e.debug_stack();
    assert_eq!(
        stack[0].locals.first(),
        Some(&("index".to_string(), Value::Num(Fx::ONE)))
    );

    // continue through the remaining pixels finishes the frame
    assert!(e.debug_step(StepKind::Continue));
    assert!(e.debug_step(StepKind::Continue));
    assert!(!e.debug_step(StepKind::Continue));
    assert!(!e.debug_paused());
    // and the frame rendered correctly despite all the pausing
    assert_eq!(e.pixels()[2], [128, 128, 128]); // v = 2/4
    assert!(e.last_error.is_none());
}

#[test]
fn step_over_stays_at_call_level() {
    let mut e = debug_engine();
    e.debug_set_breakpoints(&[2]);
    e.frame(Fx::ZERO);
    assert_eq!(e.debug_location().unwrap().0, 2);
    // over: helper() runs to completion, we land on line 3
    e.debug_step(StepKind::Over);
    assert_eq!(e.debug_location().unwrap().0, 3);
    assert_eq!(e.debug_stack().len(), 1);
    // stepping past the end of beforeRender flows into render(0)
    e.debug_step(StepKind::Over);
    let (line, _c, pixel) = e.debug_location().unwrap();
    assert_eq!(line, 10);
    assert_eq!(pixel, Some(0));
}

#[test]
fn step_into_and_out() {
    let mut e = debug_engine();
    e.debug_set_breakpoints(&[2]);
    e.frame(Fx::ZERO);
    // into: land inside helper
    e.debug_step(StepKind::Into);
    assert_eq!(e.debug_location().unwrap().0, 6);
    let stack = e.debug_stack();
    assert_eq!(stack.len(), 2);
    assert_eq!(stack[0].name, "helper");
    assert_eq!(stack[1].name, "beforeRender");
    assert_eq!(
        stack[0].locals.first(),
        Some(&("v".to_string(), Value::Num(Fx::from_int(5))))
    );
    // parent frame shows the call site line
    assert_eq!(stack[1].line, 2);
    // out: back in beforeRender, still on the call line (finishing the
    // `t = helper(5)` assignment — standard debugger behavior)
    e.debug_step(StepKind::Out);
    assert_eq!(e.debug_location().unwrap().0, 2);
    assert_eq!(e.debug_stack().len(), 1);
    e.debug_step(StepKind::Over);
    assert_eq!(e.debug_location().unwrap().0, 3);
}

#[test]
fn pause_request_stops_next_frame() {
    let mut e = debug_engine();
    e.debug_pause();
    e.frame(Fx::ZERO);
    assert!(e.debug_paused());
    assert_eq!(e.debug_stack()[0].name, "beforeRender");
    // frame() while paused is a no-op (time frozen)
    e.frame(Fx::from_int(1000));
    assert!(e.debug_paused());
    // continue to completion
    while e.debug_step(StepKind::Continue) {}
    assert!(!e.debug_paused());
}

#[test]
fn disable_abandons_paused_run() {
    let mut e = debug_engine();
    e.debug_set_breakpoints(&[10]);
    e.frame(Fx::ZERO);
    assert!(e.debug_paused());
    e.debug_set_enabled(false);
    assert!(!e.debug_paused());
    // engine renders normally afterwards
    let px = e.frame(Fx::from_f64(16.7));
    assert_eq!(px[2], [128, 128, 128]);
    assert!(e.last_error.is_none());
}

#[test]
fn breakpoints_on_blank_lines_snap_forward() {
    // non-executable lines snap forward to the next executable one (so a
    // gutter dot never silently points at a line that can't stop)
    let mut e = debug_engine();
    let resolved = e.debug_set_breakpoints(&[4, 10]);
    assert!(resolved.contains(&10));
    assert!(!resolved.contains(&4)); // line 4 itself holds no statement
    assert!(resolved.iter().all(|&l| l >= 4));
}

#[test]
fn runtime_errors_carry_source_location() {
    let src = "\
export function render(index) {
  a = [1, 2]
  x = a[5]
}
";
    let mut e = Engine::new(src, 2, 1).unwrap();
    e.frame(Fx::ZERO);
    let err = e.take_error().unwrap();
    assert!(err.message.contains("out of bounds"));
    assert_eq!(err.line, 3);
    assert!(err.col >= 1);
}

#[test]
fn stepping_is_deterministic_with_rendering() {
    // a paused+stepped frame must produce the same pixels as a free run
    let mut a = Engine::new(SRC, 4, 1).unwrap();
    let free: Vec<[u8; 3]> = a.frame(Fx::from_int(10)).to_vec();

    let mut b = debug_engine();
    b.debug_set_breakpoints(&[10]);
    b.frame(Fx::from_int(10));
    while b.debug_step(StepKind::Over) {}
    assert_eq!(b.pixels(), &free[..]);
}

#[test]
fn globals_pane_and_shadowing() {
    // `v` without var is an implicit global (the KITT case); local `x`
    // shadows the global `x`
    let src = "\
x = 100
export function render(index) {
  var x = 7
  v = index + x
  hsv(0, 0, 0)
}
";
    let mut e = Engine::new(src, 4, 1).unwrap();
    e.debug_set_enabled(true);
    assert_eq!(e.debug_set_breakpoints(&[5]), vec![5]); // hsv line, after v =
    e.frame(Fx::ZERO);
    assert!(e.debug_paused());
    // frame locals show the shadowing local x = 7
    let stack = e.debug_stack();
    let lx = stack[0].locals.iter().find(|(n, _)| n == "x").unwrap();
    assert_eq!(lx.1, Value::Num(Fx::from_int(7)));
    // globals pane: v = index(0) + local x(7); global x untouched at 100 —
    // shadowing wrote the local, not the global
    let globals = e.debug_globals();
    let find = |n: &str| globals.iter().find(|(g, _)| g == n).map(|(_, v)| *v);
    assert_eq!(find("v"), Some(Value::Num(Fx::from_int(7))));
    assert_eq!(find("x"), Some(Value::Num(Fx::from_int(100))));
    // predefined constants stay out of the pane
    assert_eq!(find("PI"), None);
    assert_eq!(find("pixelCount"), None);
    assert_eq!(find("INPUT"), None);
}

#[test]
fn breakpoint_on_blank_line_snaps_forward() {
    // line 1 = comment, line 2 = blank, line 3 = code
    let src = "// comment\n\nexport function render(index) {\n  x = 1\n  hsv(0, 0, 0)\n}\n";
    let mut e = Engine::new(src, 4, 1).expect("compile");
    e.debug_set_enabled(true);
    // line 2 (blank) snaps to line 4 (first body statement)
    let resolved = e.debug_set_breakpoints(&[2]);
    assert_eq!(resolved, vec![4]);
    // a line past all code is dropped
    let resolved = e.debug_set_breakpoints(&[99]);
    assert!(resolved.is_empty());
    // exact lines still resolve to themselves
    let resolved = e.debug_set_breakpoints(&[5]);
    assert_eq!(resolved, vec![5]);
}

#[test]
fn undeclared_assignment_in_function_is_global() {
    // Jeremy's `heat` question: assignment without `var` inside a function
    // creates/uses a global (JS + PB semantics) — the debugger showing it
    // in the globals pane is correct.
    let src = "export function render(index) {\n  heat = index\n  hsv(0, 0, 0)\n}\n";
    let mut e = Engine::new(src, 4, 1).expect("compile");
    e.frame(Fx::ZERO);
    let globals = e.debug_globals();
    assert!(
        globals.iter().any(|g| g.0 == "heat"),
        "heat should be a global: {:?}",
        globals.iter().map(|g| &g.0).collect::<Vec<_>>()
    );
}
