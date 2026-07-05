//! End-to-end language semantics: source → compile → VM → observed values.
//! These are the conformance tests for everything a pattern can observe.

use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::vm::Value;

fn engine(src: &str) -> Engine {
    let e = Engine::new(src, 10, 1).expect("compile error");
    assert!(
        e.last_error.is_none(),
        "runtime error during init: {:?}",
        e.last_error
    );
    e
}

fn eval(expr: &str) -> Fx {
    let src = format!("export var out\nout = {expr}\n");
    match engine(&src).var("out") {
        Some(Value::Num(v)) => v,
        other => panic!("`{expr}` produced {other:?}"),
    }
}

fn eval_prog(src: &str) -> Fx {
    match engine(src).var("out") {
        Some(Value::Num(v)) => v,
        other => panic!("program produced {other:?}"),
    }
}

fn fx(v: f64) -> Fx {
    Fx::from_f64(v)
}

#[test]
fn arithmetic_semantics() {
    assert_eq!(eval("1 + 2 * 3"), fx(7.0));
    assert_eq!(eval("-3.5 % 3"), fx(-0.5)); // truncated remainder
    assert_eq!(eval("mod(-3.5, 3)"), fx(2.5)); // floored mod
    assert_eq!(eval("182 * 182"), Fx::from_int(33124)); // wraps negative
    assert_eq!(eval("182 * 182").to_f64(), -32412.0);
    assert_eq!(eval("0.001 * 0.001"), Fx::ZERO); // quantizes to zero
    assert_eq!(eval("1 / 2"), fx(0.5));
    assert_eq!(eval("5 / 0"), Fx::ZERO); // TODO(oracle)
    assert_eq!(eval("floor(-5.1)"), fx(-6.0));
    assert_eq!(eval("ceil(-5.9)"), fx(-5.0));
    assert_eq!(eval("frac(-5.5)"), fx(-0.5));
    assert_eq!(eval("trunc(-5.9)"), fx(-5.0));
}

#[test]
fn bitwise_full_word() {
    assert_eq!(eval("1.5 | 0"), fx(1.5)); // |0 does NOT truncate
    assert_eq!(eval("1.25 << 1"), fx(2.5)); // shifts include fraction
    assert_eq!(eval("2.5 >> 1"), fx(1.25));
    assert_eq!(eval("frac(~1.5)"), Fx::ZERO); // ~ zeros the fraction bits
    assert_eq!(eval("~1.5"), fx(-2.0)); // JS-like ~, minus the fraction
    assert_eq!(eval("5 & 3"), Fx::from_int(1));
    assert_eq!(eval("5 ^ 3"), Fx::from_int(6));
}

#[test]
fn logic_returns_operands() {
    assert_eq!(eval("0 || 42"), Fx::from_int(42));
    assert_eq!(eval("3 && 7"), Fx::from_int(7));
    assert_eq!(eval("0 && 7"), Fx::ZERO);
    assert_eq!(eval("2 || 7"), Fx::from_int(2));
    assert_eq!(eval("!5"), Fx::ZERO);
    assert_eq!(eval("!0"), Fx::ONE);
    assert_eq!(eval("true + true"), Fx::from_int(2));
    assert_eq!(eval("1 < 2"), Fx::ONE);
    assert_eq!(eval("2 <= 1"), Fx::ZERO);
}

#[test]
fn short_circuit_skips_rhs() {
    let e = engine(
        "export var called = 0\n\
         f = () => { called = 1; return 5 }\n\
         export var out = 1 || f()\n\
         export var out2 = 0 && f()",
    );
    assert_eq!(e.var("called"), Some(Value::Num(Fx::ZERO)));
    assert_eq!(e.var("out"), Some(Value::Num(Fx::ONE)));
}

#[test]
fn ternary_and_chains() {
    assert_eq!(eval("1 ? 10 : 20"), Fx::from_int(10));
    assert_eq!(eval("0 ? 10 : 0 ? 20 : 30"), Fx::from_int(30));
}

#[test]
fn functions_and_recursion() {
    assert_eq!(
        eval_prog(
            "function fact(n) {\n\
               if (n <= 1) return 1\n\
               return n * fact(n - 1)\n\
             }\n\
             export var out = fact(5)"
        ),
        Fx::from_int(120)
    );
    // missing args become 0, extra args are dropped
    assert_eq!(
        eval_prog("function f(a, b) { return a + b }\nexport var out = f(5)"),
        Fx::from_int(5)
    );
    assert_eq!(
        eval_prog("function f(a) { return a }\nexport var out = f(1, 99)"),
        Fx::ONE
    );
}

#[test]
fn lambdas_and_dispatch_tables() {
    assert_eq!(
        eval_prog(
            "modes = array(2)\n\
             modes[0] = () => 10\n\
             modes[1] = (i) => i * 2\n\
             export var out = modes[1](21)"
        ),
        Fx::from_int(42)
    );
    assert_eq!(
        eval_prog("double = v => v + v\nexport var out = double(21)"),
        Fx::from_int(42)
    );
}

#[test]
fn scoping_rules() {
    // var in function is local and shadows the global
    assert_eq!(
        eval_prog(
            "x = 1\n\
             function f() { var x = 5\n return x }\n\
             export var out = f() + x"
        ),
        Fx::from_int(6)
    );
    // implicit assignment inside a function creates a global
    assert_eq!(
        eval_prog("function f() { g = 7 }\nf()\nexport var out = g"),
        Fx::from_int(7)
    );
    // lambdas see globals, not enclosing locals (no closures) — params work
    assert_eq!(
        eval_prog(
            "shared = 3\n\
             make = () => shared * 2\n\
             export var out = make()"
        ),
        Fx::from_int(6)
    );
}

#[test]
fn loops() {
    assert_eq!(
        eval_prog("s = 0\nfor (var i = 0; i < 10; i++) s += i\nexport var out = s"),
        Fx::from_int(45)
    );
    assert_eq!(
        eval_prog(
            "s = 0\ni = 0\nwhile (i < 10) { i++\n if (i == 3) continue\n if (i > 5) break\n s += i }\nexport var out = s"
        ),
        Fx::from_int(1 + 2 + 4 + 5)
    );
}

#[test]
fn inc_dec_value_semantics() {
    assert_eq!(
        eval_prog("i = 5\nexport var out = i++ * 10 + i"),
        Fx::from_int(56)
    );
    assert_eq!(
        eval_prog("i = 5\nexport var out = ++i * 10 + i"),
        Fx::from_int(66)
    );
    assert_eq!(
        eval_prog("i = 5\nexport var out = i-- * 10 + i"),
        Fx::from_int(54)
    );
    assert_eq!(
        eval_prog("a = [7]\na[0]++\nexport var out = a[0]"),
        Fx::from_int(8)
    );
}

#[test]
fn arrays() {
    assert_eq!(eval("[1, 2, 3].length"), Fx::from_int(3));
    assert_eq!(eval("[1, 2, 3].sum()"), Fx::from_int(6));
    assert_eq!(eval("arraySum([1, 2, 3])"), Fx::from_int(6));
    assert_eq!(
        eval_prog("a = [1, 2, 3]\na.mutate(v => v * 2)\nexport var out = a[2]"),
        Fx::from_int(6)
    );
    assert_eq!(
        eval_prog("a = array(4)\nexport var out = a.length + a[2]"),
        Fx::from_int(4)
    );
    assert_eq!(
        eval_prog("a = [3, 1, 2]\na.sort()\nexport var out = a[0] * 100 + a[1] * 10 + a[2]"),
        Fx::from_int(123)
    );
    assert_eq!(
        eval_prog("a = [1, 2, 3]\na.sortBy((x, y) => y - x)\nexport var out = a[0]"),
        Fx::from_int(3)
    );
    assert_eq!(
        eval_prog("a = [1, 2]\nexport var out = a.reduce((acc, v) => acc + v, 10)"),
        Fx::from_int(13)
    );
    // out-of-bounds: read 0, write ignored (TODO(oracle))
    assert_eq!(eval_prog("a = [1]\nexport var out = a[5]"), Fx::ZERO);
    assert_eq!(
        eval_prog("a = [1]\na[5] = 9\nexport var out = a[0]"),
        Fx::ONE
    );
    // nested arrays
    assert_eq!(
        eval_prog("m = [[1, 2], [3, 4]]\nexport var out = m[1][0]"),
        Fx::from_int(3)
    );
}

#[test]
fn builtin_constants_and_predefined() {
    assert!((eval("PI").to_f64() - core::f64::consts::PI).abs() < 1e-4);
    assert!((eval("PI2").to_f64() - core::f64::consts::TAU).abs() < 1e-4);
    assert_eq!(eval("pixelCount"), Fx::from_int(10)); // engine set to 10
}

#[test]
fn waveform_builtins() {
    assert_eq!(eval("triangle(0.25)"), fx(0.5));
    assert_eq!(eval("triangle(0.75)"), fx(0.5));
    assert_eq!(eval("square(0.2, 0.5)"), Fx::ONE);
    assert_eq!(eval("square(0.7, 0.5)"), Fx::ZERO);
    assert!((eval("wave(0.25)").to_f64() - 1.0).abs() < 1e-3);
    assert!((eval("wave(0.75)").to_f64()).abs() < 1e-3);
    assert_eq!(eval("mix(10, 20, 0.5)"), Fx::from_int(15));
    assert_eq!(eval("smoothstep(0, 1, 0.5)"), fx(0.5));
    assert_eq!(eval("clamp(5, 0, 1)"), Fx::ONE);
}

#[test]
fn prng_is_seeded_and_deterministic() {
    let a = eval_prog("prngSeed(42)\nexport var out = prng(100)");
    let b = eval_prog("prngSeed(42)\nexport var out = prng(100)");
    assert_eq!(a, b);
    assert!(a >= Fx::ZERO && a < Fx::from_int(100));
}

#[test]
fn runtime_errors_are_recorded_not_fatal() {
    // calling a number
    let e = Engine::new("f = 3\nout = f()", 10, 1).unwrap();
    assert!(e.last_error.is_some());
    // unimplemented documented builtin compiles but errors at runtime
    let e = Engine::new("export var out = perlin(1, 2, 3, 4)", 10, 1).unwrap();
    let err = e.last_error.expect("expected runtime error");
    assert!(err.message.contains("perlin"), "{}", err.message);
    // infinite loop trips the fuel guard instead of hanging
    let e = Engine::new("while (1) { }", 10, 1).unwrap();
    assert!(e.last_error.unwrap().message.contains("execution limit"));
}

#[test]
fn compile_errors() {
    assert!(Engine::new("out = undefinedThing", 10, 1).is_err());
    assert!(Engine::new("export var out = notAFunction(1)", 10, 1).is_err());
    assert!(Engine::new("function f() { function g() {} }", 10, 1).is_err());
    assert!(Engine::new("break", 10, 1).is_err());
}

#[test]
fn implicit_global_can_shadow_a_builtin() {
    // "implicit assignment creates a global" wins over the builtin table;
    // calling the shadowed name is then a runtime error. TODO(oracle).
    let e = Engine::new("sin = 3\nout = sin(1)", 10, 1).unwrap();
    assert!(e.last_error.is_some());
}

#[test]
fn export_vars_read_write() {
    let mut e = engine("export var speed = 0.5\nnotExported = 3");
    assert_eq!(e.var("speed"), Some(Value::Num(fx(0.5))));
    assert_eq!(e.var("notExported"), None);
    assert!(e.set_var("speed", fx(0.9)));
    assert_eq!(e.var("speed"), Some(Value::Num(fx(0.9))));
    // exported arrays visible
    let e = engine("export var frequencyData = array(32)");
    assert_eq!(e.var_array("frequencyData").unwrap().len(), 32);
}

#[test]
fn controls_enumerate_and_invoke() {
    let mut e = engine(
        "var speed = 0.1\n\
         export function sliderSpeed(v) { speed = v }\n\
         export function showNumberSpeed() { return speed }\n\
         export var out\n\
         export function render(i) { out = speed\n hsv(0, 0, 0) }",
    );
    assert_eq!(e.controls().len(), 2);
    e.set_control("sliderSpeed", &[fx(0.75)]);
    let shown = e.set_control("showNumberSpeed", &[]).unwrap();
    assert_eq!(shown, fx(0.75));
}
