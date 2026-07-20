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
    assert_eq!(eval("5 / 0"), Fx::ZERO); // oracle-verified (div0 vector)
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
    assert_eq!(px[0], [127, 0, 127]); // halfway red→blue, floor-quantized (PB-exact)
    let mut e = engine(
        "setPalette([0, 1, 0, 0, 1, 0, 0, 1])\n\
         export function render(index) { paint(0, 0.5) }",
    );
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [127, 0, 0]); // brightness scales (floor-quantized)
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
    // world coords max out at ≈0.99998, and quantization floors (PB-exact)
    assert_eq!(px[1], [254, 0, 0]);
    assert_eq!(px[2], [0, 254, 0]);
    assert_eq!(px[3], [254, 254, 0]);
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
fn deep_nesting_is_a_compile_error_not_a_crash() {
    // On the firmware the compiler shares a ~30 KB task stack; unbounded
    // parse/emit recursion overflowed it in the field. Depth must be
    // rejected with a diagnostic, never a stack overflow.
    let deep_expr = format!("x = {}1{}", "(".repeat(500), ")".repeat(500));
    let Err(err) = Engine::new(&deep_expr, 10, 1) else {
        panic!("deep expression should be rejected")
    };
    assert!(err.message.contains("too deep"), "{}", err.message);

    let deep_stmt = format!(
        "{}x = 1{}",
        "if (1) {\n".repeat(500),
        "\n}".repeat(500)
    );
    let Err(err) = Engine::new(&deep_stmt, 10, 1) else {
        panic!("deep statement nesting should be rejected")
    };
    assert!(err.message.contains("too deep"), "{}", err.message);

    // ...but realistic nesting is untouched (worst gallery pattern < 30).
    let ok = format!("x = {}1{}", "(".repeat(40), ")".repeat(40));
    assert!(Engine::new(&ok, 10, 1).is_ok());
}

#[test]
fn implicit_global_can_shadow_a_builtin() {
    // "implicit assignment creates a global" wins over the builtin table;
    // calling the shadowed name is then a runtime error. Oracle-verified
    // 2026-07-07: PB aborts init identically (shadow_call sentinel).
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

#[test]
fn let_behaves_like_var() {
    // let at top level = global; let in a function = local
    assert_eq!(eval_prog("export var out\nlet x = 3\nout = x * 2"), fx(6.0));
    assert_eq!(
        eval_prog("export var out\nfunction f() { let y = 5\n return y }\nout = f()"),
        fx(5.0)
    );
    // reassigning a let is fine
    assert_eq!(eval_prog("export var out\nlet z = 1\nz = 9\nout = z"), fx(9.0));
}

#[test]
fn const_forbids_reassignment_and_requires_init() {
    // const reads back its value
    assert_eq!(eval_prog("export var out\nconst k = 7\nout = k + 1"), fx(8.0));
    // reassignment is a compile error, at top level...
    assert!(Engine::new("const k = 1\nk = 2", 10, 1).is_err());
    // ...inside a function...
    assert!(
        Engine::new("function f() { const c = 1\n c = 2\n return c }", 10, 1).is_err()
    );
    // ...and via compound assignment
    assert!(Engine::new("const k = 1\nk += 2", 10, 1).is_err());
    // const without an initializer is a compile error
    assert!(Engine::new("const k", 10, 1).is_err());
    // a local const does not lock a same-named global
    assert!(
        Engine::new("const g = 1\nfunction f() { let g = 2\n g = 3\n return g }", 10, 1)
            .is_ok()
    );
}

#[test]
fn const_let_in_for_loop() {
    // let in a for-initializer works like var
    assert_eq!(
        eval_prog("export var out\nsum = 0\nfor (let i = 0; i < 4; i = i + 1) { sum = sum + i }\nout = sum"),
        fx(6.0)
    );
}

#[test]
fn extension_builtins() {
    // map: re-range
    assert_eq!(eval("map(5, 0, 10, 0, 100)"), fx(50.0));
    assert_eq!(eval("map(0, 0, 10, 20, 40)"), fx(20.0));
    // sign
    assert_eq!(eval("sign(0 - 3)"), fx(-1.0));
    assert_eq!(eval("sign(3)"), fx(1.0));
    assert_eq!(eval("sign(0)"), fx(0.0));
    // step / saturate
    assert_eq!(eval("step(0.5, 0.4)"), fx(0.0));
    assert_eq!(eval("step(0.5, 0.6)"), fx(1.0));
    assert_eq!(eval("saturate(1.5)"), fx(1.0));
    assert_eq!(eval("saturate(0 - 0.2)"), fx(0.0));
    // dist
    assert_eq!(eval("dist(0, 0, 3, 4)"), fx(5.0));
    assert_eq!(eval("dist3(0, 0, 0, 2, 3, 6)"), fx(7.0));
    // easing endpoints: all ease*(0)=0, ease*(1)=1
    for f in [
        "easeInQuad", "easeOutQuad", "easeInOutQuad",
        "easeInCubic", "easeOutCubic", "easeInOutCubic",
        "easeOutBack", "easeOutElastic", "easeOutBounce",
    ] {
        assert!(eval(&format!("{f}(0)")).to_f64().abs() < 1e-2, "{f}(0) != 0");
        assert!((eval(&format!("{f}(1)")).to_f64() - 1.0).abs() < 1e-2, "{f}(1) != 1");
    }
    // easeInQuad(0.5) = 0.25
    assert!((eval("easeInQuad(0.5)").to_f64() - 0.25).abs() < 1e-3);
    // easeOutBack overshoots past 1 mid-curve (that's its point)
    assert!(eval("easeOutBack(0.6)").to_f64() > 1.0);
    // easeOutElastic wobbles: sine peak at t=0.15 (arg = 0.25 turns)...
    assert!(eval("easeOutElastic(0.15)").to_f64() > 1.2);
    // ...and the trough at t=0.3 (arg = 0.75 turns) dips back below 1
    assert!(eval("easeOutElastic(0.3)").to_f64() < 0.95);
    // easeOutBounce: first bounce peak region ≈ n1·t² ramp, t=1/2.75 → 1
    assert!((eval("easeOutBounce(0.3636)").to_f64() - 1.0).abs() < 0.02);
    // trough between bounces dips well below 1
    assert!(eval("easeOutBounce(0.55)").to_f64() < 0.85);
}

#[test]
fn transform_semantics_match_pixelblaze() {
    // Pinned against the real PB (fw 3.67, oracle sweep 2026-07-07). Its
    // installed map put pixel 0 at world (≈1.0, 0.5); replicate that here
    // (set_map normalizes per axis) and expect the same transform behavior:
    //   - order:  first-called transform is OUTERMOST
    //             translate(.25) then scale(2)  →  (x + .25) · 2
    //             scale(2) then translate(.25)  →  x · 2 + .25
    //   - rotate(PI/2): (x, y) → (−y, x)
    let src = "export var ta = -99\nexport var tb = -99\n\
               export var rx = -99\nexport var ry = -99\n\
               export function beforeRender(delta) {\n\
                 resetTransform()\n\
                 translate(0.25, 0)\n\
                 scale(2, 2)\n\
                 mapPixels((i, x, y, z) => { if (i == 0) ta = x })\n\
                 resetTransform()\n\
                 scale(2, 2)\n\
                 translate(0.25, 0)\n\
                 mapPixels((i, x, y, z) => { if (i == 0) tb = x })\n\
                 resetTransform()\n\
                 rotate(PI / 2)\n\
                 mapPixels((i, x, y, z) => { if (i == 0) { rx = x\n ry = y } })\n\
               }\n\
               export function render(index) { hsv(0, 0, 0) }";
    let mut e = Engine::new(src, 3, 1).expect("compile");
    // normalized: pixel 0 → (1.0, 0.5)
    let m = |x: f64, y: f64| [Fx::from_f64(x), Fx::from_f64(y), Fx::ZERO];
    e.set_map(2, &[m(1.0, 0.5), m(0.0, 0.0), m(0.5, 1.0)]);
    e.frame(Fx::ZERO);
    let v = |n: &str| e.var(n).unwrap().num().to_f64();
    // PB measured: ta=2.4999695, tb=2.2499695, rx=−0.49998474, ry=0.99998474
    assert!((v("ta") - 2.5).abs() < 0.01, "ta = {}", v("ta"));
    assert!((v("tb") - 2.25).abs() < 0.01, "tb = {}", v("tb"));
    assert!((v("rx") + 0.5).abs() < 0.01, "rx = {}", v("rx"));
    assert!((v("ry") - 1.0).abs() < 0.01, "ry = {}", v("ry"));
}

#[test]
fn transforms_accumulate_across_frames() {
    // PB measured (oracle sweep): a translate(0.1) per beforeRender with no
    // resetTransform stacks up — x advances 0.1 per frame (1.1, 1.2, 1.3
    // from base 1.0 on the device). No implicit per-frame reset.
    let src = "export var x0 = -99\nexport var pass = 0\n\
               export function beforeRender(delta) {\n\
                 pass = pass + 1\n\
                 translate(0.1, 0)\n\
                 mapPixels((i, x, y, z) => { if (i == 0) x0 = x })\n\
               }\n\
               export function render(index) { hsv(0, 0, 0) }";
    let mut e = Engine::new(src, 2, 1).expect("compile");
    let v = |e: &Engine| e.var("x0").unwrap().num().to_f64();
    e.frame(Fx::ZERO);
    let a = v(&e);
    e.frame(Fx::ZERO);
    let b = v(&e);
    e.frame(Fx::ZERO);
    let c = v(&e);
    assert!((b - a - 0.1).abs() < 0.01, "frame 2: {a} → {b}");
    assert!((c - b - 0.1).abs() < 0.01, "frame 3: {b} → {c}");
}

#[test]
fn sensor_injection() {
    use luxel_core::engine::SensorFrame;
    let src = "export var frequencyData\nexport var energyAverage\n\
               export function render(index) { rgb(frequencyData[0], energyAverage, 0) }";
    let mut e = Engine::new(src, 1, 1).expect("compile");
    assert!(e.wants_sensors());
    // dark until a sensor source feeds it
    assert_eq!(e.frame(Fx::ZERO)[0], [0, 0, 0]);
    let mut s = SensorFrame::default();
    s.frequency_data[0] = Fx::ONE;
    s.energy_average = Fx::from_f64(0.5);
    e.set_sensors(&s);
    let px = e.frame(Fx::ZERO)[0];
    assert!(px[0] > 250, "freq bin drives red: {px:?}");
    assert!((px[1] as i32 - 128).abs() < 4, "energy drives green: {px:?}");
    // a pattern with no sensor bindings reports not wanting them
    let plain = Engine::new("export function render(i) { hsv(0, 0, 0) }", 1, 1).unwrap();
    assert!(!plain.wants_sensors());
}

#[test]
fn oklch_produces_reasonable_colors() {
    // oklch/oklab set the current pixel; read it back through a render
    fn render_rgb(body: &str) -> [f64; 3] {
        let src = format!("export function render(index) {{ {body} }}");
        let mut e = Engine::new(&src, 1, 1).expect("compile");
        let px = e.frame(Fx::ZERO);
        [px[0][0] as f64 / 255.0, px[0][1] as f64 / 255.0, px[0][2] as f64 / 255.0]
    }
    // OKLCH red ≈ oklch(0.628, 0.258, hue 29.23°/360 = 0.0812 turns)
    let red = render_rgb("oklch(0.628, 0.258, 0.0812)");
    assert!(red[0] > 0.8 && red[1] < 0.35 && red[2] < 0.35, "red = {red:?}");
    // near-white: high L, ~zero chroma
    let white = render_rgb("oklch(0.99, 0, 0)");
    assert!(white.iter().all(|&c| c > 0.9), "white = {white:?}");
    // black
    let black = render_rgb("oklch(0, 0, 0)");
    assert!(black.iter().all(|&c| c < 0.05), "black = {black:?}");
    // oklab neutral gray (a=b=0) is achromatic (r≈g≈b)
    let gray = render_rgb("oklab(0.6, 0, 0)");
    assert!((gray[0] - gray[1]).abs() < 0.03 && (gray[1] - gray[2]).abs() < 0.03, "gray = {gray:?}");
}

#[test]
fn extension_builtins_batch2() {
    // dot / dot3
    assert_eq!(eval("dot(1, 2, 3, 4)"), fx(11.0));
    assert_eq!(eval("dot3(1, 2, 3, 4, 5, 6)"), fx(32.0));
    // angleBetween: +x to +y is +90° (π/2 rad), signed
    assert!((eval("angleBetween(1, 0, 0, 1)").to_f64() - 1.5708).abs() < 1e-2);
    assert!((eval("angleBetween(0, 1, 1, 0)").to_f64() + 1.5708).abs() < 1e-2);
    // hash: deterministic, in [0,1), different inputs differ
    assert_eq!(eval("hash(0.5)"), eval("hash(0.5)"));
    let h = eval("hash(0.5)").to_f64();
    assert!((0.0..1.0).contains(&h));
    assert_ne!(eval("hash(0.5)"), eval("hash(0.25)"));
    assert_ne!(eval("hash2(1, 2)"), eval("hash2(2, 1)"));
    // beat at t=0 is phase 0 (engine clock starts at 0 in one eval)
    assert_eq!(eval("beat(120)"), fx(0.0));
    // beatSin defaults lo=0, hi=1 → at t=0, sin(0)=0 → midpoint 0.5
    assert!((eval("beatSin(120)").to_f64() - 0.5).abs() < 1e-3);
    assert!((eval("beatSin(120, 2, 4)").to_f64() - 3.0).abs() < 1e-2);
}

#[test]
fn blur_and_feedback() {
    // blur1D radius 1: [0,3,0] → [1.5, 1, 1.5] (edges clamp: 2-wide windows)
    assert_eq!(
        eval_prog("a = [0, 3, 0]\nblur1D(a, 1)\nexport var out = a[1]"),
        fx(1.0)
    );
    assert_eq!(
        eval_prog("a = [0, 3, 0]\nblur1D(a, 1)\nexport var out = a[0]"),
        fx(1.5)
    );
    // radius 0 is a no-op; returns the array (chainable)
    assert_eq!(
        eval_prog("a = [4, 2]\nexport var out = blur1D(a, 0)[0]"),
        fx(4.0)
    );
    // feedback: multiply-decay in place
    assert_eq!(
        eval_prog("a = [2, 4]\nfeedback(a, 0.5)\nexport var out = a[1]"),
        fx(2.0)
    );
}

#[test]
fn blur2d() {
    // 3×3 impulse (center 9), radius 1: separable box blur with clamped
    // windows, matching blur1D per axis. Horizontal [0,9,0] → [4.5,3,4.5];
    // vertical then gives center 1, corner 2.25, edge-mid 1.5.
    let grid = "a = [0,0,0, 0,9,0, 0,0,0]\nblur2D(a, 3, 3, 1)\n";
    assert_eq!(eval_prog(&format!("{grid}export var out = a[4]")), fx(1.0));
    assert_eq!(eval_prog(&format!("{grid}export var out = a[0]")), fx(2.25));
    assert_eq!(eval_prog(&format!("{grid}export var out = a[1]")), fx(1.5));
    // non-square pins row-major orientation: w=2, h=3, impulse mid-row
    let rect = "a = [0,0, 3,0, 0,0]\nblur2D(a, 2, 3, 1)\n";
    assert_eq!(eval_prog(&format!("{rect}export var out = a[0]")), fx(0.75));
    assert_eq!(eval_prog(&format!("{rect}export var out = a[2]")), fx(0.5));
    assert_eq!(eval_prog(&format!("{rect}export var out = a[5]")), fx(0.75));
    // radius 0 is a no-op; returns the array (chainable)
    assert_eq!(
        eval_prog("a = [5, 1, 5, 1]\nexport var out = blur2D(a, 2, 2, 0)[0]"),
        fx(5.0)
    );
    // an array shorter than w×h is a runtime error, not an OOB abort
    let e = luxel_core::engine::Engine::new("a = array(3)\nblur2D(a, 2, 2, 1)", 10, 1)
        .expect("compiles");
    assert!(
        e.last_error.expect("expected error").message.contains("shorter"),
        "blur2D undersized array should error"
    );
}

#[test]
fn bulk_array_math() {
    // arrayAdd / arraySub: element-wise in place, returns dst
    assert_eq!(
        eval_prog("a = [1, 2]\nb = [10, 20]\nexport var out = arrayAdd(a, b)[1]"),
        fx(22.0)
    );
    assert_eq!(
        eval_prog("a = [10, 20]\nb = [1, 2]\narraySub(a, b)\nexport var out = a[0]"),
        fx(9.0)
    );
    // length mismatch: ops run over the shorter length, extras untouched
    assert_eq!(
        eval_prog("a = [1, 2, 3]\nb = [10, 10]\narrayAdd(a, b)\nexport var out = a[2]"),
        fx(3.0)
    );
    // src is never written
    assert_eq!(
        eval_prog("a = [1, 2]\nb = [10, 20]\narrayAdd(a, b)\nexport var out = b[0]"),
        fx(10.0)
    );
    // arrayScale = feedback under its general name
    assert_eq!(
        eval_prog("a = [2, 4]\narrayScale(a, 0.5)\nexport var out = a[1]"),
        fx(2.0)
    );
    // arrayMix: dst + (src − dst)·t, unclamped lerp; t = 1 is an exact copy
    assert_eq!(
        eval_prog("a = [0, 4]\nb = [8, 0]\narrayMix(a, b, 0.25)\nexport var out = a[0]"),
        fx(2.0)
    );
    assert_eq!(
        eval_prog("a = [0, 4]\nb = [8, 0]\narrayMix(a, b, 0.25)\nexport var out = a[1]"),
        fx(3.0)
    );
    assert_eq!(
        eval_prog("a = [5, 5]\nb = [1, 9]\narrayMix(a, b, 1)\nexport var out = a[1]"),
        fx(9.0)
    );
    // aliased calls have their closed forms (no double-borrow, no garbage)
    assert_eq!(
        eval_prog("a = [3, 4]\narrayAdd(a, a)\nexport var out = a[1]"),
        fx(8.0)
    );
    assert_eq!(
        eval_prog("a = [3, 4]\narraySub(a, a)\nexport var out = a[0]"),
        fx(0.0)
    );
    assert_eq!(
        eval_prog("a = [3, 4]\narrayMix(a, a, 0.5)\nexport var out = a[0]"),
        fx(3.0)
    );
}

#[test]
fn canvas_helpers() {
    // canvasSet: floor(x·w) cells with edge clamping — x = 1 lands in the
    // last column (no `* 15.99` fudge), negatives clamp to 0. Returns v.
    assert_eq!(
        eval_prog("c = array(16)\ncanvasSet(c, 4, 0.99, 0, 7)\nexport var out = c[3]"),
        fx(7.0)
    );
    assert_eq!(
        eval_prog("c = array(16)\ncanvasSet(c, 4, 1, 1, 7)\nexport var out = c[15]"),
        fx(7.0)
    );
    assert_eq!(
        eval_prog("c = array(16)\ncanvasSet(c, 4, -2, 0.5, 7)\nexport var out = c[8]"),
        fx(7.0)
    );
    assert_eq!(
        eval_prog("c = array(16)\nexport var out = canvasSet(c, 4, 0, 0, 3)"),
        fx(3.0)
    );
    // canvasGet at a texel center returns exactly what canvasSet stored
    // (centers at (i + 0.5)/w — set and get agree on the grid)
    assert_eq!(
        eval_prog(
            "c = array(16)\ncanvasSet(c, 4, 0.125, 0.375, 5)\nexport var out = canvasGet(c, 4, 0.125, 0.375)"
        ),
        fx(5.0)
    );
    // bilinear: halfway between two texel centers blends them evenly
    assert_eq!(
        eval_prog("c = array(16)\nc[0] = 0\nc[1] = 1\nexport var out = canvasGet(c, 4, 0.25, 0.125)"),
        fx(0.5)
    );
    // edge clamp: coordinates at/past the border read the border texel
    assert_eq!(
        eval_prog("c = array(16)\nc[0] = 0.75\nexport var out = canvasGet(c, 4, 0, 0)"),
        fx(0.75)
    );
    assert_eq!(
        eval_prog("c = array(16)\nc[15] = 0.5\nexport var out = canvasGet(c, 4, 2, 2)"),
        fx(0.5)
    );
    // 2D blend: center of a 2×2 checkerboard corner block averages all 4
    assert_eq!(
        eval_prog(
            "c = array(16)\nc[0] = 1\nc[1] = 0\nc[4] = 0\nc[5] = 1\nexport var out = canvasGet(c, 4, 0.25, 0.25)"
        ),
        fx(0.5)
    );
}

#[test]
fn value_returning_color() {
    // hsv2rgb writes into out and returns it: hue 0 = red
    assert_eq!(
        eval_prog("out3 = array(3)\nexport var out = hsv2rgb(0, 1, 1, out3)[0]"),
        fx(1.0)
    );
    assert_eq!(
        eval_prog("out3 = array(3)\nhsv2rgb(0, 1, 1, out3)\nexport var out = out3[1]"),
        fx(0.0)
    );
    // rgb2hsv: pure green → h = 1/3 turn, s = 1, v = 1
    let h = eval_prog("o = array(3)\nrgb2hsv(0, 1, 0, o)\nexport var out = o[0]").to_f64();
    assert!((h - 1.0 / 3.0).abs() < 1e-3, "h = {h}");
    assert_eq!(
        eval_prog("o = array(3)\nrgb2hsv(0, 1, 0, o)\nexport var out = o[1]"),
        fx(1.0)
    );
    // hsv round-trip through rgb (orange-ish)
    let h2 = eval_prog(
        "rgb = array(3)\nhsv = array(3)\nhsv2rgb(0.1, 0.8, 0.9, rgb)\nrgb2hsv(rgb[0], rgb[1], rgb[2], hsv)\nexport var out = hsv[0]",
    )
    .to_f64();
    assert!((h2 - 0.1).abs() < 0.01, "round-trip hue = {h2}");
    // mixColors endpoints reproduce the inputs (red → blue in OKLab)
    let r0 = eval_prog(
        "o = array(3)\nmixColors(1, 0, 0, 0, 0, 1, 0, o)\nexport var out = o[0]",
    )
    .to_f64();
    assert!(r0 > 0.9, "t=0 red channel = {r0}");
    let b1 = eval_prog(
        "o = array(3)\nmixColors(1, 0, 0, 0, 0, 1, 1, o)\nexport var out = o[2]",
    )
    .to_f64();
    assert!(b1 > 0.9, "t=1 blue channel = {b1}");
    // midpoint stays in gamut-ish range on every channel
    for ch in 0..3 {
        let v = eval_prog(&format!(
            "o = array(3)\nmixColors(1, 0, 0, 0, 0, 1, 0.5, o)\nexport var out = o[{ch}]"
        ))
        .to_f64();
        assert!((-0.05..=1.05).contains(&v), "mid channel {ch} = {v}");
    }
}

#[test]
fn simplex_builtins() {
    // deterministic, bounded, varies with position and seed
    assert_eq!(eval("simplex2(1.3, 2.7)"), eval("simplex2(1.3, 2.7)"));
    assert_ne!(eval("simplex2(1.3, 2.7)"), eval("simplex2(1.4, 2.7)"));
    assert_ne!(eval("simplex2(1.3, 2.7, 5)"), eval("simplex2(1.3, 2.7, 6)"));
    let v = eval("simplex3(0.3, 0.7, 1.9)").to_f64();
    assert!((-1.6..=1.6).contains(&v), "{v}");
}

#[test]
fn set_gamma_output_curve() {
    fn render_val(body: &str) -> u8 {
        let src = format!("export function render(index) {{ {body} }}");
        let mut e = Engine::new(&src, 1, 1).expect("compile");
        e.frame(Fx::ZERO)[0][0]
    }
    // gamma 2: mid-gray 0.5 → 0.25 (128 → ~64); off leaves it linear
    let linear = render_val("rgb(0.5, 0, 0)");
    let curved = render_val("setGamma(2); rgb(0.5, 0, 0)");
    assert!((linear as i32 - 128).abs() <= 1, "linear = {linear}");
    assert!((curved as i32 - 64).abs() <= 2, "gamma 2 = {curved}");
    // endpoints survive exactly
    assert_eq!(render_val("setGamma(2.2); rgb(1, 1, 1)"), 255);
    assert_eq!(render_val("setGamma(2.2); rgb(0, 0, 0)"), 0);
    // gamma 1 = off
    assert_eq!(render_val("setGamma(1); rgb(0.5, 0, 0)"), linear);
}

#[test]
fn exponent_operator() {
    assert_eq!(eval("2 ** 10"), fx(1024.0));
    // right-associative like JS: 2 ** 3 ** 2 = 2 ** 9
    assert_eq!(eval("2 ** 3 ** 2"), fx(512.0));
    // binds tighter than *
    assert_eq!(eval("2 * 3 ** 2"), fx(18.0));
    // fractional exponents work (sqrt); tolerance for fixed-point pow
    assert!((eval("9 ** 0.5").to_f64() - 3.0).abs() < 0.01);
    // unary lhs binds first (documented divergence from JS's SyntaxError)
    assert_eq!(eval("0 - 2 ** 2"), fx(-4.0)); // (-) after: 0 - (2**2)
}

#[test]
fn familiar_builtin_aliases() {
    // fract = frac (fractional part)
    assert_eq!(eval("fract(2.75)"), fx(0.75));
    assert_eq!(eval("fract(2.75)"), eval("frac(2.75)"));
    // lerp = mix (linear blend)
    assert_eq!(eval("lerp(0, 10, 0.25)"), fx(2.5));
    assert_eq!(eval("lerp(4, 8, 0.5)"), eval("mix(4, 8, 0.5)"));
    // length / length3 = hypot / hypot3 (vector magnitude)
    assert_eq!(eval("length(3, 4)"), fx(5.0));
    assert_eq!(eval("length3(2, 3, 6)"), fx(7.0));
    assert_eq!(eval("length(3, 4)"), eval("hypot(3, 4)"));
}
