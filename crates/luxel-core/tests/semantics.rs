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
    // fractional in-bounds reads truncate (oracle: a[1.5] → a[1])
    assert_eq!(
        eval_prog("a = [10, 20, 30]\nexport var out = a[1.5]"),
        Fx::from_int(20)
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
fn array_bounds_are_runtime_errors() {
    // oracle-confirmed: out-of-range access aborts execution — OOB reads
    // and writes and negative indices
    for src in [
        "a = [1]\nout = a[5]",
        "a = [1]\na[5] = 9",
        "a = [1, 2]\nout = a[-1]",
        "a = [1, 2]\ni = -0.5\nout = a[i]",
        "a = [1, 2]\ni = -1\na[i] = 9",
    ] {
        let e = Engine::new(src, 10, 1).unwrap();
        let err = e
            .last_error
            .unwrap_or_else(|| panic!("expected error for {src:?}"));
        assert!(
            err.message.contains("out of bounds"),
            "{src:?}: {}",
            err.message
        );
    }
    // …but in-bounds fractional writes truncate (oracle: variable-index
    // probes + the stock `sparks` pattern depends on it)
    assert_eq!(
        eval_prog("a = [10, 20, 30]\ni = 1.5\na[i] = 9\nexport var out = a[1]"),
        Fx::from_int(9)
    );
    assert_eq!(
        eval_prog("a = [10, 20, 30]\ni = 1.5\na[i] += 9\nexport var out = a[1]"),
        Fx::from_int(29)
    );
}

#[test]
fn corpus_syntax_extensions() {
    // function expressions
    assert_eq!(
        eval_prog("f = function (a) { return a * 3 }\nexport var out = f(7)"),
        Fx::from_int(21)
    );
    // nested named functions hoist within their enclosing function
    assert_eq!(
        eval_prog(
            "function outer(x) {\n\
               var r = inner(x)\n\
               function inner(v) { return v + 1 }\n\
               return r\n\
             }\n\
             export var out = outer(4)"
        ),
        Fx::from_int(5)
    );
    // duplicate top-level functions: last definition wins
    assert_eq!(
        eval_prog("function f() { return 1 }\nfunction f() { return 2 }\nexport var out = f()"),
        Fx::from_int(2)
    );
    // nested declarations flatten to global scope (PB behavior — corpus
    // patterns call helpers declared inside other functions)
    assert_eq!(
        eval_prog(
            "function a() {\n\
               function helper(x) { return x * 2 }\n\
               return 0\n\
             }\n\
             function b() { return helper(21) }\n\
             export var out = b()"
        ),
        Fx::from_int(42)
    );
    // assigning to a function name demotes it to a variable (JS-style)
    let e = engine(
        "function f() { return 1 }\n\
         export var before = f()\n\
         f = 5\n\
         export var out = f",
    );
    assert_eq!(e.var("before"), Some(Value::Num(Fx::ONE)));
    assert_eq!(e.var("out"), Some(Value::Num(Fx::from_int(5))));
    // scientific-notation literals
    assert_eq!(eval("1e2"), Fx::from_int(100));
    assert_eq!(eval("1.5e2 + 5e-1"), Fx::from_f64(150.5));
    // strict equality folds to plain equality; null/undefined are 0
    assert_eq!(eval("(1 === 1) + (1 !== 2) * 10"), Fx::from_int(11));
    assert_eq!(eval("null == 0"), Fx::ONE);
    assert_eq!(eval("undefined"), Fx::ZERO);
    // GPIO constants exist (oracle-probed values)
    assert_eq!(eval("OUTPUT"), Fx::from_int(2));
    assert_eq!(eval("INPUT_PULLDOWN"), Fx::from_int(9));
    assert_eq!(eval("ANALOG"), Fx::from_int(192));
    assert_eq!(eval("HIGH + LOW"), Fx::ONE);
}

#[test]
fn transforms_apply_in_call_order() {
    // translate applied to the point first, then rotate — the corpus
    // rotate-about-center idiom. No map: pixel 1 of 4 has x = 0.25.
    let mut e = Engine::new(
        "export var tx = -1\nexport var ty = -1\n\
         export function beforeRender(delta) {\n\
           resetTransform()\n\
           translate(-0.5, -0.5)\n\
           rotate(PI / 2)\n\
           mapPixels((i, x, y, z) => { if (i == 1) { tx = x\n ty = y } })\n\
         }\n\
         export function render(index) { hsv(0, 0, 0) }",
        4,
        1,
    )
    .unwrap();
    e.frame(Fx::ZERO);
    // p = (0.25, 0) → translate → (-0.25, -0.5) → rotate 90° CCW → (0.5, -0.25)
    let Some(Value::Num(tx)) = e.var("tx") else {
        panic!()
    };
    let Some(Value::Num(ty)) = e.var("ty") else {
        panic!()
    };
    assert!((tx.to_f64() - 0.5).abs() < 0.01, "tx = {tx}");
    assert!((ty.to_f64() + 0.25).abs() < 0.01, "ty = {ty}");
}

#[test]
fn transform_stack_caps_at_31() {
    let e = Engine::new("for (i = 0; i < 40; i++) translate(0.01, 0)", 4, 1).unwrap();
    assert!(e.last_error.unwrap().message.contains("stacked transforms"));
}

#[test]
fn palettes_interpolate() {
    let mut e = engine(
        "setPalette([0, 1, 0, 0, 1, 0, 0, 1])\n\
         export function render(index) { paint(0.5) }",
    );
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [128, 0, 128]); // halfway red→blue
    let mut e = engine(
        "setPalette([0, 1, 0, 0, 1, 0, 0, 1])\n\
         export function render(index) { paint(0, 0.5) }",
    );
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [128, 0, 0]); // brightness scales
}

#[test]
fn clock_builtins() {
    let src = "export var out\n\
               export function beforeRender(delta) {\n\
                 out = clockYear() * 100 + clockMonth()\n\
                 wd = clockWeekday()\n\
                 hms = clockHour() * 10000 + clockMinute() * 100 + clockSecond()\n\
               }\n\
               export var wd\nexport var hms\n\
               export function render(index) { }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    // 86400·31 + 3661 = 1970-02-01 01:01:01, a Sunday
    e.set_wall_clock(86_400 * 31 + 3661);
    e.frame(Fx::ZERO);
    assert_eq!(e.var("out"), Some(Value::Num(Fx::from_int(197002))));
    assert_eq!(e.var("wd"), Some(Value::Num(Fx::ONE))); // Sunday = 1
    assert_eq!(e.var("hms"), Some(Value::Num(Fx::from_int(10101))));
    // without a clock source everything is 0
    let mut e = Engine::new(src, 1, 1).unwrap();
    e.frame(Fx::ZERO);
    assert_eq!(e.var("out"), Some(Value::Num(Fx::ZERO)));
}

#[test]
fn map_and_introspection() {
    let src = "export var dims\nexport var h2\n\
               export function beforeRender(delta) { dims = pixelMapDimensions()\n h2 = has2DMap() }\n\
               export function render2D(index, x, y) { rgb(x, y, 0) }";
    let mut e = Engine::new(src, 4, 1).unwrap();
    // 2×2 grid map
    let coords = [
        [Fx::ZERO, Fx::ZERO, Fx::ZERO],
        [Fx::ONE, Fx::ZERO, Fx::ZERO],
        [Fx::ZERO, Fx::ONE, Fx::ZERO],
        [Fx::ONE, Fx::ONE, Fx::ZERO],
    ];
    e.set_map(2, &coords);
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [0, 0, 0]);
    assert_eq!(px[1], [255, 0, 0]); // x ≈ 0.99998 → 255
    assert_eq!(px[2], [0, 255, 0]);
    assert_eq!(px[3], [255, 255, 0]);
    assert_eq!(e.var("dims"), Some(Value::Num(Fx::from_int(2))));
    assert_eq!(e.var("h2"), Some(Value::Num(Fx::ONE)));
}

#[test]
fn perlin_family_runs_deterministically() {
    let a = eval("perlin(1.3, 2.7, 0.5, 42)");
    let b = eval("perlin(1.3, 2.7, 0.5, 42)");
    assert_eq!(a, b);
    assert!(a.to_f64().abs() < 1.5);
    let f = eval("perlinFbm(1.3, 2.7, 0, 2, 0.5, 3)");
    assert!(f.to_f64().abs() < 2.0);
    assert!(eval("perlinRidge(1.3, 2.7, 0, 2, 0.5, 1, 3)").to_f64() >= 0.0);
    assert!(eval("perlinTurbulence(1.3, 2.7, 0, 2, 0.5, 3)").to_f64() >= 0.0);
    // setPerlinWrap changes the field
    let w = eval_prog("setPerlinWrap(4, 4, 4)\nexport var out = perlin(5.3, 0.5, 0.5, 0)");
    let u = eval_prog("export var out = perlin(5.3, 0.5, 0.5, 0)");
    assert_ne!(w, u);
}

#[test]
fn gpio_and_sequencer_stubs_are_silent() {
    let e = engine(
        "pinMode(26, OUTPUT)\ndigitalWrite(26, HIGH)\n\
         export var out = digitalRead(26) + analogRead(33) + touchRead(4) + nodeId() + sequencerGetMode()",
    );
    assert_eq!(e.var("out"), Some(Value::Num(Fx::ZERO)));
}

#[test]
fn sensor_vars_are_stubbed() {
    let e = engine(
        "export var frequencyData\nexport var accelerometer\n\
         export var out = frequencyData.length * 100 + accelerometer.length",
    );
    assert_eq!(e.var("out"), Some(Value::Num(Fx::from_int(3203))));
    // the detection idiom writes work
    let e = engine(
        "export var frequencyData\nfrequencyData[0] = -1\nexport var out = frequencyData[0]",
    );
    assert_eq!(e.var("out"), Some(Value::Num(Fx::from_int(-1))));
}

#[test]
fn runtime_errors_are_recorded_not_fatal() {
    // calling a number
    let e = Engine::new("f = 3\nout = f()", 10, 1).unwrap();
    assert!(e.last_error.is_some());
    // infinite loop trips the fuel guard instead of hanging
    let e = Engine::new("while (1) { }", 10, 1).unwrap();
    assert!(e.last_error.unwrap().message.contains("execution limit"));
}

#[test]
fn compile_errors() {
    assert!(Engine::new("out = undefinedThing", 10, 1).is_err());
    assert!(Engine::new("export var out = notAFunction(1)", 10, 1).is_err());
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
