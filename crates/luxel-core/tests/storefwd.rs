//! Store-forwarding equivalence (Gitea #320).
//!
//! `x = <expr>` followed by a statement that reads `x` back compiles to
//! `StoreL x; Pop; LoadL x`. `StoreL` peeks, so the `Pop; LoadL` pair
//! throws the value away and reads it straight back; the pass deletes it.
//!
//! Unlike the #261 peephole this deliberately carries a live value ACROSS
//! a statement boundary, so it needs its own evidence: identical pixels,
//! identical runtime errors and positions, identical debugger stops with
//! the extra stack entry live, and identical `MAX_STACK` verdicts.
//!
//! `compile_with(src, UNFORWARDED)` is the A/B lever
//! (`luxel run --no-storefwd`).

use luxel_core::bytecode::{deserialize, serialize, validate};
use luxel_core::compile::{compile, compile_with, CompileOpts};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::vm::StepKind;

const UNFORWARDED: CompileOpts = CompileOpts {
    superinstructions: true,
    const_folding: true,
    store_forwarding: false,
};
const BARE: CompileOpts = CompileOpts {
    superinstructions: false,
    const_folding: false,
    store_forwarding: false,
};

/// One source per shape the pass fires on, plus the shapes it must refuse.
const CASES: &[&str] = &[
    // the canonical site: a local declared then handed to a builtin, and a
    // compound assignment read back by the next statement
    "export function render(index) {
       var h = index / pixelCount
       hsv(h, 1, 1)
     }",
    "export function render(index) {
       var h = index / pixelCount
       h = h * 0.5
       h = h + 1
       hsv(h, 1, 1)
     }",
    // globals: StoreG; Pop; LoadG is the same shape one storage class over
    "var g = 0
     export function render(index) {
       g = index * 2
       hsv(g / 128, 1, 1)
     }",
    // the read is NOT the next statement's first operand — must not fire
    "export function render(index) {
       var d = index / pixelCount
       hsv(1 - d * 4, 1, 1)
     }",
    // the next statement is a JUMP TARGET: a loop head reading the stored
    // variable, an else arm, and a while condition
    "export function render(index) {
       var i = 0
       while (i < 4) { i = i + 1 }
       var s = 0
       for (var k = 0; k < 3; k++) s = s + k
       if (index > 1) { var a = 1 } else { var a = 2 }
       hsv((i + s) / 16, 1, 1)
     }",
    // a store that ends a BLOCK, with the read outside it: the read is the
    // `if`'s false target, so neither statement here may be forwarded
    "export function render(index) {
       var t = 0
       if (index > 0) { t = index }
       t = t + 1
       hsv(1 - t, 1, 1)
     }",
    // the stored value is read back as an array index and as an array
    // element, both of which push more after the load
    "var arr = array(8)
     export function render(index) {
       var i = index % 8
       arr[i] = i
       var v = arr[i]
       hsv(v / 8, 1, 1)
     }",
    // a different variable is read next — must not fire
    "export function render(index) {
       var a = index
       var b = 2
       hsv((a + b) / 64, 1, 1)
     }",
    // stored then returned, and stored inside a called function
    "function twice(v) {
       var d = v * 2
       return d
     }
     export function render(index) {
       var h = twice(index)
       hsv(h / 128, 1, 1)
     }",
    // short-circuit and ternary reads of a stored variable: the jumps land
    // AFTER the read, so all three statements forward
    "export function render(index) {
       var a = index > 1
       var b = a && index
       var c = b ? index : 0
       hsv(c / 64, 1, 1)
     }",
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
fn forwarded_and_unforwarded_render_identically() {
    for (i, src) in CASES.iter().enumerate() {
        assert_eq!(
            frames(src, None, 8),
            frames(src, Some(UNFORWARDED), 8),
            "case {i} renders differently with store forwarding"
        );
    }
}

#[test]
fn forwarding_is_independent_of_the_other_two_passes() {
    // All three passes compose; none may depend on another for correctness
    // (`--no-fuse --no-fold --no-storefwd` and every combination between).
    for (i, src) in CASES.iter().enumerate() {
        let base = frames(src, Some(BARE), 6);
        for fuse in [false, true] {
            for fold in [false, true] {
                for fwd in [false, true] {
                    let opts = CompileOpts {
                        superinstructions: fuse,
                        const_folding: fold,
                        store_forwarding: fwd,
                    };
                    assert_eq!(
                        frames(src, Some(opts), 6),
                        base,
                        "case {i} differs at fuse={fuse} fold={fold} fwd={fwd}"
                    );
                }
            }
        }
    }
}

#[test]
fn forwarded_programs_round_trip_byte_identically() {
    for (i, src) in CASES.iter().enumerate() {
        let prog = compile(src).unwrap();
        let blob = serialize(&prog).unwrap();
        validate(&blob).unwrap_or_else(|e| panic!("case {i} fails validation: {e}"));
        let blob2 = serialize(&deserialize(&blob).unwrap()).unwrap();
        assert_eq!(blob, blob2, "case {i} does not re-encode identically");
    }
}

#[test]
fn the_rewrite_fires_where_it_should_and_nowhere_else() {
    // Guards against a pass that silently stops matching, and against one
    // that starts matching a shape it must refuse. Measured with the
    // peephole OFF: with it on, a `LoadL` that would have fused into a
    // superinstruction anyway hides the saving (see UPDATES.md).
    let words = |o: CompileOpts, src: &str| -> usize {
        compile_with(src, o).unwrap().fns.iter().map(|f| f.code_len as usize).sum()
    };
    let on = CompileOpts {
        superinstructions: false,
        const_folding: true,
        store_forwarding: true,
    };
    let off = CompileOpts {
        store_forwarding: false,
        ..on
    };
    // cases 0-2, 6, 8 and 9 contain at least one forwardable site. Case 9
    // is the interesting one: a short-circuit `&&` and a ternary both read
    // the stored variable as their FIRST operand, and their jumps land
    // after that read, not on it.
    for i in [0usize, 1, 2, 6, 8, 9] {
        assert!(
            words(on, CASES[i]) < words(off, CASES[i]),
            "case {i} was not rewritten ({} vs {})",
            words(on, CASES[i]),
            words(off, CASES[i])
        );
    }
    // cases 3, 4, 5 and 7 contain none: the read is not the next
    // statement's first operand, the read is a jump target, or a different
    // variable is read
    for i in [3usize, 4, 5, 7] {
        assert_eq!(
            words(on, CASES[i]),
            words(off, CASES[i]),
            "case {i} was rewritten even though it must not be"
        );
    }
}

#[test]
fn runtime_errors_survive_the_rewrite_unchanged() {
    // The pass deletes the instruction that carried the second statement's
    // source position; the statement's remaining instructions must still
    // report it.
    let src = "var arr = array(4)
export function render(index) {
  var i = index + 10
  hsv(arr[i], 1, 1)
}";
    let err = |opts| {
        let mut e = Engine::from_program(compile_with(src, opts).unwrap(), 8, 1);
        e.take_error();
        e.frame(Fx::from_int(16));
        let er = e.take_error().expect("out-of-bounds read must error");
        (er.message, er.line, er.col)
    };
    assert_eq!(err(CompileOpts::default()), err(UNFORWARDED));
    assert_eq!(err(UNFORWARDED).0, "array index out of bounds");
}

#[test]
fn compile_errors_are_unaffected() {
    for opts in [CompileOpts::default(), UNFORWARDED] {
        let d = compile_with("export function render(i) { hsv(nope * 2, 1, 1) }", opts).unwrap_err();
        assert_eq!(d.message, "unknown identifier `nope`");
    }
}

#[test]
fn the_debugger_still_stops_on_every_source_line() {
    // The load the pass deletes is the FIRST instruction of the second
    // statement, so that statement's position run starts one instruction
    // later — but it still exists, and stepping must visit exactly the
    // lines it visited before.
    let src = "export function render(index) {
  var a = index * 2
  var b = a
  var c = b + 1
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
        let mut seen = alloc_vec();
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
    let forwarded = lines(CompileOpts::default());
    assert_eq!(forwarded, lines(UNFORWARDED));
    for line in [2u32, 3, 4, 5, 6, 7] {
        assert!(
            forwarded.contains(&line),
            "never stopped on line {line}: {forwarded:?}"
        );
    }
}

fn alloc_vec() -> Vec<u32> {
    Vec::new()
}

#[test]
fn the_variable_inspector_sees_the_same_values_at_every_stop() {
    // The extra live stack entry must not disturb what the debugger reports
    // for the locals — the playground's variable panel reads exactly this.
    let src = "export function render(index) {
  var a = index * 2
  var b = a
  var c = b + 1
  hsv(c / 64, 1, 1)
}";
    let trace = |opts| {
        let mut e = Engine::from_program(compile_with(src, opts).unwrap(), 4, 1);
        e.take_error();
        e.debug_set_enabled(true);
        e.debug_set_breakpoints(&[2]);
        e.frame(Fx::from_int(16));
        let mut seen = Vec::new();
        for _ in 0..24 {
            let Some(loc) = e.debug_location() else { break };
            let frames: Vec<_> = e
                .debug_stack()
                .into_iter()
                .map(|f| (f.name, f.locals))
                .collect();
            seen.push((loc, frames));
            if !e.debug_step(StepKind::Into) {
                break;
            }
        }
        seen
    };
    assert_eq!(trace(CompileOpts::default()), trace(UNFORWARDED));
}

#[test]
fn a_stack_overflow_still_overflows_at_the_same_place() {
    // The pass elides one `LoadL` push and its MAX_STACK check, but the
    // value it would have pushed is already on the stack, so the peak depth
    // is unchanged and a program that overflowed still overflows.
    let src = "function deep(v) { return deep(v) + 1 }
export function render(index) {
  var a = deep(index)
  hsv(a, 1, 1)
}";
    let err = |opts| {
        let mut e = Engine::from_program(compile_with(src, opts).unwrap(), 4, 1);
        e.take_error();
        e.frame(Fx::from_int(16));
        e.take_error().map(|er| (er.message, er.line))
    };
    let a = err(CompileOpts::default());
    assert!(a.is_some(), "unbounded recursion must fail");
    assert_eq!(a, err(UNFORWARDED));
}
