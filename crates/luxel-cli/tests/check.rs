//! `luxel check` as the pre-upload gate for a big rig (Gitea #420).
//!
//! Three parallel `array(pixelCount)` channels fit a strip and blow the
//! PB-compat element ledger on a 64x64 panel, where the pattern loads and
//! renders BLACK. `check --grid 64x64` is the host-side way to find that
//! before the upload, so it has to fail loudly and say by how much — and it
//! reports the ledger reading on the passing rigs too, so the distance from
//! the wall is visible rather than implied.

use std::process::Command;

/// The #420 shape: one buffer per colour channel, sized from `pixelCount`.
const THREE_CHANNELS: &str = "export var r = array(pixelCount)\n\
                              export var g = array(pixelCount)\n\
                              export var b = array(pixelCount)\n\
                              export function beforeRender(delta) { r[0] = delta }\n\
                              export function render(i) { rgb(r[i], g[i], b[i]) }\n";

fn pattern_file(name: &str, src: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("luxel-check-{}-{}.js", std::process::id(), name));
    std::fs::write(&p, src).expect("write pattern");
    p
}

fn check(path: &std::path::Path, rig: &[&str]) -> (bool, serde_json::Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_luxel"))
        .arg("check")
        .arg(path)
        .args(rig)
        .output()
        .expect("run luxel check");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("{e}: {stdout}"));
    (out.status.success(), v)
}

#[test]
fn a_64x64_grid_fails_loudly_and_says_by_how_much() {
    let p = pattern_file("three-channels", THREE_CHANNELS);
    let (ok, v) = check(&p, &["--grid", "64x64"]);
    assert!(!ok, "check must fail on the panel rig: {v}");
    assert_eq!(v["stage"], "init");
    let msg = v["error"].as_str().expect("error message");
    // the diagnosis, not just the fact
    assert!(msg.contains("array element budget exceeded"), "{msg}");
    assert!(msg.contains("4096-element array"), "{msg}");
    assert!(msg.contains("4100"), "{msg}"); // what it needed
    assert!(msg.contains("10236"), "{msg}"); // the budget
    assert!(msg.contains("2036 left"), "{msg}"); // what was left
    // the ledger reading, so "how close was it" needs no second run
    assert_eq!(v["arrayElems"], 8200);
    assert_eq!(v["arrayBudget"], 10236);
    let _ = std::fs::remove_file(&p);
}

#[test]
fn the_rigs_the_library_sweep_runs_still_pass_and_report_the_headroom() {
    // check-library.sh's rigs are all comfortably under the wall — which is
    // exactly why the sweep never caught this. The numbers make that visible.
    let p = pattern_file("headroom", THREE_CHANNELS);
    for (rig, elems) in [
        (vec![], 312),                        // check's own 10x10
        (vec!["--grid", "16x16"], 780),       // the sweep's second grid
        (vec!["--strip", "300"], 912),        // the mapless strips
        (vec!["--strip", "512"], 1548),       //
        (vec!["--grid", "32x32"], 3084),      // still fits
    ] {
        let (ok, v) = check(&p, &rig);
        assert!(ok, "{rig:?} should pass: {v}");
        assert_eq!(v["stage"], "ok");
        assert_eq!(v["arrayElems"], elems, "{rig:?}: {v}");
        assert_eq!(v["arrayBudget"], 10236);
    }
    let _ = std::fs::remove_file(&p);
}
