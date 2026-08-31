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

/// `?:` is right-associative, so a chain reads as a run of else-ifs. Each
/// arm is a full assignment expression, and only the taken arm runs.
#[test]
fn ternary_chains_are_right_associative() {
    // else-if chain: exactly one arm is selected, in order
    for (i, want) in [(0, 100), (1, 101), (2, 102), (7, 199)] {
        assert_eq!(
            eval_prog(&format!(
                "n = {i}\nexport var out = n == 0 ? 100 : n == 1 ? 101 : n == 2 ? 102 : 199"
            )),
            Fx::from_int(want),
            "chain arm for n = {i}"
        );
    }
    // the tail groups to the right: `1 ? 2 : (1 ? 3 : 4)`
    assert_eq!(eval("1 ? 2 : 1 ? 3 : 4"), Fx::from_int(2));
    assert_eq!(eval("0 ? 2 : 1 ? 3 : 4"), Fx::from_int(3));
    assert_eq!(eval("0 ? 2 : 0 ? 3 : 4"), Fx::from_int(4));
    assert_eq!(eval("0 ? 1 : 0 ? 2 : 0 ? 3 : 4"), Fx::from_int(4));
    // a ternary nested in the THEN slot is closed by its own `:`
    assert_eq!(eval("1 ? 0 ? 5 : 6 : 7"), Fx::from_int(6));
    assert_eq!(eval("1 ? 1 ? 5 : 6 : 7"), Fx::from_int(5));
    assert_eq!(eval("0 ? 1 ? 5 : 6 : 7"), Fx::from_int(7));
    // a ternary as the CONDITION of another
    assert_eq!(eval("(0 ? 1 : 0) ? 10 : 20"), Fx::from_int(20));
    assert_eq!(eval("(0 ? 1 : 5) ? 10 : 20"), Fx::from_int(10));
    // only the taken branch is evaluated (no side effects from the other)
    assert_eq!(
        eval_prog(
            "n = 0\n\
             function bump() { n = n + 1; return 9 }\n\
             x = 1 ? 1 : bump()\n\
             y = 0 ? bump() : 2\n\
             export var out = n"
        ),
        Fx::ZERO
    );
    // assignment is legal inside a branch and the ternary yields its value
    assert_eq!(
        eval_prog("a = 0\nb = 0\nexport var out = (1 ? a = 5 : b = 6) + a + b"),
        Fx::from_int(10)
    );
    // ternaries compose with the rest of the grammar
    assert_eq!(eval("1 + (1 ? 2 : 3) * 10"), Fx::from_int(21));
    assert_eq!(
        eval_prog("a = [1, 2, 3]\nexport var out = a[0 ? 0 : 2]"),
        Fx::from_int(3)
    );
    assert_eq!(
        eval_prog("f = c => c ? 1 : c == 0 ? 2 : 3\nexport var out = f(0) * 10 + f(1)"),
        Fx::from_int(21)
    );
}

/// `arr[i] op= x` for the whole compound-operator family, plus `++`/`--`
/// on elements. The element is read once and written once, and the index
/// and array expressions are evaluated exactly once.
#[test]
fn compound_assignment_on_array_members() {
    let cases: &[(&str, f64)] = &[
        ("a[1] += 10", 12.0),
        ("a[1] -= 10", -8.0),
        ("a[1] *= 10", 20.0),
        ("a[1] /= 4", 0.5),
        ("a[1] %= 1.5", 0.5),
        ("a[1] <<= 2", 8.0),
        ("a[1] >>= 1", 1.0),
        ("a[1] &= 3", 2.0),
        ("a[1] |= 5", 7.0),
        ("a[1] ^= 3", 1.0),
        ("a[1] **= 3", 8.0),
        ("a[1]++", 3.0),
        ("a[1]--", 1.0),
        ("++a[1]", 3.0),
        ("--a[1]", 1.0),
    ];
    for (op, want) in cases {
        assert_eq!(
            eval_prog(&format!("a = [1, 2, 3]\n{op}\nexport var out = a[1]")),
            fx(*want),
            "after `{op}`"
        );
        // neighbours untouched
        assert_eq!(
            eval_prog(&format!(
                "a = [1, 2, 3]\n{op}\nexport var out = a[0] * 10 + a[2]"
            )),
            Fx::from_int(13),
            "`{op}` disturbed a neighbouring element"
        );
    }

    // the compound assignment is an expression yielding the NEW value
    assert_eq!(
        eval_prog("a = [1]\nexport var out = (a[0] += 5) * 10 + a[0]"),
        Fx::from_int(66)
    );
    // ++/-- keep JS's prefix/postfix result values on elements
    assert_eq!(
        eval_prog("a = [7]\nexport var out = a[0]++ * 10 + a[0]"),
        Fx::from_int(78)
    );
    assert_eq!(
        eval_prog("a = [7]\nexport var out = ++a[0] * 10 + a[0]"),
        Fx::from_int(88)
    );
    assert_eq!(
        eval_prog("a = [7]\nexport var out = a[0]-- * 10 + a[0]"),
        Fx::from_int(76)
    );

    // the array and index sub-expressions run ONCE, not twice
    assert_eq!(
        eval_prog(
            "n = 0\n\
             a = [5, 5]\n\
             function idx() { n = n + 1; return 1 }\n\
             a[idx()] += 3\n\
             export var out = n * 100 + a[1]"
        ),
        Fx::from_int(108)
    );
    // rhs is evaluated after the element is read (JS order), and once
    assert_eq!(
        eval_prog(
            "n = 0\n\
             a = [5]\n\
             function rhs() { n = n + 1; return 2 }\n\
             a[0] *= rhs()\n\
             export var out = n * 100 + a[0]"
        ),
        Fx::from_int(110)
    );

    // nested arrays and array-valued expressions as the target's object
    assert_eq!(
        eval_prog("m = [[1, 2], [3, 4]]\nm[1][0] += 10\nexport var out = m[1][0]"),
        Fx::from_int(13)
    );
    assert_eq!(
        eval_prog(
            "m = [[1, 2]]\nfunction row() { return m[0] }\nrow()[1] *= 4\nexport var out = m[0][1]"
        ),
        Fx::from_int(8)
    );
    // const-pooled literal arrays copy-on-write before the first mutation
    assert_eq!(
        eval_prog("a = [1, 2, 3]\nb = [1, 2, 3]\na[0] += 100\nexport var out = a[0] * 1000 + b[0]"),
        Fx::from_int(101_001)
    );
    // inside a loop, on a global array, from inside a function
    assert_eq!(
        eval_prog(
            "acc = array(4)\n\
             function fill() { for (var i = 0; i < 4; i++) acc[i] += i * 2 }\n\
             fill()\n\
             fill()\n\
             export var out = acc[3]"
        ),
        Fx::from_int(12)
    );
    // `**=` on a plain variable too (the operator family is uniform)
    assert_eq!(eval_prog("x = 3\nx **= 2\nexport var out = x"), fx(9.0));
    // properties are still not assignable
    assert!(Engine::new("a = [1]\na.length += 1", 10, 1).is_err());
    assert!(Engine::new("a = [1]\nf() += 1", 10, 1).is_err());
}

#[test]
fn switch_statement() {
    let sw = |n: i32| {
        eval_prog(&format!(
            "n = {n}\n\
             export var out = 0\n\
             switch (n) {{\n\
               case 0: out = 10; break\n\
               case 1: out = 11; break\n\
               default: out = 99\n\
             }}"
        ))
    };
    assert_eq!(sw(0), Fx::from_int(10));
    assert_eq!(sw(1), Fx::from_int(11));
    assert_eq!(sw(7), Fx::from_int(99));

    // fall-through: no `break` means the next arm's body runs too
    let ft = |n: i32| {
        eval_prog(&format!(
            "n = {n}\n\
             export var out = 0\n\
             switch (n) {{\n\
               case 0: out = out + 1\n\
               case 1: out = out + 10\n\
               case 2: out = out + 100; break\n\
               case 3: out = out + 1000\n\
             }}"
        ))
    };
    assert_eq!(ft(0), Fx::from_int(111));
    assert_eq!(ft(1), Fx::from_int(110));
    assert_eq!(ft(2), Fx::from_int(100));
    assert_eq!(ft(3), Fx::from_int(1000));
    assert_eq!(ft(4), Fx::ZERO, "no match, no default ⇒ nothing runs");

    // empty labels stack onto the following body
    let stacked = |n: i32| {
        eval_prog(&format!(
            "n = {n}\nexport var out = 0\nswitch (n) {{ case 1: case 2: out = 5; break\ndefault: out = 6 }}"
        ))
    };
    assert_eq!(stacked(1), Fx::from_int(5));
    assert_eq!(stacked(2), Fx::from_int(5));
    assert_eq!(stacked(3), Fx::from_int(6));

    // `default` in the MIDDLE: still the no-match target, and still falls
    // through into the arm that follows it in source order
    let mid = |n: i32| {
        eval_prog(&format!(
            "n = {n}\n\
             export var out = 0\n\
             switch (n) {{\n\
               case 0: out = 1; break\n\
               default: out = out + 2\n\
               case 9: out = out + 4\n\
             }}"
        ))
    };
    assert_eq!(mid(0), Fx::from_int(1));
    assert_eq!(mid(9), Fx::from_int(4));
    assert_eq!(mid(5), Fx::from_int(6), "default falls into `case 9`");

    // the discriminant is evaluated exactly once, before any label
    assert_eq!(
        eval_prog(
            "n = 0\n\
             function disc() { n = n + 1; return 1 }\n\
             switch (disc()) { case 0: case 1: case 2: break }\n\
             export var out = n"
        ),
        Fx::ONE
    );
    // labels are arbitrary expressions, tested in order until one matches
    assert_eq!(
        eval_prog(
            "k = 2\n\
             export var out = 0\n\
             switch (k * 3) { case 1 + 1: out = 1; break\ncase 2 * 3: out = 2; break }"
        ),
        Fx::from_int(2)
    );
    // ...and only up to the match: later labels don't run
    assert_eq!(
        eval_prog(
            "n = 0\n\
             function label(v) { n = n + 1; return v }\n\
             switch (1) { case label(1): break\ncase label(2): break }\n\
             export var out = n"
        ),
        Fx::ONE
    );
    // equality is the language's `==` (one numeric domain, so `===` too);
    // fixed-point values compare exactly
    assert_eq!(
        eval_prog("export var out = 0\nswitch (0.5) { case 1/2: out = 1; break }"),
        Fx::ONE
    );

    // nested switch, with the inner `break` binding to the inner switch
    assert_eq!(
        eval_prog(
            "export var out = 0\n\
             switch (1) {\n\
               case 1:\n\
                 switch (2) { case 2: out = out + 1; break\ndefault: out = out + 8 }\n\
                 out = out + 10\n\
                 break\n\
               case 2: out = out + 100\n\
             }"
        ),
        Fx::from_int(11)
    );

    // `break` inside a switch inside a loop leaves the SWITCH, not the loop;
    // `continue` skips past the switch to the enclosing loop
    assert_eq!(
        eval_prog(
            "total = 0\n\
             for (var i = 0; i < 5; i++) {\n\
               switch (i) {\n\
                 case 1: break\n\
                 case 3: continue\n\
                 default: total = total + 1000\n\
               }\n\
               total = total + 1\n\
             }\n\
             export var out = total"
        ),
        // i=0,2,4 → default (+1000) then +1; i=1 → break out of switch, +1;
        // i=3 → continue skips the trailing +1
        Fx::from_int(3004)
    );
    // a `return` out of a switch body unwinds cleanly
    assert_eq!(
        eval_prog(
            "function pick(n) { switch (n) { case 0: return 5\ndefault: return 6 } }\n\
             export var out = pick(0) * 10 + pick(1)"
        ),
        Fx::from_int(56)
    );

    // `var` inside a switch arm hoists to the function scope like everywhere
    assert_eq!(
        eval_prog(
            "function f(n) { switch (n) { case 0: var v = 3 }\nreturn v == 0 ? 7 : v }\n\
             export var out = f(0) * 10 + f(1)"
        ),
        Fx::from_int(37)
    );

    // switch in the render path: per-pixel dispatch over a mode variable
    let mut e = engine(
        "export function render(index) {\n\
           var v = 0\n\
           switch (index % 3) {\n\
             case 0: v = 1; break\n\
             case 1:\n\
             case 2: v = 0.5; break\n\
           }\n\
           hsv(0, 0, v)\n\
         }",
    );
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [255, 255, 255], "index 0 full white");
    assert!(px[1][0] > 100 && px[1][0] < 220, "index 1 dim: {:?}", px[1]);
    assert_eq!(px[1], px[2], "indices 1 and 2 share a body");
    assert_eq!(px[3], px[0], "the dispatch repeats every 3 pixels");

    // `break`/`continue` still need an enclosing construct
    assert!(Engine::new("switch (1) { case 1: continue }", 10, 1).is_err());
    assert!(Engine::new("break", 10, 1).is_err());
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
    // and writes and negative indices. Re-probed for Gitea #107 on fw 3.67
    // (tools/oracle/oob-probes.mjs Q1-Q4): PB does NOT tolerate these. It
    // does not clamp, wrap or silently no-op — every shape below aborts the
    // handler invocation exactly as Luxel does. The "PB tolerates
    // out-of-range writes" premise came from the abort's narrow blast
    // radius (Gitea #84), not from tolerance.
    for src in [
        "a = [1]\nout = a[5]",
        "a = [1]\na[5] = 9",
        "a = [1, 2]\nout = a[-1]",
        "a = [1, 2]\ni = -0.5\nout = a[i]",
        "a = [1, 2]\ni = -1\na[i] = 9",
        // the index truncates FIRST, then the bounds check runs, so an
        // out-of-range fractional index is an error too (oracle Q4b)
        "a = [1, 2]\nout = a[2.5]",
        "a = [1, 2]\na[2.5] = 9",
        "a = [1, 2]\ni = 2.5\nout = a[i]",
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
    // …in EVERY form. A 2026-07 note claimed PB aborted on a *literal*
    // fractional index write (`a[1.5] = 9`) and that Luxel's uniform
    // truncation was a deliberate divergence. Re-probed for #107 on fw 3.67
    // (oob-probes.mjs Q4d/Q4e/Q4f): the literal-index form, the
    // array-literal target and top-level init scope are all tolerated and
    // all truncate. There is no divergence — this is an exact match.
    assert_eq!(
        eval_prog("a = [10, 20, 30]\na[1.5] = 9\nexport var out = a[1]"),
        Fx::from_int(9)
    );
    assert_eq!(
        eval_prog("a = array(3)\na[1.5] = 9\nexport var out = a[1]"),
        Fx::from_int(9)
    );
}

#[test]
fn array_replace_span_is_bounds_checked() {
    // Oracle #107 (fw 3.67, oob-probes.mjs Q8): the splat forms are checked
    // as a whole span. `offset + count > length` is a runtime error — the
    // engine used to silently drop the overflowing elements, which was the
    // opposite of what `a[i] = v` does.
    for src in [
        "b = array(4)\narrayReplaceAt(b, 3, 7, 8, 9)",
        "b = array(4)\narrayReplaceAt(b, 9, 7)",
        "b = array(2)\narrayReplace(b, 1, 2, 3)",
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
    // `offset + count == length` is the accepted boundary (Q8g)
    assert_eq!(
        eval_prog("b = array(4)\narrayReplaceAt(b, 2, 7, 8)\nexport var out = b[3]"),
        Fx::from_int(8)
    );
    // a NEGATIVE offset neither errors nor clamps to slot 0: the splat
    // shifts, and only the values landing at a valid index are stored, so
    // one value at offset −1 writes nothing (Q8c) while two write b[0] (Q8d)
    assert_eq!(
        eval_prog("b = array(4)\narrayReplaceAt(b, -1, 7)\nexport var out = b[0]"),
        Fx::ZERO
    );
    assert_eq!(
        eval_prog("b = array(4)\narrayReplaceAt(b, -1, 7, 8)\nexport var out = b[0]"),
        Fx::from_int(8)
    );
    // the method form is the offset-0 builtin (oracle 2026-08-22), unchanged
    assert_eq!(
        eval_prog("b = array(4)\nb.replace(2, 9)\nexport var out = b[0] * 10 + b[1]"),
        Fx::from_int(29)
    );
    // …and with nothing to splat there is nothing to do. This used to PANIC
    // the VM (`args[2..1]` is an inverted slice range) — on device a panic
    // is a reboot, so a pattern typo could take the fixture down. PB drops
    // the call, missing args being nothing to write.
    for src in [
        "b = array(4)\narrayReplaceAt(b)\nexport var out = b[0]",
        "b = array(4)\narrayReplaceAt(b, 1)\nexport var out = b[0]",
        "b = array(4)\narrayReplace(b)\nexport var out = b[0]",
    ] {
        let e = Engine::new(src, 10, 1).unwrap();
        assert!(e.last_error.is_none(), "{src:?}: {:?}", e.last_error);
        assert_eq!(e.var("out"), Some(Value::Num(Fx::ZERO)), "{src:?}");
    }
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
fn transform_stack_caps_at_31_silently() {
    // Oracle-verified (fw 3.67, 2026-08-22): ops past the 31st are silently
    // IGNORED on PB — no error, no abort (probed with 40 stacked translates;
    // dx stalls after 31 steps). We match: push_op is a no-op past the cap.
    let src = "export var x0 = -99\nexport var done = -99\n\
               export function beforeRender(delta) {\n\
                 resetTransform()\n\
                 for (k = 0; k < 40; k++) translate(0.01, 0)\n\
                 mapPixels((i, x, y, z) => { if (i == 0) x0 = x })\n\
                 done = 1\n\
               }\n\
               export function render(index) { hsv(0, 0, 0) }";
    let mut e = Engine::new(src, 3, 1).unwrap();
    let m = |x: f64, y: f64| [Fx::from_f64(x), Fx::from_f64(y), Fx::ZERO];
    e.set_map(2, &[m(1.0, 0.5), m(0.0, 0.0), m(0.5, 1.0)]);
    e.frame(Fx::ZERO);
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
    let v = |n: &str| e.var(n).unwrap().num().to_f64();
    assert_eq!(v("done"), 1.0);
    // PB measured dx = 0.3094 (31 steps of the 16.15-quantized 0.01), NOT 0.4
    assert!((v("x0") - 1.0 - 0.3094).abs() < 0.005, "x0 = {}", v("x0"));
}

#[test]
fn palette_edges_match_pixelblaze() {
    // Oracle-verified (fw 3.67, 2026-08-22): lookups below the first stop
    // CLAMP to the first color; anything past the last stop renders BLACK
    // (hard edge exactly at the stop). Single-stop palettes: at-or-below →
    // the color, above → black. Fresh-load default (no setPalette) is the
    // grayscale ramp, and palette state does not leak across loads.
    let src = "setPalette([0.25, 0,0,1,  0.75, 0,1,0])\n\
               export function render(index) {\n\
                 if (index == 0) { paint(0.1) }\n\
                 if (index == 1) { paint(0.5) }\n\
                 if (index == 2) { paint(0.75) }\n\
                 if (index == 3) { paint(0.8) }\n\
                 if (index == 4) { paint(0.999) }\n\
               }";
    let mut e = engine(src);
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [0, 0, 255]); // below first stop: clamp to blue
    assert_eq!(px[1], [0, 127, 127]); // midpoint blend (floor-quantized)
    assert_eq!(px[2], [0, 255, 0]); // exactly at last stop: green
    assert_eq!(px[3], [0, 0, 0]); // past last stop: BLACK, not clamp
    assert_eq!(px[4], [0, 0, 0]);

    let src = "setPalette([0.5, 1,0,0])\n\
               export function render(index) {\n\
                 if (index == 0) { paint(0) }\n\
                 if (index == 1) { paint(0.5) }\n\
                 if (index == 2) { paint(0.51) }\n\
               }";
    let mut e = engine(src);
    let px = e.frame(Fx::ZERO);
    assert_eq!(px[0], [255, 0, 0]);
    assert_eq!(px[1], [255, 0, 0]);
    assert_eq!(px[2], [0, 0, 0]);
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
fn clock_builtins_readable_at_init() {
    // PB patterns read time-of-day at TOP LEVEL (the device's RTC is set by
    // pattern-load time), so the wall clock must be visible during init —
    // a post-construction set_wall_clock only reaches beforeRender/render
    // (Gitea #104; pixelclock and friends were byte-identical across wall
    // clocks because init always saw 0).
    let src = "export var ih\nih = clockHour() * 100 + clockMinute()\n\
               export function render(index) { }";
    // 86400·31 + 3661 = 1970-02-01 01:01:01
    let mut e = Engine::new_at(src, 1, 1, Some(86_400 * 31 + 3661)).unwrap();
    e.frame(Fx::ZERO);
    assert_eq!(e.var("ih"), Some(Value::Num(Fx::from_int(101))));
    // no clock at construction keeps the documented no-time-source 0
    let mut e = Engine::new_at(src, 1, 1, None).unwrap();
    e.frame(Fx::ZERO);
    assert_eq!(e.var("ih"), Some(Value::Num(Fx::ZERO)));
}

#[test]
fn random_negative_max_spans_signed_range() {
    // random(max) with a NEGATIVE max — e.g. random(0xffff), whose literal
    // wraps to -1.0 in 16.16 on PB exactly as here — draws over the whole
    // signed range on a real PB (fw 3.67, oracle-probed 2026-08-23:
    // min/max ≈ ±32760 for both random(0xffff) and random(-5)). Corpus
    // patterns seed hand-rolled PRNGs with it; a 0-clamp collapses them
    // (Gitea #105). With randomSeed pinned the draws are deterministic, so
    // assert the shape: full-range spread, both signs, never all-zero.
    let src = "randomSeed(7)\n\
               export var lit\nlit = 0xffff\n\
               export var neg\nexport var pos\nexport var zero\nexport var big\n\
               neg = 0\npos = 0\nzero = 0\nbig = 0\n\
               for (i = 0; i < 64; i++) {\n\
                 r = random(0xffff)\n\
                 if (r < 0) { neg += 1 }\n\
                 if (r > 0) { pos += 1 }\n\
                 if (r == 0) { zero += 1 }\n\
                 if (abs(r) > 1000) { big += 1 }\n\
               }\n\
               export var inrange\ninrange = 1\n\
               for (i = 0; i < 64; i++) {\n\
                 q = random(0.5)\n\
                 if (q < 0 || q >= 0.5) { inrange = 0 }\n\
               }\n\
               export function render(index) { }";
    let mut e = Engine::new(src, 1, 1).unwrap();
    e.frame(Fx::ZERO);
    assert_eq!(e.var("lit"), Some(Value::Num(Fx::from_int(-1)))); // PB-exact wrap
    let n = |name: &str| match e.var(name) {
        Some(Value::Num(v)) => v.raw() >> 16,
        other => panic!("{name}: {other:?}"),
    };
    assert_eq!(n("zero"), 0, "old clamp behavior: every draw was 0");
    assert!(n("neg") > 10 && n("pos") > 10, "both signs expected");
    assert!(n("big") > 32, "draws should span far beyond ±1");
    assert_eq!(n("inrange"), 1, "positive max keeps plain [0, max)");
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

/// Gitea #177 item 1: with no real GPIO, `digitalRead` reports the pin's idle
/// level, and only a pull-up biases it HIGH.
#[test]
fn digital_read_honours_pin_mode_pullup() {
    let one = Some(Value::Num(Fx::ONE));
    let zero = Some(Value::Num(Fx::ZERO));

    // unconfigured pin: unchanged, idles LOW
    let e = engine("export var out = digitalRead(26)");
    assert_eq!(e.var("out"), zero);

    // INPUT_PULLUP idles HIGH — a button-to-ground reads "not pressed"
    let e = engine("pinMode(26, INPUT_PULLUP)\nexport var out = digitalRead(26)");
    assert_eq!(e.var("out"), one);
    // ...and compares equal to HIGH, which is how patterns spell it
    let e = engine("pinMode(26, INPUT_PULLUP)\nexport var out = digitalRead(26) == HIGH");
    assert_eq!(e.var("out"), one);

    // every other mode still idles LOW
    for mode in [
        "INPUT",
        "INPUT_PULLDOWN",
        "OUTPUT",
        "OUTPUT_OPEN_DRAIN",
        "ANALOG",
    ] {
        let e = engine(&format!(
            "pinMode(26, {mode})\nexport var out = digitalRead(26)"
        ));
        assert_eq!(e.var("out"), zero, "mode {mode} should idle LOW");
    }

    // the pull-up is per pin, and the LAST pinMode wins
    let e = engine(
        "pinMode(26, INPUT_PULLUP)\npinMode(27, INPUT)\n\
         export var out = digitalRead(26) * 10 + digitalRead(27)",
    );
    assert_eq!(e.var("out"), Some(Value::Num(Fx::from_int(10))));
    let e =
        engine("pinMode(26, INPUT_PULLUP)\npinMode(26, INPUT)\nexport var out = digitalRead(26)");
    assert_eq!(e.var("out"), zero);

    // out-of-window pins stay stubbed rather than aliasing a tracked one
    let e = engine("pinMode(100, INPUT_PULLUP)\nexport var out = digitalRead(100)");
    assert_eq!(e.var("out"), zero);
    let e = engine("pinMode(-1, INPUT_PULLUP)\nexport var out = digitalRead(-1)");
    assert_eq!(e.var("out"), zero);
}

/// Gitea #177 item 2: a host can DRIVE a digital pin, and the pattern sees it
/// through `digitalRead` — the injection ABI the playground, the port-review
/// harness and snap.mjs use to press a button deterministically.
#[test]
fn set_pin_drives_digital_read() {
    let one = Some(Value::Num(Fx::ONE));
    let zero = Some(Value::Num(Fx::ZERO));

    // A button-to-ground pattern: samples the pin every frame, so the test
    // observes injection through a RUNNING pattern, not just at init.
    let src = "pinMode(26, INPUT_PULLUP)\n\
               export var pressed = 0\n\
               export function beforeRender(delta) { pressed = digitalRead(26) == LOW }\n\
               export function render(index) { hsv(0, 0, pressed) }";
    let mut e = Engine::new(src, 10, 1).expect("compile error");

    // idle: pulled up, so "not pressed" — the #177 item 1 behaviour
    e.frame(Fx::ZERO);
    assert_eq!(e.var("pressed"), zero);

    // driven LOW: the button is held, and stays held across frames (the
    // injection is a level, not a one-frame pulse)
    assert!(e.set_pin(26, Some(false)));
    for _ in 0..3 {
        e.frame(Fx::from_int(16));
        assert_eq!(e.var("pressed"), one);
    }
    assert_eq!(e.frame(Fx::ZERO)[0], [255, 255, 255], "lit while held");

    // driven HIGH beats the pin's own bias too (a pull-DOWN pin held high)
    assert!(e.set_pin(26, Some(true)));
    e.frame(Fx::from_int(16));
    assert_eq!(e.var("pressed"), zero);

    // released: back to the pinMode idle level, not to LOW
    assert!(e.set_pin(26, None));
    assert!(e.pin_read(26), "released pull-up pin idles HIGH again");
    e.frame(Fx::from_int(16));
    assert_eq!(e.var("pressed"), zero);

    // injection is per pin: 27 is untouched and still idles LOW (no pinMode)
    assert!(e.set_pin(26, Some(false)));
    assert!(!e.pin_read(27));

    // out-of-window pins are REJECTED rather than silently aliasing pin 0 —
    // a typo'd pin number must not look like a stuck input
    assert!(!e.set_pin(64, Some(true)));
    assert!(!e.set_pin(-1, Some(true)));
    assert!(!e.pin_read(64));
    assert!(!e.pin_read(26), "pin 26 still driven LOW");
}

/// The pin-mode bit and the injection are independent state: driving a pin
/// then reconfiguring it keeps the driven level, and releasing it falls back
/// to whatever `pinMode` last asked for.
#[test]
fn pin_injection_and_pin_mode_compose() {
    let mut e = engine("pinMode(26, INPUT)\nexport var out = 0");
    assert!(!e.pin_read(26)); // plain INPUT idles LOW
    assert!(e.set_pin(26, Some(true)));
    assert!(e.pin_read(26));
    assert!(e.set_pin(26, None));
    assert!(!e.pin_read(26)); // released → back to the INPUT idle level

    let mut e = engine("pinMode(26, INPUT_PULLUP)\nexport var out = 0");
    assert!(e.pin_read(26));
    assert!(e.set_pin(26, Some(false)));
    assert!(!e.pin_read(26)); // injection beats the pull-up
    assert!(e.set_pin(26, None));
    assert!(e.pin_read(26)); // released → back to the pull-up idle level

    // A `pinMode` the pattern runs LATER does not knock the injection loose:
    // the host is standing in for a wire, and a wire does not come off
    // because the pattern reconfigured the pad.
    let src = "export var out = 0\n\
               export function beforeRender(delta) { pinMode(26, INPUT); out = digitalRead(26) }";
    let mut e = Engine::new(src, 10, 1).expect("compile error");
    assert!(e.set_pin(26, Some(true)));
    e.frame(Fx::from_int(16));
    assert_eq!(e.var("out"), Some(Value::Num(Fx::ONE)));
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
fn easing_library_thirty() {
    // The rest of the standard thirty easings (ten families × in/out/in-out
    // from the public easings.net reference). The quad/cubic trios and the
    // "out" springs are covered by extension_builtins above.
    let names = [
        "easeInSine", "easeOutSine", "easeInOutSine",
        "easeInQuart", "easeOutQuart", "easeInOutQuart",
        "easeInQuint", "easeOutQuint", "easeInOutQuint",
        "easeInExpo", "easeOutExpo", "easeInOutExpo",
        "easeInCirc", "easeOutCirc", "easeInOutCirc",
        "easeInBack", "easeInOutBack",
        "easeInElastic", "easeInOutElastic",
        "easeInBounce", "easeInOutBounce",
    ];
    // every easing pins its endpoints: ease*(0) = 0, ease*(1) = 1
    for f in names {
        assert!(eval(&format!("{f}(0)")).to_f64().abs() < 1e-2, "{f}(0) != 0");
        assert!((eval(&format!("{f}(1)")).to_f64() - 1.0).abs() < 1e-2, "{f}(1) != 1");
    }
    // shape: reference values at t = 0.25 / 0.5 / 0.75 (f64 math on the
    // published formulas; fixed point tracks them to well under a color step)
    let refs: [(&str, [f64; 3]); 21] = [
        ("easeInSine", [0.0761, 0.2929, 0.6173]),
        ("easeOutSine", [0.3827, 0.7071, 0.9239]),
        ("easeInOutSine", [0.1464, 0.5000, 0.8536]),
        ("easeInQuart", [0.0039, 0.0625, 0.3164]),
        ("easeOutQuart", [0.6836, 0.9375, 0.9961]),
        ("easeInOutQuart", [0.0313, 0.5000, 0.9688]),
        ("easeInQuint", [0.0010, 0.0313, 0.2373]),
        ("easeOutQuint", [0.7627, 0.9688, 0.9990]),
        ("easeInOutQuint", [0.0156, 0.5000, 0.9844]),
        ("easeInExpo", [0.0055, 0.0313, 0.1768]),
        ("easeOutExpo", [0.8232, 0.9688, 0.9945]),
        ("easeInOutExpo", [0.0156, 0.5000, 0.9844]),
        ("easeInCirc", [0.0318, 0.1340, 0.3386]),
        ("easeOutCirc", [0.6614, 0.8660, 0.9682]),
        ("easeInOutCirc", [0.0670, 0.5000, 0.9330]),
        ("easeInBack", [-0.0641, -0.0877, 0.1826]),
        ("easeInOutBack", [-0.0997, 0.5000, 1.0997]),
        ("easeInElastic", [-0.0055, -0.0156, 0.0884]),
        ("easeInOutElastic", [0.0120, 0.5000, 0.9880]),
        ("easeInBounce", [0.0273, 0.2344, 0.5273]),
        ("easeInOutBounce", [0.1172, 0.5000, 0.8828]),
    ];
    for (f, want) in refs {
        for (t, want) in ["0.25", "0.5", "0.75"].iter().zip(want) {
            let got = eval(&format!("{f}({t})")).to_f64();
            assert!((got - want).abs() < 1e-2, "{f}({t}) = {got}, want {want}");
        }
    }
    // the springs are deliberately out of range mid-curve: back anticipates
    // below 0 on the way in, its in-out form overshoots past 1 on the way out
    assert!(eval("easeInBack(0.4)").to_f64() < 0.0);
    assert!(eval("easeInOutBack(0.8)").to_f64() > 1.0);
    // elastic-in winds up backwards before releasing
    assert!(eval("easeInElastic(0.6)").to_f64() < 0.0);
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
fn event_injection() {
    let src = "export var cnt = 0\nexport var got = 0\n\
               export var t = -1\nexport var x = -1\nexport var y = -1\nexport var v = -1\n\
               var ev = array(4)\n\
               export function beforeRender(delta) {\n\
                 cnt = eventCount()\n\
                 while (readEvent(ev)) { got = got + 1; t = ev[0]; x = ev[1]; y = ev[2]; v = ev[3] }\n\
               }\n\
               export function render(index) { hsv(0, 0, 0) }";
    let mut e = Engine::new(src, 1, 1).expect("compile");
    let num = |e: &Engine, n: &str| e.var(n).unwrap().num().to_f64();
    // no events: count 0, readEvent returns 0, out untouched
    e.frame(Fx::ZERO);
    assert_eq!(num(&e, "cnt"), 0.0);
    assert_eq!(num(&e, "got"), 0.0);
    assert_eq!(num(&e, "t"), -1.0);
    // two events drain FIFO in one frame; the last one sticks
    e.push_event([Fx::from_int(1), Fx::from_f64(0.25), Fx::from_f64(0.5), Fx::ONE]);
    e.push_event([Fx::from_int(2), Fx::from_f64(0.75), Fx::ZERO, Fx::from_f64(0.5)]);
    e.frame(Fx::ZERO);
    assert_eq!(num(&e, "cnt"), 2.0, "eventCount sees both before the drain");
    assert_eq!(num(&e, "got"), 2.0);
    assert_eq!(num(&e, "t"), 2.0, "FIFO: the second event is read last");
    assert!((num(&e, "x") - 0.75).abs() < 1e-4);
    assert!((num(&e, "v") - 0.5).abs() < 1e-4);
    // drained: next frame sees an empty queue again
    e.frame(Fx::ZERO);
    assert_eq!(num(&e, "cnt"), 0.0);
    assert_eq!(num(&e, "got"), 2.0);
}

#[test]
fn event_queue_overflow_drops_oldest() {
    let src = "export var first = -1\nexport var n = 0\nvar ev = array(4)\n\
               export function beforeRender(delta) {\n\
                 while (readEvent(ev)) { n = n + 1; if (first == -1) { first = ev[3] } }\n\
               }\n\
               export function render(index) { hsv(0, 0, 0) }";
    let mut e = Engine::new(src, 1, 1).expect("compile");
    // 40 events into a 32-slot queue: 0..7 fall off the front
    for i in 0..40 {
        e.push_event([Fx::ZERO, Fx::ZERO, Fx::ZERO, Fx::from_int(i)]);
    }
    e.frame(Fx::ZERO);
    assert_eq!(e.var("n").unwrap().num().to_f64(), 32.0);
    assert_eq!(
        e.var("first").unwrap().num().to_f64(),
        8.0,
        "drop-oldest: the first surviving event is #8"
    );
}

#[test]
fn read_event_bad_out_is_a_clean_vmerr() {
    // a non-array `out` only errors when there IS an event to deliver
    let src = "export function beforeRender(delta) { readEvent(3) }\n\
               export function render(index) { hsv(0, 0, 0) }";
    let mut e = Engine::new(src, 1, 1).expect("compile");
    e.frame(Fx::ZERO);
    assert!(e.take_error().is_none(), "empty queue: no error, returns 0");
    e.push_event([Fx::ZERO; 4]);
    e.frame(Fx::ZERO);
    let err = e.take_error().expect("vmerr");
    assert!(err.message.contains("must be an array"), "{}", err.message);
    // and a too-short array errors too
    let src2 = "var ev = array(2)\n\
                export function beforeRender(delta) { readEvent(ev) }\n\
                export function render(index) { hsv(0, 0, 0) }";
    let mut e2 = Engine::new(src2, 1, 1).expect("compile");
    e2.push_event([Fx::ZERO; 4]);
    e2.frame(Fx::ZERO);
    let err2 = e2.take_error().expect("vmerr");
    assert!(err2.message.contains("length >= 4"), "{}", err2.message);
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
fn canvas_add_accumulates_in_the_same_cells_as_canvas_set() {
    // canvasAdd addresses exactly like canvasSet — floor(x·w), edges
    // clamped — but read-modify-writes the cell (particle deposits)
    assert_eq!(
        eval_prog("c = array(16)\ncanvasAdd(c, 4, 0.99, 0, 7)\nexport var out = c[3]"),
        fx(7.0)
    );
    assert_eq!(
        eval_prog(
            "c = array(16)\ncanvasAdd(c, 4, 0.3, 0.6, 0.25)\ncanvasAdd(c, 4, 0.3, 0.6, 0.5)\n\
             export var out = c[9]"
        ),
        fx(0.75)
    );
    // same cell as canvasSet for every corner, including x = 1 → last column
    assert_eq!(
        eval_prog(
            "a = array(16)\nb = array(16)\n\
             for (i = 0; i < 5; i++) {\n\
               x = i / 4\n  canvasSet(a, 4, x, 1, 1)\n  canvasAdd(b, 4, x, 1, 1)\n}\n\
             export var out = 0\n\
             for (i = 0; i < 16; i++) if (a[i] != min(b[i], 1)) out = 1"
        ),
        Fx::ZERO
    );
    // negative deposits subtract; the cell keeps its running total
    assert_eq!(
        eval_prog("c = array(4)\nc[0] = 1\ncanvasAdd(c, 2, 0, 0, -0.25)\nexport var out = c[0]"),
        fx(0.75)
    );
    // returns the cell's NEW value (`cell += v`, like JS `+=`)
    assert_eq!(
        eval_prog("c = array(4)\nc[0] = 2\nexport var out = canvasAdd(c, 2, 0, 0, 3)"),
        fx(5.0)
    );
    // degenerate canvas: nothing written, returns v — where canvasSet
    // also returns v untouched
    assert_eq!(
        eval_prog("c = array(4)\nexport var out = canvasAdd(c, 0, 0, 0, 3)"),
        fx(3.0)
    );
    assert_eq!(
        eval_prog("c = array(2)\nexport var out = canvasAdd(c, 4, 0, 0, 3)"),
        fx(3.0)
    );
    // …and a non-array first argument is the same clean runtime error
    for src in ["canvasAdd(5, 4, 0, 0, 1)", "canvasSet(5, 4, 0, 0, 1)"] {
        let e = Engine::new(src, 10, 1).unwrap();
        let err = e
            .last_error
            .unwrap_or_else(|| panic!("expected error for {src:?}"));
        assert!(err.message.contains("of a non-array"), "{}", err.message);
    }
}

#[test]
fn random_seed_pins_the_documented_sequence() {
    // The generator is part of the contract (docs/lang.md "Determinism and
    // seeding"): splitmix64, state = the seed's raw 16.16 word, output =
    // the low 32 bits, scaled by (r · max) >> 32. Same seed → same stream
    // on firmware, WASM and CLI, which is what synced installations need.
    // These values are computed independently below, not copied from a run.
    fn splitmix_next(state: &mut u64) -> u32 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as u32
    }
    let mut state = Fx::from_int(7).raw() as u32 as u64; // randomSeed(7)
    for i in 0..8 {
        let want = Fx::from_raw(((splitmix_next(&mut state) as u64 * Fx::ONE.raw() as u64) >> 32)
            as i32);
        let got = eval_prog(&format!(
            "randomSeed(7)\nexport var out\nfor (i = 0; i <= {i}; i++) out = random(1)"
        ));
        assert_eq!(got, want, "random draw {i} after randomSeed(7)");
    }
    // Fractional seeds are distinct states (the raw 16.16 word is the seed)
    assert_ne!(
        eval_prog("randomSeed(1)\nexport var out = random(1)"),
        eval_prog("randomSeed(1.5)\nexport var out = random(1)")
    );
    // Reseeding restarts the same stream
    assert_eq!(
        eval_prog("randomSeed(3)\nrandom(1)\nrandom(1)\nrandomSeed(3)\nexport var out = random(1)"),
        eval_prog("randomSeed(3)\nexport var out = random(1)")
    );
    // …and returns the PREVIOUS seed (0 before the stream is ever seeded)
    assert_eq!(eval_prog("export var out = randomSeed(4)"), Fx::ZERO);
    assert_eq!(
        eval_prog("randomSeed(4.5)\nexport var out = randomSeed(9)"),
        fx(4.5)
    );
}

#[test]
fn prng_pins_the_documented_sequence() {
    // prng() is xorshift32 (Marsaglia 13/17/5) over the seed's raw 16.16
    // word — pinned so a seeded pattern reproduces across Luxel devices.
    // Unchanged from the sequence Luxel has always produced.
    fn xorshift_next(x: &mut u32) -> u32 {
        *x ^= *x << 13;
        *x ^= *x >> 17;
        *x ^= *x << 5;
        *x
    }
    let mut state = Fx::from_int(7).raw() as u32; // prngSeed(7)
    for i in 0..8 {
        let want =
            Fx::from_raw(((xorshift_next(&mut state) as u64 * Fx::ONE.raw() as u64) >> 32) as i32);
        let got = eval_prog(&format!(
            "prngSeed(7)\nexport var out\nfor (i = 0; i <= {i}; i++) out = prng(1)"
        ));
        assert_eq!(got, want, "prng draw {i} after prngSeed(7)");
    }
    // prngSeed's state is 32 bits, so its return value round-trips exactly:
    // save the state, draw from it, restore, and the same draw comes back.
    // (prngSeed(0) is the one exception — state 0 is xorshift32's fixed
    // point, so it is remapped to 1.)
    assert_eq!(
        eval_prog(
            "prngSeed(11)\nprng(1)\ns = prngSeed(0)\nprngSeed(s)\nfirst = prng(1)\nprngSeed(s)\n\
             export var out = prng(1) - first"
        ),
        Fx::ZERO
    );
    // seeding does not disturb random()'s independent stream
    assert_eq!(
        eval_prog("randomSeed(2)\nprngSeed(99)\nprng(1)\nexport var out = random(1)"),
        eval_prog("randomSeed(2)\nexport var out = random(1)")
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

/// A frame of five pixels' red channel, for the post-process chain tests.
fn red_frame(body: &str, pixels: u32) -> Vec<u8> {
    let src = format!("export function render(index) {{ {body} }}");
    let mut e = Engine::new(&src, pixels, 1).expect("compile");
    e.frame(Fx::ZERO).iter().map(|p| p[0]).collect()
}

#[test]
fn post_process_blur_stage() {
    let spike = "rgb(index == 2, 0, 0)";
    assert_eq!(red_frame(spike, 5), vec![0, 0, 255, 0, 0]);
    // setBlur(a): each neighbour's weight is a/2 in 1/256ths, so 0.5 is
    // the classic 1-2-1 kernel — 255 becomes 128 with 64 either side.
    assert_eq!(
        red_frame(&format!("setBlur(0.5); {spike}"), 5),
        vec![0, 64, 128, 64, 0]
    );
    // a second pass widens it (ends clamp, so light doesn't fall off)
    assert_eq!(
        red_frame(&format!("setBlur(0.5, 2); {spike}"), 5),
        vec![16, 64, 96, 64, 16]
    );
    // amount 1 is a pure neighbour average: the lit pixel goes dark and
    // both neighbours take half of it
    assert_eq!(
        red_frame(&format!("setBlur(1); {spike}"), 5),
        vec![0, 128, 0, 128, 0]
    );
    // off by default and explicitly off at 0
    assert_eq!(red_frame(&format!("setBlur(0); {spike}"), 5), vec![0, 0, 255, 0, 0]);
    // the stage runs ONCE per frame, not once per pixel — two frames of a
    // static pattern give the same answer (no compounding)
    let src = format!("export function render(index) {{ setBlur(0.5); {spike} }}");
    let mut e = Engine::new(&src, 5, 1).expect("compile");
    let first: Vec<u8> = e.frame(Fx::ZERO).iter().map(|p| p[0]).collect();
    let second: Vec<u8> = e.frame(Fx::from_int(16)).iter().map(|p| p[0]).collect();
    assert_eq!(first, second, "blur compounded across frames");
}

#[test]
fn post_process_glow_stage() {
    let spike = "rgb(index == 2, 0, 0)";
    // glow keeps the source at full and bleeds `amount` of it outward
    assert_eq!(
        red_frame(&format!("setGlow(0.5); {spike}"), 5),
        vec![0, 127, 255, 127, 0]
    );
    assert_eq!(
        red_frame(&format!("setGlow(1); {spike}"), 5),
        vec![0, 255, 255, 255, 0]
    );
    assert_eq!(red_frame(&format!("setGlow(0); {spike}"), 5), vec![0, 0, 255, 0, 0]);
}

/// A W×H row-major (or serpentine) map in the coordinate units a host sends.
fn grid_map(w: usize, h: usize, serpentine: bool) -> Vec<[Fx; 3]> {
    let mut v = Vec::new();
    for r in 0..h {
        for c in 0..w {
            let x = if serpentine && r % 2 == 1 { w - 1 - c } else { c };
            v.push([Fx::from_int(x as i32), Fx::from_int(r as i32), Fx::ZERO]);
        }
    }
    v
}

#[test]
fn post_process_spatial_stages_follow_an_installed_grid() {
    // A 4x4 panel wired row by row: pixel 3 ends row 0 and pixel 4 starts
    // row 1, at the opposite edge. Blur must follow the panel, not the wire.
    let spike = "rgb(index == 3, 0, 0)";
    let src = format!("export function render(index) {{ setBlur(0.5); {spike} }}");
    let mut e = Engine::new(&src, 16, 1).expect("compile");
    e.set_map(2, &grid_map(4, 4, false));
    let g = e.grid().expect("a 4x4 row-major map is a grid");
    assert_eq!((g.w, g.h, g.serpentine), (4, 4, false));
    let f: Vec<u8> = e.frame(Fx::ZERO).iter().map(|p| p[0]).collect();
    assert_eq!(f[g.index(1, 0)], 0, "light jumped the fold to the far edge");
    assert!(f[g.index(1, 3)] > 0, "no vertical spread");

    // Same pattern, no map: the index-space behaviour is exactly unchanged.
    let mut e = Engine::new(&src, 16, 1).expect("compile");
    assert!(e.grid().is_none());
    let f: Vec<u8> = e.frame(Fx::ZERO).iter().map(|p| p[0]).collect();
    assert_eq!(&f[..6], &[0, 0, 64, 128, 64, 0]);

    // Serpentine wiring blooms symmetrically about the lit cell.
    let src = "export function render(index) { setGlow(0.5); rgb(index == 20, 0, 0) }";
    let mut e = Engine::new(src, 36, 1).expect("compile");
    e.set_map(2, &grid_map(6, 6, true));
    let g = e.grid().expect("a 6x6 serpentine map is a grid");
    assert!(g.serpentine);
    assert_eq!(g.index(3, 3), 20);
    let f: Vec<u8> = e.frame(Fx::ZERO).iter().map(|p| p[0]).collect();
    assert_eq!(f[g.index(3, 3)], 255);
    for (r, c) in [(2, 3), (4, 3), (3, 2), (3, 4)] {
        assert_eq!(f[g.index(r, c)], 127, "neighbour ({r},{c})");
    }

    // A map that isn't a grid keeps index space (nothing to be aware of).
    let mut e = Engine::new(&src.replace("index == 20", "index == 2"), 5, 1).expect("compile");
    let ring: Vec<[Fx; 3]> = (0..5)
        .map(|i| [Fx::from_int(i % 3), Fx::from_int((i * 2) % 5), Fx::ZERO])
        .collect();
    e.set_map(2, &ring);
    assert!(e.grid().is_none());
    let f: Vec<u8> = e.frame(Fx::ZERO).iter().map(|p| p[0]).collect();
    assert_eq!(f, vec![0, 127, 255, 127, 0]);
}

#[test]
fn post_process_palette_remap_stage() {
    // stops: black at 0, red at 1 — every luma becomes red at that level
    let pal = "p = array(8)\np[4] = 1\np[5] = 1\n";
    let src = format!(
        "{pal}export function render(index) {{ setOutputPalette(p); rgb(0, 1, 0) }}"
    );
    let mut e = Engine::new(&src, 1, 1).expect("compile");
    // green quantizes to [0,255,0]; luma = (255·183)>>8 = 182; the table
    // entry for 182 is the palette sampled at 182/255, i.e. red 181/255
    assert_eq!(e.frame(Fx::ZERO)[0], [181, 0, 0]);
    // amount blends the remap against the original
    let src = format!(
        "{pal}export function render(index) {{ setOutputPalette(p, 0.5); rgb(0, 1, 0) }}"
    );
    let mut e = Engine::new(&src, 1, 1).expect("compile");
    assert_eq!(e.frame(Fx::ZERO)[0], [90, 127, 0]);
    // a non-array argument clears the stage
    let src = format!(
        "{pal}export function render(index) {{ setOutputPalette(p); setOutputPalette(0); rgb(0, 1, 0) }}"
    );
    let mut e = Engine::new(&src, 1, 1).expect("compile");
    assert_eq!(e.frame(Fx::ZERO)[0], [0, 255, 0]);
    // no output palette installed → untouched
    let mut e = Engine::new("export function render(index) { rgb(0, 1, 0) }", 1, 1).expect("compile");
    assert_eq!(e.frame(Fx::ZERO)[0], [0, 255, 0]);
}

#[test]
fn post_process_chain_order() {
    // gamma is the LAST stage: it curves what blur produced, so the
    // spike's blurred 128 lands on the same value a bare 128 would
    let spike = "rgb(index == 2, 0, 0)";
    let curved = red_frame(&format!("setGamma(2); setBlur(0.5); {spike}"), 5);
    // blur leaves 128 at the peak; γ2 takes (128/255)² back to 64. Were
    // gamma applied first, the lit pixel would still be a full 255 going
    // into the blur and the peak would come out at 128.
    assert!((curved[2] as i32 - 64).abs() <= 1, "peak = {}", curved[2]);
    assert!(curved[1] < 64 && curved[1] > 0, "blurred wing = {}", curved[1]);
    // the remap feeds the blur, not the other way round: with a black→red
    // palette the green spike comes out red, then spreads
    let pal = "p = array(8)\np[4] = 1\np[5] = 1\n";
    let src = format!(
        "{pal}export function render(index) {{ setOutputPalette(p); setBlur(0.5); rgb(0, index == 2, 0) }}"
    );
    let mut e = Engine::new(&src, 5, 1).expect("compile");
    let f: Vec<[u8; 3]> = e.frame(Fx::ZERO).to_vec();
    assert_eq!(f[2], [91, 0, 0], "remap then blur: {:?}", f[2]);
    assert_eq!(f[1], [45, 0, 0], "wing: {:?}", f[1]);
    assert!(f.iter().all(|p| p[1] == 0), "green survived the remap");
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
