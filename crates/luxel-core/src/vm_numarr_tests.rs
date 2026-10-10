//! The word form of an all-number array (`ArrRepr::Num`, Gitea #947)
//! against the `Value` form it replaced.
//!
//! [`super::FORCE_VALUE_FORM`] makes every allocation the pre-#947
//! `ArrRepr::Owned` form, so each case below runs the SAME pattern twice —
//! once as shipped, once forced — and demands byte-identical pixels,
//! errors, `/api/vars` dumps and arena contents. Then the promotion rules
//! on their own: a handle store promotes in place and re-charges exactly
//! the byte difference, an out-of-range one promotes nothing, and a
//! promotion over the byte budget is a clean, recorded runtime error.

use super::{ArrRepr, Program, Value, Vm, FORCE_VALUE_FORM};
use crate::compile::compile;
use crate::engine::Engine;
use crate::fixed::Fx;
use alloc::string::String;
use alloc::vec::Vec;

/// One engine run, reduced to everything observable.
#[derive(Debug, PartialEq)]
struct Run {
    frames: Vec<Vec<[u8; 3]>>,
    error: Option<String>,
    vars: String,
    arena: Vec<Vec<Value>>,
    elems: usize,
}

/// Whether the word form was used anywhere — so a case that is meant to
/// exercise it cannot silently pass by never reaching it.
fn has_num(vm: &Vm) -> bool {
    vm.arrays.iter().any(|a| matches!(a, ArrRepr::Num(_)))
}

fn run(src: &str, grid: bool, force: bool) -> (Run, usize, bool) {
    FORCE_VALUE_FORM.with(|f| f.set(force));
    let mut e = Engine::new(src, 64, 0x5EED).unwrap_or_else(|d| panic!("{d:?}\n{src}"));
    if grid {
        e.set_grid_map(8, 8);
    }
    let mut frames = Vec::new();
    for k in 0..4 {
        if k == 1 {
            e.push_event([Fx::ONE, Fx::from_raw(0x4000), Fx::from_raw(0xC000), Fx::ONE]);
            let mut s = crate::engine::SensorFrame::default();
            for (i, v) in s.frequency_data.iter_mut().enumerate() {
                *v = Fx::from_raw(i as i32 * 977);
            }
            s.accelerometer = [Fx::ONE, Fx::from_raw(-7000), Fx::from_raw(123)];
            e.set_sensors(&s);
        }
        frames.push(e.frame(Fx::from_int(16)).to_vec());
    }
    FORCE_VALUE_FORM.with(|f| f.set(false));
    let vm = e.vm();
    let prog: &Program = e.program();
    let arena = (0..vm.arena_slots() as u32)
        .map(|id| vm.array(prog, id).map(|a| a.to_vec()).unwrap_or_default())
        .collect();
    let r = Run {
        frames,
        error: e.last_error.as_ref().map(|m| m.message.clone()),
        vars: String::from(crate::jsonview::vars_json(&e).as_str()),
        arena,
        elems: vm.arena_elems(),
    };
    (r, vm.arena_bytes(), has_num(vm))
}

/// Run `src` both ways; they must agree on everything but the bytes,
/// which the word form must never charge MORE of.
fn same_both_ways(name: &str, src: &str, grid: bool) {
    let (words, wbytes, wnum) = run(src, grid, false);
    let (vals, vbytes, vnum) = run(src, grid, true);
    assert!(!vnum, "{name}: the forced run still made a word-form array");
    assert!(wnum, "{name}: the case never reached the word form");
    assert_eq!(words, vals, "{name}: word form and Value form disagree");
    // a case that errors out before it renders anything proves nothing
    if !name.starts_with("out-of-range") {
        assert_eq!(words.error, None, "{name}");
    } else {
        assert!(words.error.is_some(), "{name}");
    }
    let lit = words.frames.iter().flatten().any(|p| *p != [0, 0, 0]);
    assert!(lit, "{name}: rendered nothing");
    assert!(
        wbytes <= vbytes,
        "{name}: word form charged more ({wbytes} > {vbytes})"
    );
}

#[test]
fn every_array_builtin_matches_the_value_form() {
    let cases: &[(&str, &str, bool)] = &[
        (
            "index load/store, literals, const copy-on-write",
            "export var a = array(16)\nexport var lit\nexport var k = [3, 1, 2]\n\
             export var t = 0\n\
             export function beforeRender(d) { t += d\n\
               for (i = 0; i < 16; i++) a[i] = sin(i * 0.3 + t * 0.001)\n\
               lit = [a[1], a[2], t]\n k[1] = k[1] + 0.5 }\n\
             export function render(i) { hsv(a[i % 16] + k[1], 1, lit[0] * 0.5 + 0.5) }",
            false,
        ),
        (
            "arrayReplace / arrayReplaceAt, numbers and a handle",
            "export var a = array(6)\nexport var b = array(3)\n\
             arrayReplace(a, 1, 2, 3)\narrayReplaceAt(a, 3, 0.25, 0.5)\n\
             arrayReplaceAt(b, 1, a)\n\
             export function render(i) { hsv(a[i % 6], 1, a[(i + 1) % 6]) }",
            false,
        ),
        (
            "arraySort / arraySortBy / arrayMutate / arrayMapTo / arrayForEach / arrayReduce / arraySum / arrayLength",
            "export var a = array(20)\nexport var m = array(20)\nexport var s\nexport var r\n\
             for (i = 0; i < 20; i++) a[i] = ((i * 7919) % 23) / 23 - 0.4\n\
             arraySort(a)\n\
             arraySortBy(m, (x, y) => y - x)\n\
             arrayMutate(m, (v, i) => a[19 - i] * 2)\n\
             arraySortBy(m, (x, y) => x - y)\n\
             arrayMapTo(a, m, (v) => v * v)\n\
             arrayForEach(m, (v, i) => { a[i] = a[i] + v })\n\
             r = arrayReduce(a, (acc, v) => acc + v, 0)\n\
             s = arraySum(m) + arrayLength(m)\n\
             export function render(i) { hsv(a[i % 20], 1, m[i % 20] + s * 0.001) }",
            false,
        ),
        (
            "arrayMapTo storing handles promotes",
            "export var a = array(4)\nexport var h = array(4)\n\
             arrayMapTo(a, h, (v, i) => i == 2 ? a : v + i)\n\
             export var n = h[1]\n\
             export function render(i) { hsv(n * 0.1, 1, 1) }",
            false,
        ),
        (
            "blur1D / feedback / arrayScale / arrayMaxAbs",
            "export var a = array(64)\nexport var mx\n\
             export function beforeRender(d) {\n\
               a[(time(0.05) * 64) | 0] = 1\n a[7] = -0.75\n\
               blur1D(a, 2)\n feedback(a, 0.9)\n arrayScale(a, 1.01)\n mx = arrayMaxAbs(a) }\n\
             export function render(i) { hsv(0.6, 1, a[i] / (mx + 0.01)) }",
            false,
        ),
        (
            "blur2D / canvasSet / canvasAdd / canvasGet",
            "export var c = array(64)\nexport var g\n\
             export function beforeRender(d) {\n\
               canvasSet(c, 8, time(0.03), 0.4, 1)\n canvasAdd(c, 8, 0.7, time(0.05), 0.5)\n\
               blur2D(c, 8, 8, 1)\n g = canvasGet(c, 8, 0.33, 0.66) }\n\
             export function render2D(i, x, y) { hsv(g, 1, canvasGet(c, 8, x, y)) }\n\
             export function render(i) { hsv(g, 1, c[i]) }",
            true,
        ),
        (
            "canvasSet with a handle promotes, a number does not",
            "export var c = array(16)\nexport var o = array(3)\n\
             canvasSet(c, 4, 0.5, 0.5, 2)\ncanvasSet(o, 1, 0, 0.9, c)\n\
             export function render(i) { hsv(c[i % 16], 1, 1) }",
            false,
        ),
        (
            "arrayAdd / arraySub / arrayMix, aliased and not",
            "export var a = array(32)\nexport var b = array(32)\nexport var k = [0.1, 0.2, 0.3]\n\
             export function beforeRender(d) {\n\
               for (i = 0; i < 32; i++) b[i] = wave(i / 32 + time(0.02))\n\
               arrayAdd(a, b)\n arrayMix(a, b, 0.3)\n arraySub(a, k)\n\
               arrayAdd(k, k)\n arrayMix(a, a, 0.5)\n arraySub(b, b) }\n\
             export function render(i) { hsv(a[i % 32], 1, 0.5 + k[i % 3] * 0.01) }",
            false,
        ),
        (
            "fillNoise2D / fillNoise3D / stencil2D",
            "export var n = array(64)\nexport var m = array(64)\nexport var w = array(64)\n\
             export function beforeRender(d) {\n\
               fillNoise2D(n, 8, 8, 0.2, 0.2, time(0.1), 0, 3)\n\
               fillNoise3D(m, 8, 8, 0.3, 0.3, 0, 0, time(0.07), 1)\n\
               stencil2D(w, n, 8, 8, 0.5, 0.125, 0.0625) }\n\
             export function render(i) { hsv(n[i], 1, abs(w[i]) + m[i] * 0.1) }",
            false,
        ),
        (
            "hsv2rgb / rgb2hsv / mixColors / curl2 / curl3 out-arrays",
            "export var o = array(3)\nexport var p = array(3)\nexport var q = [0, 0, 0]\n\
             export function render(i) {\n\
               hsv2rgb(i / 64, 1, 1, o)\n rgb2hsv(o[0], o[1], o[2], p)\n\
               mixColors(o[0], o[1], o[2], 0, 0, 1, 0.3, q)\n\
               curl2(i * 0.1, 0.5, o)\n curl3(i * 0.1, 0.5, 0.2, p)\n\
               rgb(abs(o[0]) * 0.1 + q[0], abs(p[2]) * 0.1, p[0] * 0.01 + q[2]) }",
            false,
        ),
        (
            "readEvent and the sensor arrays",
            "export var ev = array(4)\nexport var hit = 0\nexport var frequencyData\n\
             export var accelerometer\n\
             export function beforeRender(d) { while (readEvent(ev)) hit = ev[1] + ev[2] }\n\
             export function render(i) { hsv(hit + frequencyData[i % 32], 1, accelerometer[0]) }",
            false,
        ),
        (
            "setPalette over a word-form array, written through",
            "export var pal = array(8)\n\
             arrayReplace(pal, 0, 1, 0, 0, 1, 0, 0, 1)\nsetPalette(pal)\n\
             export function beforeRender(d) { pal[5] = time(0.04) }\n\
             export function render(i) { paint(i / 64, 1) }",
            false,
        ),
        (
            "renderBulk fills read word-form arrays",
            "export var h = array(64)\nexport var v = array(64)\nexport var c = array(16)\n\
             export var pal = [0, 0, 0, 1, 1, 1, 0.5, 0]\nsetPalette(pal)\n\
             export function beforeRender(d) {\n\
               for (i = 0; i < 64; i++) { h[i] = i / 64 + time(0.05); v[i] = wave(i / 16) }\n\
               for (i = 0; i < 16; i++) c[i] = i / 16 }\n\
             export function renderBulk() {\n\
               fillHSV(h, 1, v)\n fillCanvas(c, 1, c, 4, 4)\n paintCanvas(c, 4, 4, v) }",
            true,
        ),
        (
            "setOutputPalette from a word-form array",
            "export var pal = array(8)\n\
             arrayReplace(pal, 0, 0, 0, 0.5, 1, 1, 0.5, 0)\nsetOutputPalette(pal)\n\
             export function render(i) { hsv(i / 64, 1, 1) }",
            false,
        ),
        (
            "out-of-range store errors identically",
            "export var a = array(4)\nexport var x = 0\n\
             export function beforeRender(d) { x += 1\n if (x > 2) a[4] = 1 }\n\
             export function render(i) { hsv(a[0], 1, 1) }",
            false,
        ),
    ];
    for (name, src, grid) in cases {
        same_both_ways(name, src, *grid);
    }
}

/// A VM over `src` with top-level init run.
fn vm_with(src: &str, byte_budget: usize) -> (Program, Vm, Result<Value, super::VmError>) {
    let p = compile(src).unwrap_or_else(|d| panic!("{d:?}"));
    let mut vm = Vm::new(&p, 1);
    vm.array_byte_budget = byte_budget;
    let r = vm.call(&p, 0, &[]);
    (p, vm, r)
}

fn global(p: &Program, vm: &Vm, name: &str) -> Value {
    vm.globals[p.global_index(name).expect("global") as usize]
}

#[test]
fn a_handle_store_promotes_in_place_and_recharges_the_difference() {
    let (p, mut vm, r) = vm_with(
        "export var a = array(10)\nexport var b = array(2)\n\
         for (i = 0; i < 10; i++) a[i] = i * 0.5 - 1\n",
        usize::MAX,
    );
    r.expect("init");
    let Value::Arr(a) = global(&p, &vm, "a") else {
        panic!()
    };
    let b = global(&p, &vm, "b");
    assert!(matches!(vm.arrays[a as usize], ArrRepr::Num(_)));
    let (bytes, elems) = (vm.arena_bytes(), vm.arena_elems());
    // (10 + 2) × 4 + 2 × 32: the word form's charge.
    assert_eq!(bytes, 12 * 4 + 64);

    // out of range: an error, and nothing promoted
    assert!(vm.index_write(&p, a, 10, b).is_err());
    assert!(matches!(vm.arrays[a as usize], ArrRepr::Num(_)));
    assert_eq!(vm.arena_bytes(), bytes);

    vm.index_write(&p, a, 3, b).expect("promoting store");
    assert!(matches!(vm.arrays[a as usize], ArrRepr::Owned(_)));
    assert_eq!(
        vm.arena_bytes(),
        bytes + 10 * 4,
        "re-charged 4 B per element"
    );
    assert_eq!(
        vm.arena_elems(),
        elems,
        "the PB element ledger is untouched"
    );
    let view = vm.arr(&p, a);
    for i in 0..10 {
        let want = if i == 3 {
            b
        } else {
            Value::Num(Fx::from_raw(
                Fx::from_int(i as i32).raw() / 2 - Fx::ONE.raw(),
            ))
        };
        assert_eq!(view.at(i), want, "element {i}");
    }
    // a number into the promoted array is a plain store, no further charge
    vm.index_write(&p, a, 4, Value::Num(Fx::ONE))
        .expect("store");
    assert_eq!(vm.arena_bytes(), bytes + 10 * 4);
}

#[test]
fn a_promotion_over_the_byte_budget_fails_cleanly() {
    // Exactly the word form's bill: init fits, the promotion cannot.
    let src = "export var a = array(100)\nexport var b = array(1)\n\
               export function render(i) { a[i] = b }";
    let budget = (100 + 1) * 4 + 2 * 32;
    let (p, mut vm, r) = vm_with(src, budget);
    r.expect("init fits the word form");
    let Value::Arr(a) = global(&p, &vm, "a") else {
        panic!()
    };
    let b = global(&p, &vm, "b");
    let before = vm.arena_bytes();
    let e = vm.index_write(&p, a, 0, b).expect_err("over budget");
    assert!(e.starts_with("array memory budget exceeded"), "{e}");
    assert!(super::is_array_budget_error(&e));
    assert!(
        matches!(vm.arrays[a as usize], ArrRepr::Num(_)),
        "left as it was"
    );
    assert_eq!(vm.arena_bytes(), before);

    // and through the interpreter: a recorded runtime error, not a panic
    let render = p.exported_fn("render").expect("render");
    let err = vm
        .call(&p, render, &[Value::Num(Fx::ZERO)])
        .expect_err("refused");
    assert!(
        err.message.starts_with("array memory budget exceeded"),
        "{}",
        err.message
    );
    assert!(matches!(vm.arrays[a as usize], ArrRepr::Num(_)));

    // the same pattern on the Value form would not even have loaded
    FORCE_VALUE_FORM.with(|f| f.set(true));
    let (_, _, r) = vm_with(src, budget);
    FORCE_VALUE_FORM.with(|f| f.set(false));
    assert!(r.is_err(), "the Value form needs twice the element bytes");
}

#[test]
fn literals_choose_their_form_by_content() {
    let (p, vm, r) = vm_with(
        "export var x = 2\nexport var n = [x, x + 1]\nexport var h = [x, n]\n",
        usize::MAX,
    );
    r.expect("init");
    let Value::Arr(n) = global(&p, &vm, "n") else {
        panic!()
    };
    let Value::Arr(h) = global(&p, &vm, "h") else {
        panic!()
    };
    assert!(matches!(vm.arrays[n as usize], ArrRepr::Num(_)));
    assert!(matches!(vm.arrays[h as usize], ArrRepr::Owned(_)));
    assert_eq!(vm.arr(&p, h).at(1), Value::Arr(n));
    assert_eq!(vm.arr(&p, n).at(1), Value::Num(Fx::from_int(3)));
}
