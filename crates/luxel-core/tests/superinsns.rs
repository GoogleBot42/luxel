//! Superinstruction equivalence (Gitea #261).
//!
//! Every fused opcode must be EXACTLY the base sequence it replaces. The
//! tests here pin that from both ends: the same source compiled with and
//! without the peephole must render the same pixels, raise the same errors
//! at the same source positions, stop the debugger on the same lines, and
//! survive the same LXBC round-trip — and a v5 blob that uses no
//! superinstruction at all must still decode and run, since appending them
//! did not change `FORMAT_VERSION`.

use luxel_core::bytecode::{deserialize, serialize, validate, FORMAT_VERSION};
use luxel_core::compile::{compile, compile_with, CompileOpts};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::vm::StepKind;

const UNFUSED: CompileOpts = CompileOpts {
    superinstructions: false,
    const_folding: true,
};

/// One source per fusion family, plus the shapes that must NOT fuse.
const CASES: &[&str] = &[
    // StoreLPop / LoadLConstOp / PopRetNull / CallBuiltinCC — the `i++`,
    // `x * 0.5` and `hsv(h, 1, 1)` shapes.
    "export function render(index) {
       var h = index / pixelCount
       h = h * 0.5
       h = h + 1
       hsv(h, 1, 1)
     }",
    // LoadGL / LoadGLIdx / LoadLIdx — the global-array read idiom.
    "var arr = array(16)
     var n = 3
     export function beforeRender(delta) { n = (n + 1) % 16 }
     export function render(index) {
       var i = index % 16
       var v = arr[i] + arr[n]
       arr[i] = v * 0.5
       rgb(v, v, v)
     }",
    // CmpJf / LoadLL / ConstOp — loop headers and comparisons.
    "export function render(index) {
       var s = 0
       for (var i = 0; i < 8; i++) {
         if (i * index > 4) s = s + i
         else s = s - 1
       }
       hsv(s / 64, 1, 1)
     }",
    // StoreGPop / CallBuiltinC — global assignment in statement context and
    // a unary builtin with a constant argument.
    "var t = 0
     export function beforeRender(delta) { t = time(0.1) }
     export function render(index) { hsv(t + wave(0.3), 1, 1) }",
    // A jump landing between two otherwise fusable instructions (the `&&`
    // short-circuit lands right on the second half of a pair).
    "export function render(index) {
       var a = index > 2 && index < 6
       var b = a ? index * 2 : index + 2
       hsv(b / pixelCount, 1, 1)
     }",
    // Every comparison and arithmetic sub-opcode, against a constant.
    "export function render(index) {
       var x = index
       var s = (x < 1) + (x <= 2) + (x > 3) + (x >= 4) + (x == 5) + (x != 6)
       s = s + (x + 1) + (x - 1) + (x * 2) + (x / 2) + (x % 3)
       hsv(s / 64, 1, 1)
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
fn fused_and_unfused_render_identically() {
    for (i, src) in CASES.iter().enumerate() {
        assert_eq!(
            frames(src, None, 8),
            frames(src, Some(UNFUSED), 8),
            "case {i} renders differently with superinstructions"
        );
    }
}

#[test]
fn fused_programs_round_trip_byte_identically() {
    for (i, src) in CASES.iter().enumerate() {
        let prog = compile(src).unwrap();
        let blob = serialize(&prog).unwrap();
        validate(&blob).unwrap_or_else(|e| panic!("case {i} fails validation: {e}"));
        let blob2 = serialize(&deserialize(&blob).unwrap()).unwrap();
        assert_eq!(blob, blob2, "case {i} does not re-encode identically");
        // and the decoded program renders like the source path
        let mut a = Engine::from_program(compile(src).unwrap(), 32, 7);
        let mut b = Engine::from_program(deserialize(&blob).unwrap(), 32, 7);
        a.take_error();
        b.take_error();
        for _ in 0..5 {
            assert_eq!(
                a.frame(Fx::from_int(16)).to_vec(),
                b.frame(Fx::from_int(16)).to_vec(),
                "case {i} differs after a bytecode round-trip"
            );
        }
    }
}

#[test]
fn the_superinstructions_are_actually_emitted() {
    // Guards against a peephole that silently stops matching: every case
    // must be strictly shorter fused than unfused.
    for (i, src) in CASES.iter().enumerate() {
        let fused = compile(src).unwrap();
        let plain = compile_with(src, UNFUSED).unwrap();
        let words = |p: &luxel_core::vm::Program| -> usize {
            p.fns.iter().map(|f| f.code_len as usize).sum()
        };
        assert!(
            words(&fused) < words(&plain),
            "case {i} fused to no fewer words ({} vs {})",
            words(&fused),
            words(&plain)
        );
    }
}

#[test]
fn a_blob_using_no_superinstruction_still_validates_and_runs() {
    // Appending opcodes did NOT bump the format version, so an unfused v5
    // blob — what an older producer emits — has to keep working.
    let src = CASES[0];
    let blob = serialize(&compile_with(src, UNFUSED).unwrap()).unwrap();
    assert_eq!(u16::from_le_bytes([blob[4], blob[5]]), FORMAT_VERSION);
    validate(&blob).unwrap();
    let mut e = Engine::from_program(deserialize(&blob).unwrap(), 32, 7);
    e.take_error();
    assert_eq!(
        (0..4)
            .map(|_| e.frame(Fx::from_int(16)).to_vec())
            .collect::<Vec<_>>(),
        frames(src, Some(UNFUSED), 4)
    );
}

#[test]
fn runtime_errors_inside_a_fused_op_report_the_same_place() {
    // LoadGLIdx / LoadLIdx carry LoadIdx's two error messages; the fused
    // and unfused forms must agree on message AND (line, col).
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
    assert_eq!(err(CompileOpts::default()), err(UNFUSED));
    assert_eq!(err(UNFUSED).0, "array index out of bounds");

    // …and indexing a non-array through the same fused op
    let src2 = "var notarr = 3
export function render(index) {
  hsv(notarr[index], 1, 1)
}";
    let err2 = |opts| {
        let mut e = Engine::from_program(compile_with(src2, opts).unwrap(), 8, 1);
        e.take_error();
        e.frame(Fx::from_int(16));
        let er = e.take_error().expect("must error");
        (er.message, er.line, er.col)
    };
    assert_eq!(err2(CompileOpts::default()), err2(UNFUSED));
    assert_eq!(err2(UNFUSED).0, "indexing a non-array value");
}

#[test]
fn the_debugger_still_stops_on_every_source_line() {
    // The peephole never fuses across a source position, so stepping visits
    // exactly the lines it visited before.
    let src = "export function render(index) {
  var a = index * 2
  var b = a + 1
  var c = b / pixelCount
  hsv(c, 1, 1)
}";
    let lines = |opts| {
        let mut e = Engine::from_program(compile_with(src, opts).unwrap(), 4, 1);
        e.take_error();
        e.debug_set_enabled(true);
        assert_eq!(e.debug_set_breakpoints(&[2]), alloc_vec(&[2]));
        e.frame(Fx::from_int(16));
        let mut seen = alloc::vec::Vec::new();
        for _ in 0..8 {
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
    let fused = lines(CompileOpts::default());
    assert_eq!(fused, lines(UNFUSED));
    // and it really did stop on every statement line of the body (the run
    // wraps to the next pixel, so lines repeat — that is the same either way)
    for line in [2u32, 3, 4, 5] {
        assert!(fused.contains(&line), "never stopped on line {line}: {fused:?}");
    }
}

extern crate alloc;
fn alloc_vec(v: &[u32]) -> alloc::vec::Vec<u32> {
    v.to_vec()
}


/// The word region is the tail of the blob (`serialize` writes it last), so
/// its first byte is `len - 4 * n_words`. Scanning only there keeps a name
/// or message byte from masquerading as an opcode.
fn find_word(blob: &[u8], words: usize, opcode: u8) -> usize {
    let start = blob.len() - words * 4;
    (start..blob.len())
        .step_by(4)
        .find(|&at| blob[at] == opcode)
        .unwrap_or_else(|| panic!("no word with opcode {opcode:#04x} in the code region"))
}

fn patched(blob: &[u8], at: usize, w: u32) -> alloc::vec::Vec<u8> {
    let mut b = blob.to_vec();
    b[at..at + 4].copy_from_slice(&w.to_le_bytes());
    b
}

/// A source that emits STORE_L_POP, LOAD_LL, LOAD_LG, CONST_OP and CMP_JF.
const MALFORMED_SRC: &str = "var n = 4
export function render(index) {
  var a = index
  var b = a
  var s = a * b + 1
  if (a > b) s = s + 1
  if (a < n) s = s - 1
  hsv(s, 1, 1)
}";

#[test]
fn the_decoder_rejects_malformed_superinstructions() {
    let prog = compile(MALFORMED_SRC).unwrap();
    let nwords = prog.words.len();
    let blob = serialize(&prog).unwrap();
    validate(&blob).unwrap();
    let find = |o| find_word(&blob, nwords, o);

    // CONST_OP with a sub-opcode that is not a two-operand value op
    let at = find(0x48);
    assert!(validate(&patched(&blob, at, 0x48 | (0x08 << 8))).is_err());
    // CMP_JF with a value op that is not a COMPARISON
    let at = find(0x4D);
    assert!(validate(&patched(&blob, at, 0x4D | (0x10 << 8))).is_err());
    // CMP_JF's target lives in the word AFTER the opcode word
    assert!(validate(&patched(&blob, at + 4, 0xFFFF_FFFF)).is_err());
    // reserved operand bits set on a u8-only superinstruction
    let at = find(0x41);
    let w = u32::from_le_bytes(blob[at..at + 4].try_into().unwrap());
    assert!(validate(&patched(&blob, at, w | 1 << 20)).is_err());
    // a local slot past the function's frame
    assert!(validate(&patched(&blob, at, 0x41 | (250 << 8))).is_err());
    // LOAD_LL's SECOND local operand is validated too
    let at = find(0x43);
    let w = u32::from_le_bytes(blob[at..at + 4].try_into().unwrap());
    assert!(validate(&patched(&blob, at, (w & 0x0000_FFFF) | 250 << 16)).is_err());
    // LOAD_LG's global operand lives in the high half
    let at = find(0x44);
    let w = u32::from_le_bytes(blob[at..at + 4].try_into().unwrap());
    assert!(validate(&patched(&blob, at, (w & 0x0000_FFFF) | 9999 << 16)).is_err());
}
