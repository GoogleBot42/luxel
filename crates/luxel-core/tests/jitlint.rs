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
fn a_callback_builtin_refuses_the_whole_program() {
    let prog = build(
        "var a = array(4)\n\
         export function beforeRender(delta) {\n\
           arrayMutate(a, (v, i) => 1)\n\
         }\n\
         export function render(i) { hsv(a[0], 1, 1) }\n",
    );
    let kinds = kinds_of(&prog);
    let err = jit_eligibility(&prog, &kinds).expect_err("arrayMutate is refused");
    let JitRefusal::Callbacks { name, line, .. } = &err;
    assert_eq!(*name, "arrayMutate");
    assert_eq!(*line, 3, "anchored on the call site");
    assert_eq!(err.id(), "callbacks");
    assert!(
        err.text().contains("arrayMutate") && err.text().contains("callback"),
        "unhelpful wording: {}",
        err.text()
    );
}

#[test]
fn every_callback_builtin_is_found_by_signature_not_by_name() {
    // The six of docs/jit-design.md §4. They are found through
    // `builtin_sig(..).callback`, so #626 removing them removes the refusal
    // with no edit here.
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
        assert!(
            jit_eligibility(&prog, &kinds).is_err(),
            "{call} must force interpreter mode"
        );
    }
}

#[test]
fn a_callback_builtin_used_as_a_value_still_refuses() {
    let prog = build("var f = arrayMutate\nexport function render(i) { hsv(0, 1, 1) }\n");
    let kinds = kinds_of(&prog);
    assert!(jit_eligibility(&prog, &kinds).is_err());
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
    let src = "var a = array(4)\n\
               export function beforeRender(delta) {\n\
                 arrayMutate(a, (v, i) => v + 1)\n\
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
    assert_eq!(v.line, 3, "the lambda's own line");
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
