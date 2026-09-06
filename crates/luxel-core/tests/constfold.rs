//! Constant folding equivalence (Gitea #312).
//!
//! The #312 compiler passes evaluate what they can at compile time —
//! literal arithmetic, reads of predefined globals a pattern never writes,
//! and the operand order of a commutative operator — and drop the
//! old-value recovery from a postfix `x++` whose value is popped. None of
//! that may change a single rendered pixel, a runtime error's text or
//! position, or where the debugger stops.
//!
//! `compile_with(src, UNFOLDED)` is the A/B lever (`luxel run --no-fold`):
//! the same source through the same binary with the passes off. Every test
//! here compares the two.

use luxel_core::bytecode::{deserialize, serialize, validate};
use luxel_core::compile::{compile, compile_with, CompileOpts};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::vm::StepKind;

const UNFOLDED: CompileOpts = CompileOpts {
    superinstructions: true,
    const_folding: false,
};
const UNFUSED_UNFOLDED: CompileOpts = CompileOpts {
    superinstructions: false,
    const_folding: false,
};

/// One source per rewrite, plus the shapes that must NOT be rewritten.
const CASES: &[&str] = &[
    // literal arithmetic, negated literals, and a frozen global feeding a
    // multiply from both sides
    "export function render(index) {
       var a = index * (1 / 3) + -1
       var b = 2 * index - PI2 * index
       var c = index * PI2 / (2 * PI)
       hsv(a + b + c, 1, 1)
     }",
    // postfix increments in both discarding positions, and one used as a
    // value (which must keep the old-value semantics)
    "export function render(index) {
       var s = 0
       var i = 0
       for (i = 0; i < 8; i++) s = s + i
       i++
       var j = 0
       var k = j++
       hsv((s + i + j + k) / 64, 1, 1)
     }",
    // a postfix increment on an array element, in statement position
    "var arr = array(4)
     export function render(index) {
       var i = index % 4
       arr[i]++
       hsv(arr[i] / 8, 1, 1)
     }",
    // commuting across a short-circuit whose jump lands between operands
    "export function render(index) {
       var a = 2 * (index > 1 && index)
       var b = 3 < index ? 4 * index : index * 4
       hsv((a + b) / 64, 1, 1)
     }",
    // every foldable operator over literals, and the non-commutative ones
    // the commute pass must leave alone
    "export function render(index) {
       var s = (1 + 2) + (5 - 3) + (2 * 3) + (7 / 2) + (7 % 3)
       s = s + (2 < 3) + (3 <= 3) + (4 > 5) + (5 >= 5) + (1 == 1) + (1 != 2)
       s = s + (6 & 3) + (6 | 1) + (6 ^ 3) + (1 << 2) + (8 >> 2)
       s = s + 2 / index - 2 - index + 8 / (index + 1)
       hsv(s / 256, 1, 1)
     }",
    // a predefined global the pattern WRITES — must not be frozen
    "PI = 3
     export function render(index) { hsv(index * PI / 16, 1, 1) }",
    // an exported predefined global — settable from the host, so not frozen
    "export var SQRT2 = 2
     export function render(index) { hsv(index * SQRT2 / 16, 1, 1) }",
    // pixelCount, which the engine writes without any StoreG
    "export function render(index) { hsv(index / pixelCount, 1, 1) }",
];

fn frames(src: &str, opts: Option<CompileOpts>, n: usize) -> Vec<Vec<[u8; 3]>> {
    let prog = match opts {
        Some(o) => compile_with(src, o).unwrap(),
        None => compile(src).unwrap(),
    };
    let mut e = Engine::from_program(prog, 32, 7);
    e.take_error();
    (0..n).map(|_| e.frame(Fx::from_int(16)).to_vec()).collect()
}

#[test]
fn folded_and_unfolded_render_identically() {
    for (i, src) in CASES.iter().enumerate() {
        assert_eq!(
            frames(src, None, 8),
            frames(src, Some(UNFOLDED), 8),
            "case {i} renders differently with constant folding"
        );
    }
}

#[test]
fn folding_is_independent_of_the_superinstruction_peephole() {
    // The two passes compose; neither may depend on the other for
    // correctness (`luxel run --no-fuse --no-fold` and every combination
    // in between must agree).
    for (i, src) in CASES.iter().enumerate() {
        let base = frames(src, Some(UNFUSED_UNFOLDED), 6);
        for (name, opts) in [
            ("fused+folded", CompileOpts::default()),
            ("fused only", UNFOLDED),
            (
                "folded only",
                CompileOpts {
                    superinstructions: false,
                    const_folding: true,
                },
            ),
        ] {
            assert_eq!(
                frames(src, Some(opts), 6),
                base,
                "case {i} differs under {name}"
            );
        }
    }
}

#[test]
fn folded_programs_round_trip_byte_identically() {
    for (i, src) in CASES.iter().enumerate() {
        let prog = compile(src).unwrap();
        let blob = serialize(&prog).unwrap();
        validate(&blob).unwrap_or_else(|e| panic!("case {i} fails validation: {e}"));
        let blob2 = serialize(&deserialize(&blob).unwrap()).unwrap();
        assert_eq!(blob, blob2, "case {i} does not re-encode identically");
    }
}

#[test]
fn the_rewrites_actually_fire() {
    // Guards against a pass that silently stops matching. Cases 0, 3 and 4
    // must be strictly shorter folded than unfolded; cases 5-7 (the ones
    // whose globals are NOT frozen) must be exactly the same length.
    // Cases 1 and 2 exercise the postfix-increment rewrite, which is
    // unconditional codegen with no A/B switch — the instruction sequence
    // it produces is pinned by `compile.rs`'s own unit tests instead.
    let words = |p: &luxel_core::vm::Program| -> usize {
        p.fns.iter().map(|f| f.code_len as usize).sum()
    };
    for i in [0usize, 3, 4] {
        let folded = words(&compile(CASES[i]).unwrap());
        let plain = words(&compile_with(CASES[i], UNFOLDED).unwrap());
        assert!(
            folded < plain,
            "case {i} folded to no fewer words ({folded} vs {plain})"
        );
    }
    for i in 5..CASES.len() {
        assert_eq!(
            words(&compile(CASES[i]).unwrap()),
            words(&compile_with(CASES[i], UNFOLDED).unwrap()),
            "case {i} was rewritten even though its global is writable"
        );
    }
}

#[test]
fn a_written_predefined_global_keeps_its_written_value() {
    // The strongest form of the freeze check: `PI = 3` really does change
    // what `PI` reads as, folding on.
    let src = "PI = 3
export var out = 0
export function render(index) { out = PI }";
    let mut e = Engine::from_program(compile(src).unwrap(), 4, 1);
    e.take_error();
    e.frame(Fx::from_int(16));
    assert_eq!(
        e.var("out"),
        Some(luxel_core::vm::Value::Num(Fx::from_int(3)))
    );
}

#[test]
fn runtime_errors_survive_the_rewrites_unchanged() {
    // Folding removes instructions from the stream; the surviving ones
    // must still be attributed to the same source position.
    let src = "var arr = array(4)
export function render(index) {
  var i = 2 * (index + 10)
  hsv(arr[i], 1, 1)
}";
    let err = |opts| {
        let mut e = Engine::from_program(compile_with(src, opts).unwrap(), 8, 1);
        e.take_error();
        e.frame(Fx::from_int(16));
        let er = e.take_error().expect("out-of-bounds read must error");
        (er.message, er.line, er.col)
    };
    assert_eq!(err(CompileOpts::default()), err(UNFOLDED));
    assert_eq!(err(UNFOLDED).0, "array index out of bounds");
}

#[test]
fn compile_errors_are_unaffected() {
    for opts in [CompileOpts::default(), UNFOLDED] {
        let d = compile_with("export function render(i) { hsv(nope * 2, 1, 1) }", opts)
            .unwrap_err();
        assert_eq!(d.message, "unknown identifier `nope`");
    }
}

#[test]
fn the_debugger_still_stops_on_every_source_line() {
    // Neither pass moves an instruction across a source position, so
    // stepping visits exactly the lines it visited before.
    let src = "export function render(index) {
  var a = 2 * index
  var b = a + (1 / 4)
  var c = b * PI2
  var d = 0
  for (var i = 0; i < 2; i++) d = d + c
  hsv(d, 1, 1)
}";
    let lines = |opts| {
        let mut e = Engine::from_program(compile_with(src, opts).unwrap(), 4, 1);
        e.take_error();
        e.debug_set_enabled(true);
        assert_eq!(e.debug_set_breakpoints(&[2]), vec![2]);
        e.frame(Fx::from_int(16));
        let mut seen = Vec::new();
        for _ in 0..24 {
            match e.debug_location() {
                Some((line, _, _)) => seen.push(line),
                None => break,
            }
            if !e.debug_step(StepKind::Into) {
                break;
            }
        }
        seen
    };
    let folded = lines(CompileOpts::default());
    assert_eq!(folded, lines(UNFOLDED));
    for line in [2u32, 3, 4, 5, 6, 7] {
        assert!(
            folded.contains(&line),
            "never stopped on line {line}: {folded:?}"
        );
    }
}
