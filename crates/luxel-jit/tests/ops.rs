//! One snippet per row of docs/jit-design.md §3.5, native against
//! interpreted.
//!
//! `library_diff.rs` is the gate; this is the microscope. Each case is a
//! whole pattern whose `render` leaves its answer in a global, so a
//! failure names the operator rather than the pattern.

mod common;

use common::bridge::{fresh_vm, Bridge};
use common::{env_with_fake_addresses, program_of};

use luxel_core::fixed::Fx;
use luxel_core::vm::Value;

const STEP_LIMIT: u64 = 2_000_000;

/// Run `render(i)` for a few `i`, natively and interpreted, and compare
/// every global afterwards.
fn check_with(prelude: &str, body: &str) {
    let src = format!("var out = 0\nvar out2 = 0\n{prelude}\nexport function render(i, x) {{ {body} }}\n");
    let (prog, kinds) = program_of(&src).unwrap_or_else(|e| panic!("{src}\n{e}"));
    let env = env_with_fake_addresses();
    let img = luxel_jit::compile(&prog, &kinds, &env)
        .unwrap_or_else(|e| panic!("{src}\nrefused: {}", e.reason()));

    let mut vi = fresh_vm(&prog, 60, 7);
    let ei = vi.call(&prog, 0, &[]).err();
    let mut b = Bridge::new(&prog, &img, fresh_vm(&prog, 60, 7));
    b.enter(0, &[]);
    b.run(STEP_LIMIT).unwrap_or_else(|e| panic!("{src}\ninit trapped: {e:?}"));
    assert_eq!(
        ei.is_some(),
        b.failed(),
        "{src}\ninit error disagrees ({:?})",
        ei.as_ref().map(|e| &e.message)
    );
    cmp_globals(&src, "init", &vi, &b);

    let f = prog.exported_fn("render").unwrap();
    for i in [0i32, 1, 3, 17, 59] {
        let iv = Fx::from_int(i).raw();
        let xv = (Fx::from_int(i) / Fx::from_int(60)).raw();
        let ei = vi
            .call(
                &prog,
                f,
                &[Value::Num(Fx::from_raw(iv)), Value::Num(Fx::from_raw(xv))],
            )
            .err();
        b.enter(f as usize, &[iv, xv]);
        b.run(STEP_LIMIT)
            .unwrap_or_else(|e| panic!("{src}\ni={i} trapped: {e:?}"));
        assert_eq!(
            ei.is_some(),
            b.failed(),
            "{src}\ni={i}: error disagrees (interpreter {:?})",
            ei.as_ref().map(|e| &e.message)
        );
        cmp_globals(&src, &format!("i={i}"), &vi, &b);
    }
}

fn cmp_globals(src: &str, at: &str, vi: &luxel_core::vm::Vm, b: &Bridge) {
    for (g, (x, y)) in vi.globals.iter().zip(b.vm.globals.iter()).enumerate() {
        assert_eq!(
            x.raw(),
            y.raw(),
            "{src}\n{at}: global {g} ({}) is {x:?} interpreted, {y:?} native",
            b.prog.globals[g].name
        );
    }
}

fn check(body: &str) {
    check_with("", body);
}

#[test]
fn arithmetic() {
    for e in [
        "i + x", "i - x", "i * x", "x * x", "i / 3", "x / 0", "i % 7", "x % 0.3",
        "i ** 2", "2 ** x", "-i", "-x", "i * -1", "(i + 1) * (x - 0.5)",
        "i * 32767", "32767 * 32767", "0.5 * 0.5", "-0.5 * 0.5", "-3.5 % 3",
    ] {
        check(&format!("out = {e}"));
    }
}

#[test]
fn bitwise_and_shifts() {
    for e in [
        "i & 3", "i | 8", "i ^ 5", "~i", "~x", "~0", "~-1",
        "i << 1", "i << 0", "i >> 1", "i >> 0", "x << 2", "x >> 2",
        "i << -0.5", "i >> -0.5", "i << 33", "i >> 33", "-i >> 1", "-i << 1",
        "i << x", "i >> x", "-4 >> 1",
    ] {
        check(&format!("out = {e}"));
    }
}

#[test]
fn comparisons() {
    for e in [
        "i < 3", "i <= 3", "i > 3", "i >= 3", "i == 3", "i != 3",
        "x < 0.5", "x >= 0.5", "-1 < i", "i == i", "!i", "!x", "!0", "!1",
    ] {
        check(&format!("out = {e}"));
    }
}

#[test]
fn control_flow() {
    check("if (i > 3) { out = 1 } else { out = 2 }");
    check("out = 0; for (var j = 0; j < i; j++) { out = out + j }");
    check("out = 0; var j = 0; while (j < i) { out = out + j * 2; j++ }");
    check("out = i > 3 ? 10 : 20");
    check("out = i > 3 && x < 0.5");
    check("out = i > 3 || x < 0.5");
    check("out = 0; for (var j = 0; j < 4; j++) { if (j == 2) { continue } out = out + j }");
    check("out = 0; for (var j = 0; j < 9; j++) { if (j == 3) { break } out = out + 1 }");
}

#[test]
fn locals_and_globals() {
    check("var a = i; var b = a * 2; out = b; out2 = a");
    check("out = out + i");
    check("var a = 1; var b = 2; var c = 3; var d = 4; var e = 5; var f = 6; out = a+b+c+d+e+f+i");
    check_with(
        "var g1 = 5\nvar g2 = 7\n",
        "g1 = g1 + i; g2 = g2 * 2; out = g1 + g2",
    );
}

#[test]
fn arrays() {
    check_with("var a = array(8)\n", "a[i % 8] = i; out = a[i % 8]");
    check_with("var a = array(8)\n", "out = a.length");
    check_with("var a = [1, 2, 3, 4]\n", "out = a[i % 4]");
    check_with("var a = [1, 2, 3, 4]\n", "a[i % 4] = i; out = a[i % 4]");
    check_with("var a = array(4)\n", "out = a[i]"); // out of bounds for i >= 4
    check("var a = [i, i + 1]; out = a[1]");
    check_with("var a = array(4)\nvar b = array(4)\n", "a[0] = i; b[0] = a[0] * 2; out = b[0]");
}

#[test]
fn calls() {
    check_with(
        "function twice(v) { return v * 2 }\n",
        "out = twice(i) + twice(x)",
    );
    check_with(
        "function add6(a, b, c, d, e, f) { return a+b+c+d+e+f }\n",
        "out = add6(i, 1, 2, 3, 4, 5)",
    );
    check_with("function noret(v) { v * 2 }\n", "out = noret(i)");
    check_with(
        "function fib(n) { if (n < 2) { return n } return fib(n - 1) + fib(n - 2) }\n",
        "out = fib(i % 8)",
    );
    check_with("function one() { return 1 }\n", "out = one()");
    // a function value, reached through CallValue
    check_with(
        "function twice(v) { return v * 2 }\nvar f = twice\n",
        "out = f(i)",
    );
}

#[test]
fn builtins() {
    for e in [
        "abs(-i)", "floor(x)", "ceil(x)", "round(x)", "sqrt(i)", "sin(x)",
        "cos(x)", "min(i, 3)", "max(i, 3)", "clamp(i, 1, 5)", "mix(0, i, x)",
        "wave(x)", "triangle(x)", "square(x)", "square(x, 0.25)", "mod(-i, 7)",
        "hypot(i, 3)", "atan2(x, 1)", "log(i + 1)", "perlin(x, 0, 0, 0)",
    ] {
        check(&format!("out = {e}"));
    }
    check("hsv(x, 1, 1); out = 1");
    check("rgb(x, 0.5, 0.25); out = 1");
    check("out = time(0.1) > -1");
}

#[test]
fn boxed_slots() {
    // A ternary whose arms differ in kind makes the compiler insert `Box`,
    // so the slot is `Dyn` and every use of it is the unboxing path.
    check_with(
        "var pal = array(4)\n",
        "var v = i > 3 ? pal : 0; out = v == 0; out2 = !v",
    );
    check_with("var pal = array(4)\n", "var v = i > 3 ? pal : 0; out = v + 1");
    check_with(
        "var pal = array(4)\n",
        "var v = i > 3 ? pal : 0; if (v) { out = 1 } else { out = 2 }",
    );
}

/// `assert()` is top-level only (it runs once, as part of init), so these
/// exercise the `Assert` row in fn 0 rather than in `render`.
#[test]
fn assertions() {
    check_with("assert(pixelCount > 4, \"needs at least 5 pixels\")\n", "out = 1");
    check_with("assert(pixelCount > 4000, \"needs a big strip\")\n", "out = 1");
    check_with("assert(pixelCount % 2 == 0)\n", "out = 1");
}
