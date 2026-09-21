//! The playground's JIT lint: which programs the JIT refuses at compile
//! time, and the name + position enrichment the editor anchors a boxed-slot
//! warning on (Gitea #627, docs/jit-design.md §4a).

use luxel_core::compile::compile;
use luxel_core::jitlint::{cause_id, dyn_lints, jit_eligibility, slot_stats, DynScope, JitRefusal};
use luxel_core::kinds::DynCause;
use luxel_core::vm::Program;

fn build(src: &str) -> Program {
    compile(src).expect("compiles")
}

fn kinds_of(prog: &Program) -> luxel_core::kinds::Kinds {
    prog.kinds.clone().expect("compile() infers kinds")
}

// ------------------------------------------------------------ eligibility

#[test]
fn every_callback_helper_is_eligible_since_it_is_a_prelude_function() {
    // The six of docs/jit-design.md §4 are pattern-language prelude
    // functions since #626: the compiler never emits a builtin call for
    // them, so nothing in the source forces interpreter mode.
    for call in [
        "arrayForEach(a, (v, i) => 0)",
        "arrayMutate(a, (v, i) => 1)",
        "arrayMapTo(a, b, (v, i) => v)",
        "arrayReduce(a, (acc, v) => acc + v)",
        "arraySortBy(a, (x, y) => x - y)",
        "mapPixels((i, x) => 0)",
    ] {
        let src = format!(
            "var a = array(4)\nvar b = array(4)\nexport function render(i) {{ {call}\n hsv(0, 1, 1) }}\n"
        );
        let prog = build(&src);
        let kinds = kinds_of(&prog);
        assert_eq!(
            jit_eligibility(&prog, &kinds),
            Ok(()),
            "{call} is a prelude function and must not force interpreter mode"
        );
    }
}

#[test]
fn a_removed_builtin_in_the_words_still_refuses() {
    // The tombstone check: a stale or hand-built blob that calls one of
    // the removed ids is refused at the call word. The compiler cannot
    // produce this, so the word is patched in by hand.
    use luxel_core::vm::{BKind, Words, BUILTINS};
    let removed = BUILTINS
        .iter()
        .position(|b| b.name == "arrayMutate" && matches!(b.kind, BKind::Removed))
        .expect("arrayMutate is a tombstone") as u32;
    let mut prog = build("export function render(i) { hsv(sin(i), 1, 1) }\n");
    let kinds = kinds_of(&prog);
    assert_eq!(jit_eligibility(&prog, &kinds), Ok(()));
    // Replace the `sin` call with the tombstone: CallBuiltin = 0x39,
    // id in bits 8..24, argc in bits 24..32.
    let sin = BUILTINS.iter().position(|b| b.name == "sin").unwrap() as u32;
    let mut words: Vec<u32> = prog.words.to_vec();
    let at = words
        .iter()
        .position(|&w| w & 0xFF == 0x39 && (w >> 8) & 0xFFFF == sin)
        .expect("the sin call word");
    words[at] = 0x39 | (removed << 8) | (words[at] & 0xFF00_0000);
    prog.words = Words::Owned(words);
    let err = jit_eligibility(&prog, &kinds).expect_err("a tombstone call is refused");
    let JitRefusal::Callbacks { name, .. } = &err;
    assert_eq!(*name, "arrayMutate");
    assert_eq!(err.id(), "callbacks");
}

#[test]
fn a_plain_pattern_is_eligible() {
    let prog = build(
        "var t = 0\n\
         export function beforeRender(delta) { t = t + delta / 1000 }\n\
         export function render(i) { hsv(t + i / pixelCount, 1, 1) }\n",
    );
    let kinds = kinds_of(&prog);
    assert_eq!(jit_eligibility(&prog, &kinds), Ok(()));
}

#[test]
fn ordinary_array_builtins_and_lambdas_are_eligible() {
    // A function VALUE is not a refusal: v1 compiles `CallValue` (§4).
    let prog = build(
        "var a = array(8)\n\
         var f = (x) => x * 2\n\
         export function beforeRender(delta) { arraySum(a) }\n\
         export function render(i) { hsv(f(i / pixelCount), 1, 1) }\n",
    );
    let kinds = kinds_of(&prog);
    assert_eq!(jit_eligibility(&prog, &kinds), Ok(()));
}

// -------------------------------------------------------- boxed-slot lint

#[test]
fn a_two_kind_global_is_named_and_anchored_on_the_widening_store() {
    let src = "var heat = 0\n\
               export function beforeRender(delta) {\n\
                 heat = array(8)\n\
               }\n\
               export function render(i) { hsv(heat[0], 1, 1) }\n";
    let prog = build(src);
    let kinds = kinds_of(&prog);
    let lints = dyn_lints(&prog, &kinds);
    let heat = lints
        .iter()
        .find(|l| l.name == "heat")
        .unwrap_or_else(|| panic!("no lint for `heat`: {lints:?}"));
    assert_eq!(heat.scope, DynScope::Global);
    assert_eq!(heat.cause, DynCause::AssignMerge);
    assert_eq!(cause_id(heat.cause), "assign-merge");
    // `heat = array(8)`, not the `var heat = 0` on line 1.
    assert_eq!(heat.line, 3, "anchored on the store that widened it");
    assert!(heat.col > 0);
    assert!(
        heat.message.starts_with("`heat` ") && heat.message.ends_with("so it runs boxed"),
        "unhelpful wording: {}",
        heat.message
    );
}

#[test]
fn a_callback_param_is_named_from_debug_info_and_anchored_in_its_function() {
    // A callback held in a VARIABLE is a run-time value: #626's
    // specialisation cannot bind it, so the prelude's shared copy calls it
    // through `CallValue` and its params are boxed. (A literal lambda or a
    // named function at the call site is specialised and stays typed.)
    let src = "var a = array(4)\n\
               var bump = (v, i) => v + 1\n\
               export function beforeRender(delta) {\n\
                 arrayMutate(a, bump)\n\
               }\n\
               export function render(i) { hsv(a[0], 1, 1) }\n";
    let prog = build(src);
    let kinds = kinds_of(&prog);
    let lints = dyn_lints(&prog, &kinds);
    let v = lints
        .iter()
        .find(|l| l.name == "v" && l.scope == DynScope::Local)
        .unwrap_or_else(|| panic!("no lint for the lambda's `v`: {lints:?}"));
    assert_eq!(v.cause, DynCause::CallbackParam);
    assert_eq!(v.line, 2, "the lambda's own line");
    assert!(
        !v.fn_name.is_empty(),
        "a local lint names the function it lives in"
    );
    assert!(v.message.contains("`v`"));
}

#[test]
fn a_clean_pattern_has_no_lints_and_every_slot_is_typed() {
    let prog = build(
        "var t = 0\n\
         export function beforeRender(delta) { t = t + delta / 1000 }\n\
         export function render(i) { hsv(t + i / pixelCount, 1, 1) }\n",
    );
    let kinds = kinds_of(&prog);
    assert_eq!(dyn_lints(&prog, &kinds), vec![]);
    let s = slot_stats(&prog, &kinds);
    assert!(s.total > 0);
    assert_eq!(s.typed, s.total, "nothing boxed: {s:?}");
}

#[test]
fn one_lint_per_slot_even_with_several_causes() {
    let src = "var a = array(4)\n\
               var m = 0\n\
               export function beforeRender(delta) {\n\
                 m = array(2)\n\
                 m[0] = (x) => x\n\
                 a = m\n\
               }\n\
               export function render(i) { hsv(0, 1, 1) }\n";
    let prog = build(src);
    let kinds = kinds_of(&prog);
    let lints = dyn_lints(&prog, &kinds);
    let mut seen = std::collections::BTreeSet::new();
    for l in &lints {
        assert!(seen.insert(l.slot), "slot linted twice: {l:?}");
        assert!(!l.name.is_empty());
        assert!(l.message.contains(&l.name));
    }
    let s = slot_stats(&prog, &kinds);
    assert!(s.typed < s.total, "this program boxes something: {s:?}");
}

#[test]
fn lints_come_back_in_source_order() {
    let src = "var heat = 0\n\
               var glow = 0\n\
               export function beforeRender(delta) {\n\
                 glow = array(2)\n\
                 heat = array(8)\n\
               }\n\
               export function render(i) { hsv(heat[0] + glow[0], 1, 1) }\n";
    let prog = build(src);
    let kinds = kinds_of(&prog);
    let lints = dyn_lints(&prog, &kinds);
    let lines: Vec<u32> = lints.iter().map(|l| l.line).collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted, "{lints:?}");
}
