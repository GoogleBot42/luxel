//! Call inlining equivalence (Gitea #943).
//!
//! A call from a `render*` entry to a small user function is expanded in
//! place. That may not change a single rendered pixel, the error a frame
//! raises, or where in the SOURCE it is reported — only the function index
//! an inlined body's error is attributed to (the caller's) is allowed to
//! move. `CompileOpts { inline: false, .. }` is the A/B lever (`luxel
//! compile --no-inline`); every test here compares the two.

use luxel_core::bytecode::{enc, op};
use luxel_core::compile::{compile_with, CompileOpts};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::vm::Program;

const OFF: CompileOpts = CompileOpts {
    superinstructions: true,
    const_folding: true,
    store_forwarding: true,
    kinds: true,
    inline: false,
};

/// One source per expansion rule, and the shapes that must stay calls.
const CASES: &[&str] = &[
    // arguments evaluated in order, with side effects on both sides of
    // the call and inside it
    "var arr = array(8)
     var hits = 0
     function bump(i, k) { hits = hits + 1; arr[i % 8] = arr[i % 8] + k; return arr[i % 8] }
     export function render(index) {
       var i = index
       var s = arr[i % 8] + bump(i++, i) + bump(i, i++)
       hsv(s / 64 + hits / 1000, 1, 1)
     }",
    // missing arguments read 0, extra ones are evaluated and dropped
    "var n = 0
     function two(a, b) { return a * 3 + b }
     function side() { n = n + 1; return n }
     export function render(index) {
       hsv(two(index) / 100 + two(index, 2, side()) / 100 + n / 4096, 1, 1)
     }",
    // early return from inside a loop, return with no value, falling off
    // the end — the last two are both 0
    "function firstOver(lim) {
       for (var k = 0; k < 20; k++) { if (k * k > lim) return k }
       return
     }
     function nothing(x) { var y = x * 2 }
     export function render(index) {
       hsv(firstOver(index) / 20 + nothing(index) + firstOver(1000), 1, 1)
     }",
    // a global the callee reads that the CALLER shadows with a local, and a
    // callee local named like a caller local
    "var t = 0.25
     function useT(x) { var i = x + 1; return t + i * 0 }
     export function beforeRender(d) { t = t + d / 10000 }
     export function render(index) {
       var t = 5
       var i = 3
       hsv(useT(index) + i / 100 + t / 100, 1, 1)
     }",
    // nested sites of one callee, a callee that calls another user
    // function (that call stays a call) and a recursive one
    "function sq(x) { return x * x }
     function twice(x) { return sq(x) + sq(x) }
     function fact(n) { if (n <= 1) return 1; return n * fact(n - 1) }
     export function render(index) {
       hsv(sq(sq(index / 64)) + twice(index / 128) + fact(index % 5) / 200, 1, 1)
     }",
    // `break`/`continue` inside the callee's own loops, while the CALLER
    // is inside a loop too; const locals; a var re-declared in a loop
    "function sumTo(n) {
       const step = 1
       var s = 0
       for (var k = 0; k < 100; k = k + step) {
         var j = k
         if (j == n) break
         if (j % 2) continue
         s = s + j
       }
       return s
     }
     export function render(index) {
       var acc = 0
       for (var r = 0; r < 3; r++) { acc = acc + sumTo(index % 7 + r); if (r == 1) break }
       hsv(acc / 50, 1, 1)
     }",
    // render2D and render3D callers, a callee writing the brush
    "function paint(v) { if (v > 0.5) { rgb(v, 0, 1 - v) } else { hsv(v, 1, 1) } }
     export function render2D(index, x, y) { paint(x * y + 0.2) }
     export function render3D(index, x, y, z) { paint(x + y + z) }",
    // the shapes that must NOT expand: a switch, a lambda, a nested
    // function — still correct, still calls
    "function sw(k) { switch (k % 3) { case 0: return 0.1; case 1: return 0.5; default: return 0.9 } }
     function lam(a) { var f = (q) => q * 2; return f(a) }
     export function render(index) { hsv(sw(index) + lam(index) / 100, 1, 1) }",
    // two expansions of one callee in one call share its slots: a local
    // read before it is written must still start at 0 the second time
    "function acc(x) { var s; s = s + x; return s }
     export function render(index) { hsv((acc(index) + acc(1)) / 64, 1, 1) }",
    // a runtime error inside an inlined body: same message, same source
    // position, and the brush the PB blast radius keeps
    "var a = [1, 2]
     function bad(i) { hsv(0.3, 1, 1)\n return a[i + 5] }
     export function render(index) { hsv(bad(index), 1, 1) }",
];

/// `(frames, errors as (message, line, col))`, n frames on a strip.
fn run(src: &str, opts: CompileOpts, pixels: u32, grid: Option<(u16, u16)>, n: usize) -> (Vec<Vec<[u8; 3]>>, Vec<Option<(String, u32, u32)>>) {
    let prog = compile_with(src, opts).unwrap_or_else(|d| panic!("compile: {}", d.message));
    let mut e = Engine::from_program(prog, pixels, 7);
    if let Some((w, h)) = grid {
        e.set_grid_map(w, h);
    }
    e.take_error();
    let mut frames = Vec::new();
    let mut errs = Vec::new();
    for _ in 0..n {
        frames.push(e.frame(Fx::from_int(16)).to_vec());
        errs.push(e.take_error().map(|x| (x.message, x.line, x.col)));
    }
    (frames, errs)
}

fn same(src: &str, what: &str) {
    for (pixels, grid) in [(32u32, None), (64, Some((8u16, 8u16)))] {
        let on = run(src, CompileOpts::default(), pixels, grid, 6);
        let off = run(src, OFF, pixels, grid, 6);
        assert_eq!(on.0, off.0, "{what}: frames differ ({pixels} px, grid {grid:?})");
        assert_eq!(on.1, off.1, "{what}: errors differ ({pixels} px, grid {grid:?})");
    }
}

/// `CallFn`s in the exported function `name`.
fn calls_in(prog: &Program, name: &str) -> usize {
    let Some(&(_, fi)) = prog.exported_fns.iter().find(|(n, _)| n == name) else {
        return 0;
    };
    let f = &prog.fns[fi as usize];
    let code = &prog.words[f.code_start as usize..(f.code_start + f.code_len) as usize];
    let mut at = 0;
    let mut n = 0;
    while at < code.len() {
        let o = enc::opcode(code[at]);
        if o == op::CALL_FN {
            n += 1;
        }
        at += luxel_core::kinds::ilen(o);
    }
    n
}

#[test]
fn inlined_and_called_render_identically() {
    for (i, src) in CASES.iter().enumerate() {
        same(src, &format!("case {i}"));
    }
}

#[test]
fn the_expansion_fires_and_the_refusals_hold() {
    let on = |src: &str| compile_with(src, CompileOpts::default()).unwrap();
    let off = |src: &str| compile_with(src, OFF).unwrap();
    // case 1: both sites of `two` and the `side()` argument expand
    assert_eq!(calls_in(&on(CASES[1]), "render"), 0);
    assert_eq!(calls_in(&off(CASES[1]), "render"), 3);
    // case 4: all four sites in render expand (the nested `sq(sq())`
    // both levels, being two sites); what the expanded bodies call stays
    // a call — `twice`'s two `sq`s and `fact`'s recursion: one level only
    assert_eq!(calls_in(&off(CASES[4]), "render"), 4);
    assert_eq!(calls_in(&on(CASES[4]), "render"), 3);
    // case 7: switch and lambda bodies are refused — two calls remain
    assert_eq!(calls_in(&on(CASES[7]), "render"), 2);
    // only render* entries expand: beforeRender keeps its call
    let src = "var v = 0
               function f(x) { return x + 1 }
               export function beforeRender(d) { v = f(v) }
               export function render(index) { hsv(f(v) / 100, 1, 1) }";
    assert_eq!(calls_in(&on(src), "beforeRender"), 1);
    assert_eq!(calls_in(&on(src), "render"), 0);
}

#[test]
fn an_error_inside_an_inlined_body_keeps_its_source_position() {
    let src = CASES[9];
    let (_, errs) = run(src, CompileOpts::default(), 8, None, 1);
    let (msg, line, _) = errs[0].clone().expect("the out-of-bounds read raises");
    assert!(msg.contains("out of bounds"), "{msg}");
    assert_eq!(line, 3, "attributed to the callee's `return` line");
}

fn library() -> Vec<(String, String)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../library");
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .expect("library/ must exist")
        .filter_map(|e| {
            let p = e.ok()?.path();
            if p.extension()? != "js" {
                return None;
            }
            let name = p.file_name()?.to_string_lossy().to_string();
            Some((name, std::fs::read_to_string(&p).ok()?))
        })
        .collect();
    out.sort();
    out
}

/// The whole library, inlined against not: 6 frames on a 64-px strip and
/// on an 8x8 grid. A wrong rewrite shows here and NOWHERE else — the
/// interpreter-vs-JIT diffs compare two runs of the same bytecode.
#[test]
fn the_library_renders_identically_inlined() {
    let mut expanded = 0usize;
    for (name, src) in library() {
        let Ok(on) = compile_with(&src, CompileOpts::default()) else {
            continue; // a pattern the compiler refuses is refused either way
        };
        let off = compile_with(&src, OFF).expect("compiles with inlining off too");
        let ons = ["render", "render2D", "render3D"].iter().map(|n| calls_in(&on, n)).sum::<usize>();
        let offs = ["render", "render2D", "render3D"].iter().map(|n| calls_in(&off, n)).sum::<usize>();
        if ons < offs {
            expanded += 1;
        }
        same(&src, &name);
    }
    assert!(expanded >= 20, "only {expanded} library patterns expand a call");
}
