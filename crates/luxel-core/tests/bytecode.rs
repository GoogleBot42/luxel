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
    assert_eq!(prog.data_arrays.len(), 2, "expected dedup to 2 pool entries");
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
    // hand-corrupt an export's fn index (last 2 bytes of the last export)
    let prog = compile("export function render(i) { hsv(0,0,0) }").unwrap();
    let mut blob = serialize(&prog).unwrap();
    let n = blob.len();
    blob[n - 2] = 0xFF;
    blob[n - 1] = 0xFF;
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
