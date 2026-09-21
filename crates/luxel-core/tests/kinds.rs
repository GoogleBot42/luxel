//! LXBC v6 kinds: the lattice, the stack-map verifier, the container, and
//! the library-wide census the JIT design is sized against (Gitea #607,
//! docs/jit-design.md §2 and §9).

use luxel_core::bytecode::{deserialize, serialize};
use luxel_core::compile::{compile, compile_with, CompileOpts};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::kinds::{self, Kind};
use luxel_core::vm::Program;
use std::collections::BTreeSet;

const NO_KINDS: CompileOpts = CompileOpts {
    superinstructions: true,
    const_folding: true,
    store_forwarding: true,
    kinds: false,
};

// ---------------------------------------------------------------- lattice

#[test]
fn lattice_order_and_join() {
    use Kind::*;
    for k in [Dyn, Num, Arr, ArrNum, Fun, Builtin] {
        assert!(k.le(k));
        assert!(k.le(Dyn), "{k:?} must be below Dyn");
        assert_eq!(k.join(Dyn), Dyn);
        assert_eq!(k.join(k), k);
        assert_eq!(Kind::from_byte(k.as_byte()), Some(k));
    }
    assert!(ArrNum.le(Arr));
    assert!(!Arr.le(ArrNum));
    assert_eq!(ArrNum.join(Arr), Arr);
    // incomparable pairs go straight to the top
    assert_eq!(Num.join(Arr), Dyn);
    assert_eq!(Fun.join(Builtin), Dyn);
    assert_eq!(Num.join(ArrNum), Dyn);
    assert_eq!(Kind::from_byte(6), None);
}

// ------------------------------------------------------------- inference

fn kinds_of(src: &str) -> (Program, kinds::Kinds) {
    let prog = compile(src).expect("compiles");
    let k = kinds::infer(&prog);
    kinds::verify(&prog, &k).expect("compiler output must verify");
    assert_eq!(
        prog.kinds.as_ref(),
        Some(&k),
        "the compiler must attach exactly what infer() computes"
    );
    (prog, k)
}

fn global_kind(prog: &Program, k: &kinds::Kinds, name: &str) -> Kind {
    let g = prog.global_index(name).unwrap_or_else(|| panic!("no global {name}"));
    k.global(g as usize)
}

#[test]
fn numeric_slots_are_num_and_arrays_are_arrnum() {
    let (p, k) = kinds_of(
        "var buf = array(8)\n\
         var t = 0\n\
         export function beforeRender(delta) { t = delta / 1000 }\n\
         export function render(i) { hsv(t + buf[i % 8], 1, 1) }\n",
    );
    assert_eq!(global_kind(&p, &k, "t"), Kind::Num);
    assert_eq!(global_kind(&p, &k, "buf"), Kind::ArrNum);
    let f = p.exported_fn("render").unwrap() as usize;
    assert_eq!(k.slot(f, 0), Kind::Num, "render's index param");
}

#[test]
fn the_init_value_is_dropped_when_init_definitely_assigns() {
    // The §2.3 exemption: every global's DECLARED init is an `Fx`, so it
    // is a `Num` store. Folding it in unconditionally would make every
    // array global `Dyn`; the exemption drops it when the init function
    // definitely assigns the global before anything can read it.
    let (p, k) = kinds_of(
        "var hues = array(8)\n\
         export function render(i) { hsv(hues[i % 8], 1, 1) }\n",
    );
    assert_eq!(global_kind(&p, &k, "hues"), Kind::ArrNum);

    // …but an EXPLICIT `var hues = 0` emits a real `StoreG` of `Num`,
    // which is a second kind the slot genuinely holds. The exemption is
    // about the declared init only — the verifier sees that store and
    // would reject an `ArrNum` annotation, so the inference must not make
    // one. (docs/jit-design.md §2.3 reads the `var hues = 0` case as an
    // init value; in this compiler it is a store.)
    let (p2, k2) = kinds_of(
        "var hues = 0\n\
         hues = array(8)\n\
         export function render(i) { hsv(hues[i % 8], 1, 1) }\n",
    );
    assert_eq!(global_kind(&p2, &k2, "hues"), Kind::Dyn);

    // A call before the store only matters if the callee can READ the
    // global: init functions routinely call helpers between array setups.
    let (p3, k3) = kinds_of(
        "function unrelated() { return 1 }\n\
         var x = unrelated()\n\
         var hues = array(8)\n\
         export function render(i) { hsv(hues[i % 8] + x, 1, 1) }\n",
    );
    assert_eq!(global_kind(&p3, &k3, "hues"), Kind::ArrNum);
}

#[test]
fn a_global_that_really_holds_two_kinds_stays_dyn() {
    // fire-blue.js's shape: `heat = 0` at top level, `heat = array(n)`
    // only once beforeRender has run. The init value IS observable.
    let (p, k) = kinds_of(
        "var heat = 0\n\
         export function beforeRender(delta) { if (!heat) heat = array(8) }\n\
         export function render(i) { hsv(heat[i % 8], 1, 1) }\n",
    );
    assert_eq!(global_kind(&p, &k, "heat"), Kind::Dyn);
}

#[test]
fn an_array_of_arrays_is_arr_not_arrnum() {
    let (p, k) = kinds_of(
        "var rows = array(2)\n\
         rows[0] = array(4)\n\
         rows[1] = array(4)\n\
         export function render(i) { hsv(rows[0][i % 4], 1, 1) }\n",
    );
    assert_eq!(global_kind(&p, &k, "rows"), Kind::Arr);
}

#[test]
fn an_exported_global_the_host_can_poke_joins_num() {
    // `Engine::set_var` writes a `Value::Num` into any exported global, so
    // an exported array global cannot be proven ArrNum.
    let (p, k) = kinds_of(
        "export var pal = array(4)\n\
         export function render(i) { hsv(pal[i % 4], 1, 1) }\n",
    );
    assert_eq!(global_kind(&p, &k, "pal"), Kind::Dyn);
    // the same pattern with a private global is fine
    let (p2, k2) = kinds_of(
        "var pal = array(4)\n\
         export function render(i) { hsv(pal[i % 4], 1, 1) }\n",
    );
    assert_eq!(global_kind(&p2, &k2, "pal"), Kind::ArrNum);
}

#[test]
fn a_callback_functions_params_are_dyn() {
    // A callback that is only a RUN-TIME value: the call site cannot bind
    // it, so it goes through the shared prelude copy's `CallValue` and the
    // lambda is reachable from anywhere with anything (Gitea #626).
    let (p, k) = kinds_of(
        "var a = array(4)\n\
         var f = (v, i) => i / 4\n\
         var pick = 1\n\
         a.mutate(pick > 0 ? f : f)\n\
         export function render(i) { hsv(a[i % 4], 1, 1) }\n",
    );
    let lam = (0..p.fns.len())
        .find(|&f| p.fns[f].params == 2 && p.fns[f].name.starts_with("<lambda"))
        .expect("the lambda");
    assert_eq!(k.slot(lam, 0), Kind::Dyn);
    // and the array it mutated is no longer provably all-numeric
    assert_eq!(global_kind(&p, &k, "a"), Kind::Arr);
}

/// …but a STATIC callback is specialised: the prelude helper is cloned for
/// this call site, the callback is called with `CallFn`, and its parameters
/// keep their kinds (Gitea #626, docs/jit-design.md §4).
#[test]
fn a_static_callback_keeps_typed_params() {
    let (p, k) = kinds_of(
        "var a = array(4)\n\
         a.mutate((v, i) => i / 4)\n\
         export function render(i) { hsv(a[i % 4], 1, 1) }\n",
    );
    let lam = (0..p.fns.len())
        .find(|&f| p.fns[f].params == 2 && p.fns[f].name.starts_with("<lambda"))
        .expect("the lambda");
    assert_eq!(k.slot(lam, 0), Kind::Num, "the callback's value param");
    assert_eq!(k.slot(lam, 1), Kind::Num, "the callback's index param");
    assert!(
        p.fns.iter().any(|f| f.name.starts_with("arrayMutate$")),
        "the helper should have been cloned for this call site"
    );
    // a numeric callback keeps the array provably all-numeric too
    assert_eq!(global_kind(&p, &k, "a"), Kind::ArrNum);
}

// ------------------------------------------------------------- verifier

#[test]
fn verify_rejects_a_lie_about_a_global() {
    let (p, mut k) = kinds_of(
        "var a = array(4)\n\
         export function render(i) { hsv(a[i % 4], 1, 1) }\n",
    );
    let g = p.global_index("a").unwrap() as usize;
    // claim the array global is a plain number
    k.globals[g] = Kind::Num;
    let e = kinds::verify(&p, &k).expect_err("a Num slot cannot hold an array");
    assert!(e.what.contains("Num"), "{e}");
}

#[test]
fn verify_rejects_a_shape_mismatch() {
    let (p, mut k) = kinds_of("export function render(i) { hsv(i, 1, 1) }\n");
    k.globals.pop();
    let e = kinds::verify(&p, &k).expect_err("shape mismatch");
    assert!(e.what.contains("shape"), "{e}");
}

#[test]
fn a_blob_with_an_invalid_kind_byte_is_rejected() {
    let prog = compile("export function render(i) { hsv(i, 1, 1) }\n").unwrap();
    let mut blob = serialize(&prog).unwrap();
    // the kinds section starts right after the exports table; find a legal
    // kind byte and corrupt it. Scanning is fine: every other byte of the
    // blob we might hit makes the decode fail too, which is also the
    // assertion we want — so require only that SOME corruption is caught.
    let n = blob.len();
    let mut caught = false;
    for i in (n / 2)..n {
        let save = blob[i];
        if save > 5 {
            continue;
        }
        blob[i] = 0x7F;
        if deserialize(&blob).is_err() {
            caught = true;
        }
        blob[i] = save;
        if caught {
            break;
        }
    }
    assert!(caught, "a corrupt kinds section must not decode");
}

// ------------------------------------------------------------ container

#[test]
fn kinds_round_trip_through_the_blob() {
    let src = "var pal = array(8)\n\
               var t = 0\n\
               export function beforeRender(d) { t = d }\n\
               export function render2D(i, x, y) { hsv(t + pal[i % 8] + x + y, 1, 1) }\n";
    let prog = compile(src).unwrap();
    assert!(prog.kinds.is_some(), "a default compile is TYPED");
    let blob = serialize(&prog).unwrap();
    let back = deserialize(&blob).unwrap();
    assert_eq!(prog.kinds, back.kinds, "kinds survive the round trip");
    assert_eq!(blob, serialize(&back).unwrap(), "re-encode is byte-identical");

    // the section's length must follow from the header + fns table alone —
    // that is what lets a decoder without the `kinds` feature skip it
    let untyped = serialize(&compile_with(src, NO_KINDS).unwrap()).unwrap();
    let expect = kinds::Kinds::section_len(
        prog.globals.len(),
        prog.fns.iter().map(|f| f.locals as usize),
    );
    assert_eq!(
        blob.len() - untyped.len(),
        expect,
        "the TYPED blob is exactly the kinds section bigger"
    );
}

#[test]
fn an_untyped_blob_is_legal_and_smaller() {
    let src = "var pal = array(8)\n\
               export function render(i) { hsv(pal[i % 8], 1, 1) }\n";
    let typed = serialize(&compile(src).unwrap()).unwrap();
    let untyped_prog = compile_with(src, NO_KINDS).unwrap();
    assert!(untyped_prog.kinds.is_none());
    let untyped = serialize(&untyped_prog).unwrap();
    assert!(untyped.len() < typed.len(), "the section costs bytes");
    let back = deserialize(&untyped).unwrap();
    assert!(back.kinds.is_none(), "no TYPED flag ⇒ no kinds");
}

// ------------------------------------------------------------ Box is a no-op

fn frames(src: &str, opts: Option<CompileOpts>, n: usize) -> Vec<Vec<[u8; 3]>> {
    let prog = match opts {
        Some(o) => compile_with(src, o).unwrap(),
        None => compile(src).unwrap(),
    };
    let mut e = Engine::from_program_budgeted_at(prog, 60, 7, usize::MAX, Some(1_700_000_000));
    e.take_error();
    (0..n).map(|_| e.frame(Fx::from_int(16)).to_vec()).collect()
}

#[test]
fn box_insertion_does_not_change_pixels() {
    // a ternary whose arms differ in kind: the compiler has to insert a Box
    let src = "var pal = array(4)\n\
               export function render(i) { var v = i > 0.5 ? pal : 0; hsv(i + (v == 0), 1, 1) }\n";
    let prog = compile(src).unwrap();
    assert!(
        box_count(&prog) > 0,
        "this shape is supposed to need a Box"
    );
    assert_eq!(frames(src, None, 4), frames(src, Some(NO_KINDS), 4));
}

fn box_count(prog: &Program) -> usize {
    const BOX: u8 = 0x4F;
    let mut n = 0;
    for f in &prog.fns {
        let code = &prog.words[f.code_start as usize..(f.code_start + f.code_len) as usize];
        let mut at = 0usize;
        while at < code.len() {
            let o = code[at] as u8;
            if o == BOX {
                n += 1;
            }
            at += match o {
                0x01 | 0x48 | 0x49 | 0x4A | 0x4B | 0x4D => 2,
                0x4C => 3,
                _ => 1,
            };
        }
    }
    n
}

// ------------------------------------------------------- library census

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

/// Functions the engine can reach from a render entry — the census's
/// "render path" (docs/jit-design.md §9).
fn render_path(prog: &Program) -> (BTreeSet<usize>, Vec<BTreeSet<usize>>) {
    const CONST_FUN: u8 = 0x02;
    const CALL_FN: u8 = 0x38;
    const CALL_VALUE: u8 = 0x3A;
    let mut const_funs: BTreeSet<usize> = BTreeSet::new();
    let mut callees: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); prog.fns.len()];
    let mut ref_globals: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); prog.fns.len()];
    let mut dyncall = vec![false; prog.fns.len()];
    for (fi, f) in prog.fns.iter().enumerate() {
        let code = &prog.words[f.code_start as usize..(f.code_start + f.code_len) as usize];
        let mut at = 0usize;
        while at < code.len() {
            let w = code[at];
            let o = w as u8;
            match o {
                CONST_FUN => {
                    const_funs.insert((w >> 8) as u16 as usize);
                }
                CALL_FN => {
                    callees[fi].insert((w >> 8) as u16 as usize);
                }
                // No builtin can reach pattern code since #626, so a
                // `CallValue` is the only way out of the call graph.
                CALL_VALUE => dyncall[fi] = true,
                _ => {}
            }
            match o {
                0x04 | 0x05 | 0x42 | 0x45 | 0x47 | 0x4A => {
                    ref_globals[fi].insert((w >> 8) as u16 as usize);
                }
                0x44 => {
                    ref_globals[fi].insert((w >> 16) as u16 as usize);
                }
                _ => {}
            }
            at += match o {
                0x01 | 0x48 | 0x49 | 0x4A | 0x4B | 0x4D => 2,
                0x4C => 3,
                _ => 1,
            };
        }
    }
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    for n in ["render", "render2D", "render3D", "renderFrame"] {
        if let Some(f) = prog.exported_fn(n) {
            seen.insert(f as usize);
        }
        if prog.global_index(n).is_some() {
            seen.extend(const_funs.iter().copied());
        }
    }
    let mut stack: Vec<usize> = seen.iter().copied().collect();
    let mut any_dyncall = false;
    while let Some(f) = stack.pop() {
        any_dyncall |= dyncall[f];
        for &c in &callees[f] {
            if seen.insert(c) {
                stack.push(c);
            }
        }
    }
    if any_dyncall {
        seen.extend(const_funs.iter().copied());
        let mut stack: Vec<usize> = seen.iter().copied().collect();
        while let Some(f) = stack.pop() {
            for &c in &callees[f] {
                if seen.insert(c) {
                    stack.push(c);
                }
            }
        }
    }
    (seen, ref_globals)
}

/// The §9 census, as a regression gate. Every number here is what the real
/// implementation produces today; a compiler change that moves one is
/// either a win worth re-pinning or a bug.
#[test]
fn library_census_is_pinned() {
    let lib = library();
    assert!(lib.len() > 300, "expected the whole library, got {}", lib.len());
    let mut typed = 0usize;
    let mut verified = 0usize;
    let mut boxes = 0usize;
    let mut fully_typed = 0usize;
    let mut round_tripped = 0usize;
    let mut box_patterns: Vec<String> = Vec::new();
    let mut untyped: Vec<String> = Vec::new();
    for (name, src) in &lib {
        let prog = compile(src).unwrap_or_else(|d| panic!("{name}: {}", d.message));
        let Some(k) = prog.kinds.clone() else {
            untyped.push(name.clone());
            continue;
        };
        typed += 1;
        assert_eq!(
            k,
            kinds::infer(&prog),
            "{name}: attached kinds differ from a fresh infer()"
        );
        if let Err(e) = kinds::verify(&prog, &k) {
            panic!("{name}: verify failed: {e}");
        }
        verified += 1;
        let b = box_count(&prog);
        if b > 0 {
            boxes += b;
            box_patterns.push(name.clone());
        }
        // blob round trip with kinds intact
        let blob = serialize(&prog).unwrap();
        let back = deserialize(&blob).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.kinds.as_ref(), Some(&k), "{name}: kinds lost in the blob");
        assert_eq!(blob, serialize(&back).unwrap(), "{name}: re-encode differs");
        round_tripped += 1;
        // fully typed render path? (the census's definition: every slot a
        // render-reachable function actually TOUCHES is non-Dyn)
        let (path, ref_globals) = render_path(&prog);
        let dyn_on_path = path.iter().any(|&f| {
            (0..prog.fns[f].locals as usize).any(|i| k.slot(f, i) == Kind::Dyn)
                || ref_globals[f].iter().any(|&g| k.global(g) == Kind::Dyn)
        });
        if !dyn_on_path {
            fully_typed += 1;
        }
    }
    assert!(untyped.is_empty(), "untyped blobs: {untyped:?}");
    assert_eq!(typed, lib.len());
    assert_eq!(verified, lib.len());
    assert_eq!(round_tripped, lib.len());
    // docs/jit-design.md §9 predicted ~11 Box sites from the prototype;
    // the shipped inference needs 8, in 6 patterns.
    assert_eq!(boxes, 8, "Box sites moved: {box_patterns:?}");
    assert_eq!(box_patterns.len(), 6, "{box_patterns:?}");
    // The prototype measured 291/307 fully typed render paths without
    // modelling the engine's own writes or the declared init value; the
    // shipped, sound inference gets 271 by this test's (stricter,
    // whole-program-globals) definition. `jitcensus` counts only the
    // globals a render-path function actually references and reports 286.
    assert!(
        fully_typed >= 271,
        "fully typed render paths regressed: {fully_typed}/{}",
        lib.len()
    );
}

/// `Box` is a no-op, so a typed blob and an untyped one must render the
/// same pixels — every pattern, bit for bit.
#[test]
fn library_renders_identically_with_and_without_kinds() {
    for (name, src) in library() {
        let a = frames(&src, None, 2);
        let b = frames(&src, Some(NO_KINDS), 2);
        assert_eq!(a, b, "{name}: --no-kinds renders differently");
    }
}
