//! LXBC round-trip and validation tests. The corpus-wide round-trip runs in
//! `luxel check` (tools/corpus/report.mjs); these cover the properties and
//! the rejection paths directly.

use luxel_core::bytecode::{deserialize, serialize, validate, BcError, FORMAT_VERSION};
use luxel_core::compile::compile;
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;

const PATTERN: &str = r#"
export var speed = 0.5
var arr = array(8)
arr.mutate((v, i) => i / 8)
export function sliderSpeed(v) { speed = v }
export function beforeRender(delta) { t = time(0.05 * speed) }
export function render(index) {
  var h = t + index / pixelCount + arr[index % 8]
  hsv(h, 1, triangle(h) > 0.5 ? 1 : 0.2)
}
"#;

fn frames(e: &mut Engine, n: usize) -> Vec<Vec<[u8; 3]>> {
    (0..n).map(|_| e.frame(Fx::from_int(16)).to_vec()).collect()
}

#[test]
fn roundtrip_is_byte_identical_and_runs_identically() {
    let prog = compile(PATTERN).unwrap();
    let blob = serialize(&prog).unwrap();
    let prog2 = deserialize(&blob).unwrap();
    let blob2 = serialize(&prog2).unwrap();
    assert_eq!(blob, blob2, "serialize∘deserialize must be identity");

    // identical rendering, source path vs bytecode path (same seed)
    let mut a = Engine::new(PATTERN, 60, 42).unwrap();
    let mut b = Engine::from_program(prog2, 60, 42);
    assert_eq!(frames(&mut a, 5), frames(&mut b, 5));

    // by-name surfaces survive: controls, exported vars
    let e = Engine::from_program(deserialize(&blob).unwrap(), 60, 42);
    assert_eq!(e.controls().len(), 1);
    assert_eq!(e.controls()[0].name, "sliderSpeed");
    assert!(e.exported_vars().any(|v| v == "speed"));
}

#[test]
fn debug_info_survives() {
    let prog = compile(PATTERN).unwrap();
    let prog2 = deserialize(&serialize(&prog).unwrap()).unwrap();
    for (f, g) in prog.fns.iter().zip(prog2.fns.iter()) {
        assert_eq!(f.pos, g.pos, "positions differ in {}", f.name);
        assert_eq!(f.local_names, g.local_names);
        assert_eq!(f.name, g.name);
    }
}

#[test]
fn assert_messages_round_trip_and_validate() {
    // messages dedup into the v4 table and survive serialize→deserialize
    let src = "assert(pixelCount > 4, \"needs at least 5 pixels\")\n\
               assert(pixelCount < 10000, \"needs at least 5 pixels\")\n\
               assert(pixelCount % 2 == 0)\n\
               export function render(i) { hsv(0, 0, 1) }";
    let prog = compile(src).unwrap();
    assert_eq!(prog.assert_msgs.len(), 2, "identical messages must dedup");
    let blob = serialize(&prog).unwrap();
    let prog2 = deserialize(&blob).unwrap();
    assert_eq!(prog.assert_msgs, prog2.assert_msgs);
    assert_eq!(blob, serialize(&prog2).unwrap());

    // a corrupted Assert message index must be a decode error, not a panic:
    // patch every 0x40-opcode operand to an out-of-range table index
    let mut evil = blob.clone();
    let mut hits = 0;
    for i in 0..evil.len() - 2 {
        if evil[i] == 0x40 && evil[i + 1] < 2 && evil[i + 2] == 0 {
            evil[i + 1] = 0xFF;
            evil[i + 2] = 0xFF;
            hits += 1;
        }
    }
    assert!(hits >= 3, "expected to find the assert instructions");
    assert!(validate(&evil).is_err(), "bad msg index must be rejected");
}

#[test]
fn const_array_dedup_keeps_identities_separate() {
    // Two identical all-numeric literals intern to ONE const-pool entry
    // but remain distinct mutable arrays (copy-on-write): writing through
    // one must never leak into the other.
    let src = r#"
var a = [1, 2, 3]
var b = [1, 2, 3]
var c = [4, -5, 3.5]
export var a0
export var b0
export var probe
a[0] = 9
a0 = a[0]
b0 = b[0]
probe = c[1]
b.mutate((v) => v * 2)      // CoW via a builtin, too
export var b0m
b0m = b[0]
export function render(i) { hsv(0, 0, 0) }
"#;
    let prog = compile(src).unwrap();
    // identical literals deduped; the distinct one adds a second entry
    assert_eq!(prog.pool.len(), 2, "expected dedup to 2 pool entries");
    // and the whole thing round-trips + runs identically from the blob
    let blob = serialize(&prog).unwrap();
    let prog2 = deserialize(&blob).unwrap();
    assert_eq!(serialize(&prog2).unwrap(), blob);
    let e = Engine::from_program(prog2, 10, 1);
    let get = |name: &str| match e.var(name) {
        Some(luxel_core::vm::Value::Num(v)) => v,
        other => panic!("{name}: {other:?}"),
    };
    assert_eq!(get("a0"), Fx::from_int(9), "write through a");
    assert_eq!(get("b0"), Fx::from_int(1), "b unaffected by a's write");
    assert_eq!(get("b0m"), Fx::from_int(2), "mutate() copied-on-write");
    assert_eq!(get("probe"), Fx::from_int(-5));
}

#[test]
fn validate_agrees_with_deserialize() {
    let prog = compile(PATTERN).unwrap();
    let blob = serialize(&prog).unwrap();
    assert!(validate(&blob).is_ok());
    // every truncation and every single-byte corruption must produce the
    // same accept/reject verdict as the allocating decoder
    for cut in 0..blob.len() {
        assert_eq!(
            validate(&blob[..cut]).is_ok(),
            deserialize(&blob[..cut]).is_ok(),
            "verdicts diverge at truncation {cut}"
        );
    }
    for i in 0..blob.len() {
        let mut b = blob.clone();
        b[i] ^= 0xFF;
        assert_eq!(
            validate(&b).is_ok(),
            deserialize(&b).is_ok(),
            "verdicts diverge at corrupted byte {i}"
        );
    }
}

#[test]
fn version_mismatch_is_distinct() {
    let prog = compile("export function render(i) { hsv(0,0,0) }").unwrap();
    let mut blob = serialize(&prog).unwrap();
    blob[4] = (FORMAT_VERSION + 1) as u8;
    match deserialize(&blob) {
        Err(BcError::Version { found }) => assert_eq!(found, FORMAT_VERSION + 1),
        other => panic!("expected Version error, got {other:?}"),
    }
}

#[test]
fn malformed_blobs_are_rejected_not_panics() {
    let prog = compile(PATTERN).unwrap();
    let blob = serialize(&prog).unwrap();

    // bad magic
    let mut b = blob.clone();
    b[0] = b'X';
    assert!(matches!(deserialize(&b), Err(BcError::Malformed(_))));

    // every truncation point must error cleanly
    for cut in 0..blob.len() {
        assert!(
            deserialize(&blob[..cut]).is_err(),
            "truncation at {cut} accepted"
        );
    }

    // single-byte corruption must never panic (error or benign decode ok)
    for i in 0..blob.len() {
        let mut b = blob.clone();
        b[i] ^= 0xFF;
        let _ = deserialize(&b);
    }

    // trailing garbage
    let mut b = blob.clone();
    b.push(0);
    assert!(matches!(deserialize(&b), Err(BcError::Malformed(_))));
}

#[test]
fn out_of_range_indices_are_rejected() {
    // hand-corrupt an export's fn index: the export table is the last
    // table before the word region, so the LAST "render" str8 in the blob
    // is the export entry and the u16 after it is its fn index
    let prog = compile("export function render(i) { hsv(0,0,0) }").unwrap();
    let mut blob = serialize(&prog).unwrap();
    let name = b"\x06render";
    let at = blob
        .windows(name.len())
        .rposition(|w| w == name)
        .expect("export name in blob");
    blob[at + name.len()] = 0xFF;
    blob[at + name.len() + 1] = 0xFF;
    assert!(matches!(deserialize(&blob), Err(BcError::Malformed(_))));
}

#[test]
fn unknown_builtin_import_names_the_culprit() {
    // patch the first import-table name ("array", from `array(8)`) into an
    // unknown one of equal length
    let prog = compile(PATTERN).unwrap();
    let blob = serialize(&prog).unwrap();
    let name = b"array";
    let at = blob
        .windows(name.len())
        .position(|w| w == name)
        .expect("import name in blob");
    let mut b = blob.clone();
    b[at..at + name.len()].copy_from_slice(b"zzzzz");
    match deserialize(&b) {
        Err(BcError::Malformed(m)) => assert!(m.contains("zzzzz"), "{m}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn static_decode_borrows_aligned_words_and_runs_identically() {
    use luxel_core::bytecode::deserialize_lean_static;
    use luxel_core::vm::Words;

    // an array literal so the const pool is exercised through the
    // borrowed word region too (LOAD_IDX on a Const-backed array, then a
    // copy-on-write promotion)
    let src = r#"
var pal = [0.1, 0.4, 0.7, 0.9]
export var probe
probe = pal[2]
export function render(index) {
  if (index == 3) pal[0] = 0.5
  hsv(pal[index % 4] + index / pixelCount, 1, 1)
}
"#;
    let prog = compile(src).unwrap();
    let blob = serialize(&prog).unwrap();

    // aligned 'static copy: Vec<u32>-backed so the pointer is 4-aligned
    let words = blob.len().div_ceil(4);
    let mut backing: Vec<u32> = vec![0; words];
    let aligned: &'static [u8] = {
        let p = backing.as_mut_ptr() as *mut u8;
        unsafe { core::ptr::copy_nonoverlapping(blob.as_ptr(), p, blob.len()) };
        let leaked = Box::leak(backing.into_boxed_slice());
        unsafe { core::slice::from_raw_parts(leaked.as_ptr() as *const u8, blob.len()) }
    };
    let p_static = deserialize_lean_static(aligned).unwrap();
    assert!(
        matches!(p_static.words, Words::Static(_)),
        "an aligned 'static blob must be borrowed, not copied"
    );
    assert_eq!(p_static.fns[0].pos.len(), 0, "lean: no debug positions");

    // unaligned 'static input: must fall back to copying, never fail
    let mut shifted = vec![0u8; blob.len() + 1];
    shifted[1..].copy_from_slice(&blob);
    let leaked: &'static [u8] = Box::leak(shifted.into_boxed_slice());
    let unaligned = &leaked[1..];
    assert_eq!(unaligned.as_ptr() as usize % 4, 1);
    let p_copy = deserialize_lean_static(unaligned).unwrap();
    assert!(matches!(p_copy.words, Words::Owned(_)));

    // both render exactly like the fully decoded program
    let mut a = Engine::from_program(deserialize(&blob).unwrap(), 12, 7);
    let mut b = Engine::from_program(p_static, 12, 7);
    let mut c = Engine::from_program(p_copy, 12, 7);
    let fa = frames(&mut a, 4);
    assert_eq!(fa, frames(&mut b, 4));
    assert_eq!(fa, frames(&mut c, 4));
    assert_eq!(
        b.var("probe"),
        Some(luxel_core::vm::Value::Num(Fx::from_f64_lit(0.7)))
    );
}

#[test]
fn builtin_id_outside_import_table_is_rejected() {
    // an instruction word calling a builtin the import table does not
    // list must fail validation even when the id itself is in range: the
    // by-name check is what makes ids trustworthy.
    // `hsv(i,i,i)`, not `hsv(0,0,0)`: constant trailing arguments fuse into
    // the CALL_BUILTIN_CC superinstruction (Gitea #261) and there would be
    // no plain 0x39 word left to corrupt.
    let prog = compile("export function render(i) { hsv(i,i,i) }").unwrap();
    let blob = serialize(&prog).unwrap();
    let hsv = luxel_core::vm::lookup_builtin("hsv").unwrap();
    let other = luxel_core::vm::lookup_builtin("rgb").unwrap();
    // find the CALL_BUILTIN hsv word (opcode 0x39, u16 id, argc 3) in the
    // 4-aligned word region
    let n = blob.len();
    let mut hit = None;
    for at in (0..n).step_by(4) {
        let w = u32::from_le_bytes(blob[at..at + 4].try_into().unwrap());
        if w as u8 == 0x39 && ((w >> 8) & 0xFFFF) as u16 == hsv && (w >> 24) == 3 {
            hit = Some(at);
        }
    }
    let at = hit.expect("CALL_BUILTIN hsv word");
    let mut evil = blob.clone();
    let w = 0x39u32 | (other as u32) << 8 | 3 << 24;
    evil[at..at + 4].copy_from_slice(&w.to_le_bytes());
    match validate(&evil) {
        Err(BcError::Malformed(m)) => assert!(m.contains("import table"), "{m}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
    // and reserved operand bits on a bare opcode are rejected too
    let mut evil2 = blob.clone();
    let ret_null = 0x3Fu32 | 1 << 20;
    evil2[n - 4..n].copy_from_slice(&ret_null.to_le_bytes());
    assert!(validate(&evil2).is_err());
}

/// `insn_count` walks a function's words the same way `validate` does, so
/// every function of a compiled program counts cleanly, an instruction is
/// never wider than the range it sits in, and the fused build never needs
/// MORE instructions than the unfused one (Gitea #312 — the static half of
/// the Pixelblaze op-count comparison, `tools/oracle/opcount.mjs`).
#[test]
fn insn_count_walks_every_function() {
    use luxel_core::bytecode::insn_count;
    use luxel_core::compile::{compile_with, CompileOpts};

    let mut totals = Vec::new();
    for superinstructions in [true, false] {
        let prog = compile_with(PATTERN, CompileOpts {
            superinstructions,
            const_folding: true,
        }).unwrap();
        let mut total = 0u32;
        for f in &prog.fns {
            let s = f.code_start as usize;
            let code = &prog.words[s..s + f.code_len as usize];
            let n = insn_count(code).expect("counts cleanly");
            assert!(n as usize <= code.len(), "more instructions than words");
            assert_eq!(n == 0, code.is_empty());
            total += n;
        }
        assert!(total > 0);
        totals.push(total);
    }
    let (fused, unfused) = (totals[0], totals[1]);
    assert!(fused <= unfused, "fusing grew the instruction count: {fused} > {unfused}");

    // An undecodable word is an error, not a silent miscount.
    assert!(insn_count(&[0x0000_00FF]).is_err());

    // So is a range that ends inside a multi-word instruction: `render` holds
    // literals, so some prefix of it must cut a CONST_NUM in half.
    let prog = compile(PATTERN).unwrap();
    let f = prog.fns.iter().find(|f| f.name == "render").unwrap();
    let s = f.code_start as usize;
    let code = &prog.words[s..s + f.code_len as usize];
    assert!(
        (1..code.len()).any(|n| insn_count(&code[..n]).is_err()),
        "no prefix of render cut an instruction — is it all single-word?",
    );
}
