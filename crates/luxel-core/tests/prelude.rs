//! The pattern-language prelude (Gitea #626, docs/jit-design.md §4).
//!
//! `arrayForEach`, `arrayMutate`, `arrayMapTo`, `arrayReduce`, `arraySortBy`
//! and `mapPixels` used to be builtins that re-entered the interpreter
//! through `Vm::dispatch_direct`. They are pattern functions now, linked in
//! only when used.
//!
//! The gate is GOLDEN DATA recorded from the builtin arms before they were
//! deleted: `tests/data/prelude-golden.txt` holds one line per case,
//! produced by compiling each case with the prelude linker OFF (so the six
//! names fell through to the builtins) on the commit that still had them.
//! Every case is re-run here through the prelude and must reproduce its line
//! exactly. Regenerate — which you cannot do any more, the arms are gone —
//! with `LUXEL_WRITE_GOLDEN=1 cargo test -p luxel-core --test prelude`.

use luxel_core::compile::compile;
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/prelude-golden.txt");

/// One case: a name and a pattern. Every case ends in a `render` that reads
/// the helper's result back out, so the recorded pixels ARE the result.
///
/// `#PX n` on the first line sets the pixel count, `#MAP2` installs a 4×4
/// grid map (for `mapPixels`). Both default to 16 pixels and no map.
fn cases() -> Vec<(&'static str, &'static str)> {
    vec![
        // ---- arrayForEach: visits every element in order, returns the array
        (
            "forEach-order-and-return",
            "var a = array(5)\n\
             var log = array(5)\n\
             var k = 0\n\
             var i = 0\n\
             while (i < 5) { a[i] = (i + 1) / 8; i++ }\n\
             var ret = arrayForEach(a, (v, idx, arr) => { log[k] = v * 4 + idx / 16 + arr.length / 64; k++ })\n\
             export function render(i) { hsv(log[i % 5] + ret[i % 5] / 2 + k / 32, 1, 1) }\n",
        ),
        (
            "forEach-empty",
            "var a = array(0)\n\
             var k = 0\n\
             var ret = arrayForEach(a, (v, i, arr) => { k++ })\n\
             export function render(i) { hsv(k + ret.length / 2, 1, 1) }\n",
        ),
        // ---- arrayMutate: writes the callback's return back, returns the array
        (
            "mutate-writes-and-returns",
            "var a = array(6)\n\
             var i = 0\n\
             while (i < 6) { a[i] = i / 6; i++ }\n\
             var ret = arrayMutate(a, (v, idx, arr) => v * 2 + idx / 32)\n\
             export function render(i) { hsv(a[i % 6] + ret[(i + 1) % 6] / 4, 1, 1) }\n",
        ),
        (
            "mutate-empty",
            "var a = array(0)\n\
             var ret = arrayMutate(a, (v) => 1)\n\
             export function render(i) { hsv(ret.length / 2 + a.length, 1, 1) }\n",
        ),
        (
            "mutate-named-callback",
            "var a = array(4)\n\
             function twice(v, i, arr) { return v * 2 + i / 8 }\n\
             var i = 0\n\
             while (i < 4) { a[i] = i / 4; i++ }\n\
             arrayMutate(a, twice)\n\
             export function render(i) { hsv(a[i % 4], 1, 1) }\n",
        ),
        (
            "mutate-method-form",
            "var a = array(4)\n\
             var i = 0\n\
             while (i < 4) { a[i] = i / 4; i++ }\n\
             a.mutate((v) => v + 0.125)\n\
             export function render(i) { hsv(a[i % 4], 1, 1) }\n",
        ),
        (
            "mutate-runtime-callback-value",
            // the callback is a GLOBAL holding a function value: the call
            // site cannot specialise, so this exercises the unspecialised
            // helper's `CallValue`
            "var a = array(4)\n\
             var f = (v, i) => v + i / 8\n\
             var i = 0\n\
             while (i < 4) { a[i] = i / 4; i++ }\n\
             arrayMutate(a, f)\n\
             export function render(i) { hsv(a[i % 4], 1, 1) }\n",
        ),
        // ---- arrayMapTo: shorter length wins, callback sees SRC, returns DST
        (
            "mapTo-equal-lengths",
            "var s = array(4)\n\
             var d = array(4)\n\
             var i = 0\n\
             while (i < 4) { s[i] = i / 4; i++ }\n\
             var ret = arrayMapTo(s, d, (v, idx, arr) => v + idx / 16 + arr.length / 64)\n\
             export function render(i) { hsv(d[i % 4] + ret[(i + 1) % 4] / 8, 1, 1) }\n",
        ),
        (
            "mapTo-src-longer",
            "var s = array(6)\n\
             var d = array(3)\n\
             var i = 0\n\
             while (i < 6) { s[i] = i / 6; i++ }\n\
             arrayMapTo(s, d, (v, idx) => v + idx / 16)\n\
             export function render(i) { hsv(d[i % 3], 1, 1) }\n",
        ),
        (
            "mapTo-dst-longer",
            "var s = array(2)\n\
             var d = array(5)\n\
             var i = 0\n\
             while (i < 5) { d[i] = 0.75; i++ }\n\
             s[0] = 0.25\n\
             s[1] = 0.5\n\
             arrayMapTo(s, d, (v, idx) => v + idx / 16)\n\
             export function render(i) { hsv(d[i % 5], 1, 1) }\n",
        ),
        (
            "mapTo-empty-src",
            "var s = array(0)\n\
             var d = array(3)\n\
             var ret = arrayMapTo(s, d, (v) => 1)\n\
             export function render(i) { hsv(d[i % 3] + ret.length / 8, 1, 1) }\n",
        ),
        // ---- arrayReduce: (acc, v, i, arr), returns the accumulator
        (
            "reduce-with-init",
            "var a = array(5)\n\
             var i = 0\n\
             while (i < 5) { a[i] = i / 16; i++ }\n\
             var sum = arrayReduce(a, (acc, v, idx, arr) => acc + v + idx / 128, 0.25)\n\
             export function render(i) { hsv(sum, 1, 1) }\n",
        ),
        (
            "reduce-without-init",
            "var a = array(4)\n\
             var i = 0\n\
             while (i < 4) { a[i] = i / 16; i++ }\n\
             var sum = arrayReduce(a, (acc, v) => acc + v)\n\
             export function render(i) { hsv(sum, 1, 1) }\n",
        ),
        (
            "reduce-empty-returns-init",
            "var a = array(0)\n\
             var sum = arrayReduce(a, (acc, v) => acc + 99, 0.375)\n\
             export function render(i) { hsv(sum, 1, 1) }\n",
        ),
        // ---- arraySortBy: insertion sort, `cmp(x, y) > 0` means x after y
        (
            "sortBy-descending",
            "var a = array(6)\n\
             a[0] = 0.5\n a[1] = 0.125\n a[2] = 0.875\n a[3] = 0.25\n a[4] = 0.75\n a[5] = 0.375\n\
             var ret = arraySortBy(a, (x, y) => y - x)\n\
             export function render(i) { hsv(a[i % 6] + ret[(i + 1) % 6] / 16, 1, 1) }\n",
        ),
        (
            "sortBy-equal-keys-keep-order",
            // every comparison is 0, so nothing may move: a stable sort
            // leaves the array exactly as it was
            "var a = array(5)\n\
             a[0] = 0.5\n a[1] = 0.125\n a[2] = 0.875\n a[3] = 0.25\n a[4] = 0.75\n\
             arraySortBy(a, (x, y) => 0)\n\
             export function render(i) { hsv(a[i % 5], 1, 1) }\n",
        ),
        (
            "sortBy-already-sorted",
            "var a = array(4)\n\
             a[0] = 0.125\n a[1] = 0.25\n a[2] = 0.5\n a[3] = 0.75\n\
             arraySortBy(a, (x, y) => x - y)\n\
             export function render(i) { hsv(a[i % 4], 1, 1) }\n",
        ),
        (
            "sortBy-single-and-empty",
            "var a = array(1)\n\
             var b = array(0)\n\
             a[0] = 0.625\n\
             arraySortBy(a, (x, y) => x - y)\n\
             arraySortBy(b, (x, y) => x - y)\n\
             export function render(i) { hsv(a[0] + b.length, 1, 1) }\n",
        ),
        // ---- mapPixels: (index, x, y, z) with the transform applied
        (
            "mapPixels-1d-no-map",
            "#PX 8\n\
             var xs = array(8)\n\
             var ys = array(8)\n\
             function scan(idx, x, y, z) { xs[idx] = x; ys[idx] = y + z / 2 }\n\
             export function beforeRender(delta) { mapPixels(scan) }\n\
             export function render(i) { hsv(xs[i] / 2 + ys[i] / 4, 1, 1) }\n",
        ),
        (
            "mapPixels-2d-map",
            "#PX 16\n#MAP2\n\
             var xs = array(16)\n\
             var ys = array(16)\n\
             export function beforeRender(delta) { mapPixels((idx, x, y, z) => { xs[idx] = x; ys[idx] = y + z }) }\n\
             export function render(i) { hsv(xs[i] / 2 + ys[i] / 4, 1, 1) }\n",
        ),
        (
            "mapPixels-2d-map-with-transform",
            "#PX 16\n#MAP2\n\
             var xs = array(16)\n\
             var ys = array(16)\n\
             export function beforeRender(delta) {\n\
               resetTransform()\n\
               translate(-0.5, -0.5)\n\
               rotate(0.125)\n\
               scale(2, 0.5)\n\
               mapPixels((idx, x, y, z) => { xs[idx] = x; ys[idx] = y + z })\n\
             }\n\
             export function render(i) { hsv(xs[i] / 4 + ys[i] / 8, 1, 1) }\n",
        ),
        (
            "mapPixels-transform-reset-midway",
            // the transform is read per pixel, so a callback that clears it
            // changes the coordinates the REST of the pass sees
            "#PX 8\n\
             var xs = array(8)\n\
             export function beforeRender(delta) {\n\
               resetTransform()\n\
               translate(0.25, 0)\n\
               mapPixels((idx, x, y, z) => { xs[idx] = x; if (idx == 3) { resetTransform() } })\n\
             }\n\
             export function render(i) { hsv(xs[i], 1, 1) }\n",
        ),
        // ---- index and coercion behaviour
        (
            "index-is-the-loop-counter-not-a-value",
            // the callback's second argument is a plain integer index
            "var a = array(4)\n\
             var acc = 0\n\
             arrayForEach(a, (v, idx) => { acc = acc + idx * idx })\n\
             export function render(i) { hsv(acc / 32, 1, 1) }\n",
        ),
        (
            "mutate-callback-returning-nothing-writes-zero",
            "var a = array(4)\n\
             var i = 0\n\
             while (i < 4) { a[i] = 0.5; i++ }\n\
             arrayMutate(a, (v) => { var q = v })\n\
             export function render(i) { hsv(a[i % 4] + 0.25, 1, 1) }\n",
        ),
        (
            "nested-helpers",
            "var a = array(3)\n\
             var b = array(3)\n\
             var i = 0\n\
             while (i < 3) { a[i] = (i + 1) / 8; i++ }\n\
             arrayMutate(b, (v, idx) => arrayReduce(a, (acc, x) => acc + x, idx / 8))\n\
             export function render(i) { hsv(b[i % 3], 1, 1) }\n",
        ),
        // ---- error cases: a non-array where an array was expected
        ("error-forEach-non-array", "arrayForEach(5, (v) => v)\nexport function render(i) { hsv(i, 1, 1) }\n"),
        ("error-mutate-non-array", "arrayMutate(5, (v) => v)\nexport function render(i) { hsv(i, 1, 1) }\n"),
        (
            "error-mapTo-non-array-dst",
            "var s = array(3)\narrayMapTo(s, 7, (v) => v)\nexport function render(i) { hsv(i, 1, 1) }\n",
        ),
        (
            "error-mapTo-non-array-dst-empty-src",
            "var s = array(0)\narrayMapTo(s, 7, (v) => v)\nexport function render(i) { hsv(i, 1, 1) }\n",
        ),
        ("error-reduce-non-array", "arrayReduce(5, (a, v) => v, 0)\nexport function render(i) { hsv(i, 1, 1) }\n"),
        ("error-sortBy-non-array", "arraySortBy(5, (x, y) => x - y)\nexport function render(i) { hsv(i, 1, 1) }\n"),
        (
            "error-callback-is-not-a-function",
            "var a = array(3)\nvar f = 7\narrayMutate(a, f)\nexport function render(i) { hsv(i, 1, 1) }\n",
        ),
        (
            "error-inside-the-callback",
            "var a = array(3)\nvar b = array(1)\narrayMutate(a, (v, i) => b[i])\nexport function render(i) { hsv(i, 1, 1) }\n",
        ),
    ]
}

/// Run one case and describe everything a pattern can observe about it: the
/// init-time error (if any) and two rendered frames.
fn record(name: &str, src: &str) -> String {
    let (px, map2, body) = directives(src);
    let built = if std::env::var("LUXEL_GOLDEN_BUILTINS").is_ok() {
        // Recording the goldens: compile with the prelude linker off so the
        // six names fall through to the builtin arms (Gitea #626 commit 1).
        luxel_core::compile::compile_without_prelude(body)
    } else {
        compile(body)
    };
    let prog = match built {
        Ok(p) => p,
        Err(d) => return format!("{name}\tCOMPILE-ERROR\t{}", d.message),
    };
    let mut e = Engine::from_program_budgeted_at(prog, px, 7, usize::MAX, Some(1_700_000_000));
    if map2 {
        let coords: Vec<[Fx; 3]> = (0..px as usize)
            .map(|i| {
                let w = 4usize;
                let (c, r) = (i % w, i / w);
                [
                    Fx::from_int(c as i32) / Fx::from_int(3),
                    Fx::from_int(r as i32) / Fx::from_int(3),
                    Fx::ZERO,
                ]
            })
            .collect();
        e.set_map(2, &coords);
    }
    let err = match e.take_error() {
        Some(v) => format!("{} @{}:{}", v.message, v.line, v.col),
        None => "-".to_string(),
    };
    let mut out = format!("{name}\t{err}");
    for _ in 0..2 {
        let f = e.frame(Fx::from_int(16));
        out.push('\t');
        for p in f {
            out.push_str(&format!("{:02x}{:02x}{:02x}", p[0], p[1], p[2]));
        }
    }
    out
}

/// `#PX n` / `#MAP2` on the leading lines of a case source.
fn directives(src: &str) -> (u32, bool, &str) {
    let (mut px, mut map2) = (16u32, false);
    let mut rest = src;
    loop {
        let line = rest.lines().next().unwrap_or("").trim();
        if let Some(n) = line.strip_prefix("#PX ") {
            px = n.trim().parse().expect("#PX takes a number");
        } else if line == "#MAP2" {
            map2 = true;
        } else {
            return (px, map2, rest);
        }
        rest = &rest[rest.find('\n').map(|i| i + 1).unwrap_or(rest.len())..];
    }
}

/// The cases whose ERROR TEXT deliberately changed with #626, and what the
/// prelude says instead.
///
/// The builtins raised messages of their own ("array method on a non-array")
/// because they checked their arguments before doing anything. A prelude
/// helper is ordinary pattern code, so a non-array argument fails the way
/// the pattern language fails anywhere else — at the first `.length` or the
/// first call. Every PIXEL is unchanged, and so is the reported position
/// (the specialised clone is attributed to its call site), with one
/// exception noted below.
fn error_text_changed(name: &str) -> Option<&'static str> {
    Some(match name {
        // a non-array reaches `.length` first, in all five helpers
        "error-forEach-non-array"
        | "error-mutate-non-array"
        | "error-mapTo-non-array-dst"
        | "error-mapTo-non-array-dst-empty-src"
        | "error-reduce-non-array"
        | "error-sortBy-non-array" => ".length of a non-array value",
        // the callback is a run-time value here, so the call site goes
        // through the SHARED helper copy, which belongs to no one call site
        // and therefore reports position (0, 0) rather than the builtin's
        // (3, 1). The message is the language's own `CallValue` error.
        "error-callback-is-not-a-function" => "call of a non-function value",
        _ => return None,
    })
}

#[test]
fn the_prelude_reproduces_the_builtins_golden_output() {
    let mut lines: Vec<String> = Vec::new();
    for (name, src) in cases() {
        lines.push(record(name, src));
    }
    let produced = lines.join("\n") + "\n";
    if std::env::var("LUXEL_WRITE_GOLDEN").is_ok() {
        std::fs::create_dir_all(std::path::Path::new(GOLDEN).parent().unwrap()).unwrap();
        std::fs::write(GOLDEN, &produced).unwrap();
        return;
    }
    let want = std::fs::read_to_string(GOLDEN).expect("tests/data/prelude-golden.txt must exist");
    assert_eq!(
        produced.lines().count(),
        want.lines().count(),
        "the golden file has a different number of cases"
    );
    for (got, gold) in produced.lines().zip(want.lines()) {
        let g: Vec<&str> = got.split('\t').collect();
        let w: Vec<&str> = gold.split('\t').collect();
        assert_eq!(g[0], w[0], "cases are out of order");
        let name = g[0];
        match error_text_changed(name) {
            None => assert_eq!(
                g[1], w[1],
                "{name}: error text changed but is not in `error_text_changed`"
            ),
            Some(now) => {
                assert!(
                    g[1].starts_with(now),
                    "{name}: expected the documented new error `{now}`, got `{}`",
                    g[1]
                );
                assert_ne!(g[1], w[1], "{name}: the error no longer differs — drop the entry");
            }
        }
        assert_eq!(
            &g[2..],
            &w[2..],
            "{name}: the prelude renders different pixels from the builtin"
        );
    }
}

/// A helper the pattern never mentions is not in the blob, and one it does
/// mention is — the tree-shaking half of #626.
#[test]
fn only_the_helpers_a_pattern_uses_are_linked() {
    let bare = compile("export function render(i) { hsv(i, 1, 1) }\n").unwrap();
    assert!(
        !bare.fns.iter().any(|f| f.name.starts_with("array") || f.name.starts_with("mapPixels")),
        "an unused prelude must not be linked"
    );
    let used = compile(
        "var a = array(4)\n\
         arrayMutate(a, (v) => 1)\n\
         export function render(i) { hsv(a[i % 4], 1, 1) }\n",
    )
    .unwrap();
    assert!(
        used.fns.iter().any(|f| f.name.starts_with("arrayMutate$")),
        "the call site should have been specialised: {:?}",
        used.fns.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
    assert!(
        !used.fns.iter().any(|f| f.name == "arrayForEach"),
        "arrayMutate must not drag in the rest of the prelude"
    );
}

/// A user function of the same name wins, exactly as it won over the
/// builtin — and then no prelude copy is linked at all.
#[test]
fn a_user_function_shadows_the_prelude() {
    let prog = compile(
        "var hits = 0\n\
         function arrayMutate(a, f) { hits = 42 }\n\
         var a = array(4)\n\
         arrayMutate(a, (v) => 1)\n\
         export function render(i) { hsv(hits / 64, 1, 1) }\n",
    )
    .unwrap();
    let mut e = Engine::from_program_budgeted_at(prog, 4, 7, usize::MAX, Some(1_700_000_000));
    assert!(e.take_error().is_none());
    let f = e.frame(Fx::from_int(16));
    // hits == 42 ⇒ hue 42/64; the prelude copy would have left it 0
    let mut control =
        Engine::from_program_budgeted_at(
            compile("export function render(i) { hsv(42 / 64, 1, 1) }\n").unwrap(),
            4,
            7,
            usize::MAX,
            Some(1_700_000_000),
        );
    assert_eq!(f, control.frame(Fx::from_int(16)));
}

/// The prelude's own locals are locals: nothing it uses leaks into the
/// pattern's global namespace.
#[test]
fn the_prelude_leaks_no_globals() {
    let plain = compile("export function render(i) { hsv(i, 1, 1) }\n").unwrap();
    let with_prelude = compile(
        "var a = array(4)\n\
         arraySortBy(a, (x, y) => x - y)\n\
         arrayMapTo(a, a, (v) => v)\n\
         mapPixels((idx, x, y, z) => { a[0] = x })\n\
         export function render(i) { hsv(a[i % 4], 1, 1) }\n",
    )
    .unwrap();
    let extra: Vec<&str> = with_prelude
        .globals
        .iter()
        .map(|g| g.name.as_str())
        .filter(|n| !plain.globals.iter().any(|g| g.name == *n) && *n != "a")
        .collect();
    assert!(extra.is_empty(), "the prelude leaked globals: {extra:?}");
}

/// A callback that is only a run-time value goes through the unspecialised
/// helper — correct, just boxed — and renders the same pixels as the
/// specialised form of the same computation.
#[test]
fn a_runtime_callback_value_still_works() {
    let dynamic = "var a = array(4)\n\
                   var f = (v, i) => (i + 1) / 8\n\
                   var g = (v, i) => (i + 1) / 8\n\
                   var pick = 1\n\
                   arrayMutate(a, pick > 0 ? f : g)\n\
                   export function render(i) { hsv(a[i % 4], 1, 1) }\n";
    let stat = "var a = array(4)\n\
                arrayMutate(a, (v, i) => (i + 1) / 8)\n\
                export function render(i) { hsv(a[i % 4], 1, 1) }\n";
    let frames = |src: &str| {
        let mut e = Engine::from_program_budgeted_at(
            compile(src).unwrap(),
            8,
            7,
            usize::MAX,
            Some(1_700_000_000),
        );
        e.take_error();
        e.frame(Fx::from_int(16)).to_vec()
    };
    assert_eq!(frames(dynamic), frames(stat));
}

// ------------------------------------------------- the library-wide gate

fn library() -> Vec<(String, String)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../library");
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .expect("library/ must exist")
        .filter_map(|e| {
            let p = e.ok()?.path();
            if p.extension()? != "js" {
                return None;
            }
            Some((
                p.file_name()?.to_string_lossy().to_string(),
                std::fs::read_to_string(&p).ok()?,
            ))
        })
        .collect();
    out.sort();
    out
}

/// The library-render harness `tests/kinds.rs` uses, hashed: 60 pixels,
/// seed 7, fixed wall clock, two frames.
fn library_frames_hash(prog: luxel_core::vm::Program) -> u64 {
    let mut e = Engine::from_program_budgeted_at(prog, 60, 7, usize::MAX, Some(1_700_000_000));
    e.take_error();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for _ in 0..2 {
        for p in e.frame(Fx::from_int(16)) {
            for b in p {
                h = (h ^ *b as u64).wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    h
}

const LIB_GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/library-callbacks.txt");

/// Every `library/` pattern that uses a prelude helper, pinned to the pixels
/// the BUILTINS produced (Gitea #626). These eight are the only patterns the
/// change can reach, and the file was written from the builtin arms before
/// they were deleted.
#[test]
fn the_callback_patterns_render_as_the_builtins_did() {
    let helpers = [
        "arrayForEach",
        "arrayMutate",
        "arrayMapTo",
        "arrayReduce",
        "arraySortBy",
        "mapPixels",
    ];
    let mut lines: Vec<String> = Vec::new();
    for (name, src) in library() {
        if !helpers.iter().any(|h| src.contains(h)) {
            continue;
        }
        let prog = match std::env::var("LUXEL_GOLDEN_BUILTINS") {
            Ok(_) => luxel_core::compile::compile_without_prelude(&src),
            Err(_) => compile(&src),
        }
        .unwrap_or_else(|d| panic!("{name}: {}", d.message));
        lines.push(format!("{name}\t{:016x}", library_frames_hash(prog)));
    }
    assert!(lines.len() >= 8, "expected the callback patterns, got {lines:?}");
    let produced = lines.join("\n") + "\n";
    if std::env::var("LUXEL_WRITE_GOLDEN").is_ok() {
        std::fs::write(LIB_GOLDEN, &produced).unwrap();
        return;
    }
    let want = std::fs::read_to_string(LIB_GOLDEN).expect("tests/data/library-callbacks.txt");
    for (a, b) in produced.lines().zip(want.lines()) {
        assert_eq!(a, b, "a callback pattern renders differently from the builtins");
    }
    assert_eq!(produced, want);
}

/// The whole library, A/B: the prelude against the builtins it replaces,
/// pattern by pattern, frame by frame. This test goes away with the arms —
/// `the_callback_patterns_render_as_the_builtins_did` is the golden that
/// outlives it.
#[test]
fn the_whole_library_renders_identically_without_the_prelude() {
    for (name, src) in library() {
        let with = compile(&src).unwrap_or_else(|d| panic!("{name}: {}", d.message));
        let without = luxel_core::compile::compile_without_prelude(&src)
            .unwrap_or_else(|d| panic!("{name}: {}", d.message));
        assert_eq!(
            library_frames_hash(with),
            library_frames_hash(without),
            "{name}: the prelude renders differently from the builtins"
        );
    }
}
