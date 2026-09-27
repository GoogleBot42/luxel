//! `luxel sprite import` — the Rust half of the image → `LXSP` pipeline, and
//! the assertion that it is byte-identical to the browser's (Gitea #784).
//!
//! THE POINT of this file is the goldens. `web/tests/fixtures/` holds an image,
//! its pixels as a raw sidecar, the import options, and the record the
//! pipeline must produce. `web/tests/imageImport.test.mjs` feeds the SIDECAR to
//! the TypeScript pipeline; this feeds the IMAGE ITSELF, decoded by the `image`
//! crate, to the Rust one. Both must land on the same `.lxsp` bytes — so a
//! golden that passes on both sides also proves the two decoders agree about
//! the fixture's pixels, and either implementation drifting turns exactly one
//! of the two suites red.
//!
//! Regenerate with:
//!   node --experimental-strip-types web/tools/gen-sprite-fixtures.mjs

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "../src/spriteimport.rs"]
mod spriteimport;

use spriteimport::{
    check_record, decode_file, default_target_size, derived_fps, encode, fit_under_cap,
    import_sprite, kept_indices, nearest_index, normalized_delay, planned_bytes, resolve_options,
    sprite_name, FitMode, Options, Resample, SourceFrame, SourceImage,
};

fn fixtures() -> PathBuf {
    // crates/luxel-cli/tests → the repo root → web/tests/fixtures
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../web/tests/fixtures")
        .canonicalize()
        .expect("web/tests/fixtures must exist (gen-sprite-fixtures.mjs writes it)")
}

/// The options block a fixture's `.json` states, read without a JSON crate:
/// this is a file the generator writes with one field per line, and a test
/// that hand-rolls the six values it needs cannot drift into accepting a
/// shape the generator never writes.
struct Fixture {
    image: String,
    options: Options,
}

fn field<'a>(json: &'a str, key: &str) -> &'a str {
    let at = json.find(&format!("\"{key}\":")).unwrap_or_else(|| panic!("no {key} in the fixture"));
    let rest = &json[at + key.len() + 3..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    let end = rest.find(['"', ',', '\n', '}']).unwrap_or(rest.len());
    rest[..end].trim()
}

fn read_fixture(name: &str) -> Fixture {
    let json = std::fs::read_to_string(fixtures().join(format!("{name}.json"))).expect("fixture json");
    let mut o = Options {
        name: field(&json, "name").to_string(),
        w: field(&json, "w").parse().expect("w"),
        h: field(&json, "h").parse().expect("h"),
        fit: match field(&json, "fit") {
            "fit" => FitMode::Fit,
            "fill" => FitMode::Fill,
            "crop" => FitMode::Crop,
            other => panic!("unknown fit {other}"),
        },
        resample: match field(&json, "resample") {
            "nearest" => Resample::Nearest,
            "area" => Resample::Area,
            other => panic!("unknown resample {other}"),
        },
        colors: field(&json, "colors").parse().expect("colors"),
        alpha_threshold: field(&json, "alphaThreshold").parse().expect("alphaThreshold"),
        dither: field(&json, "dither") == "true",
        keep_every: field(&json, "keepEvery").parse().expect("keepEvery"),
        fps_override: None,
    };
    // the generator writes the OPTIONS block's w/h after the image's, so the
    // first two matches above are the image's — re-read them from the tail
    let tail = &json[json.find("\"options\"").expect("options block")..];
    o.w = field(tail, "w").parse().expect("options.w");
    o.h = field(tail, "h").parse().expect("options.h");
    let fps = field(tail, "fpsOverride");
    o.fps_override = if fps == "null" { None } else { Some(fps.parse().expect("fpsOverride")) };
    Fixture { image: field(&json, "image").to_string(), options: o }
}

/// Every fixture, decoded from its own image file and compared to the golden.
#[test]
fn the_rust_pipeline_writes_the_same_records_the_browser_does() {
    for name in ["ramp", "blob", "spin", "spin-half"] {
        let fx = read_fixture(name);
        let src = decode_file(fixtures().join(&fx.image).to_str().expect("path"))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let got = encode(&import_sprite(&src, &fx.options).sprite);
        let want = std::fs::read(fixtures().join(format!("{name}.lxsp"))).expect("golden");
        assert_eq!(check_record(&want), Ok(()), "{name}: the golden is a valid record");
        assert_eq!(
            got, want,
            "{name}: the Rust pipeline and web/src/lib/imageImport.ts disagree \
             (got {} B, golden {} B)",
            got.len(),
            want.len()
        );
    }
}

/// The decoders agree about the fixtures' pixels — asserted directly, so a
/// golden mismatch above can be read as a PIPELINE difference rather than as a
/// decode difference.
#[test]
fn the_image_crate_decodes_the_fixtures_to_their_raw_sidecars() {
    for name in ["ramp", "blob", "spin"] {
        let fx = read_fixture(name);
        let src = decode_file(fixtures().join(&fx.image).to_str().expect("path"))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let raw = std::fs::read(fixtures().join(format!("{name}.rgba"))).expect("sidecar");
        let per = src.w * src.h * 4;
        assert_eq!(raw.len(), per * src.frames.len(), "{name}: frame count");
        for (i, f) in src.frames.iter().enumerate() {
            assert_eq!(f.rgba, raw[i * per..(i + 1) * per], "{name}: frame {i} pixels");
        }
    }
}

/// The report an import comes back with, which is what the CLI prints and the
/// console's readout draws — every field, on the animated fixture.
#[test]
fn an_import_reports_what_it_did_to_the_frames_and_the_palette() {
    let fx = read_fixture("spin-half");
    let src = decode_file(fixtures().join(&fx.image).to_str().unwrap()).expect("decode");
    let r = import_sprite(&src, &fx.options);
    assert_eq!(r.sprite.frames, 2, "4 source frames, one kept in two");
    assert_eq!(r.skipped_frames, 2);
    assert_eq!(r.dropped_frames, 0, "nothing hit the 255-frame limit");
    assert_eq!(r.colors_used, 4);
    assert_eq!(r.colors_seen, 4, "the whole palette survived");
    assert_eq!(r.fps, 5, "10 fps sampled one in two");
    assert_eq!(r.bytes, encode(&r.sprite).len());
    assert!(!r.over_cap);

    // …and the 255-frame limit is REPORTED rather than applied quietly
    let many = SourceImage {
        w: 1,
        h: 1,
        frames: (0..300).map(|_| SourceFrame { rgba: vec![9, 9, 9, 255], delay_ms: 100 }).collect(),
    };
    let o = resolve_options(&Options { w: 1, h: 1, fit: FitMode::Fill, ..Options::default() });
    let r = import_sprite(&many, &o);
    assert_eq!(r.sprite.frames, 255);
    assert_eq!(r.dropped_frames, 45);
}

#[test]
fn the_gif_fixture_carries_its_frame_rate() {
    let src = decode_file(fixtures().join("spin.gif").to_str().unwrap()).expect("decode");
    assert_eq!(src.frames.len(), 4);
    for f in &src.frames {
        assert_eq!(normalized_delay(f.delay_ms), 100, "100 ms a frame");
    }
    assert_eq!(derived_fps(&src, 1, 4), 10);
    assert_eq!(derived_fps(&src, 2, 2), 5, "keeping one in two halves the rate");
    assert_eq!(kept_indices(4, 2), vec![0, 2]);
}

#[test]
fn a_name_comes_from_the_file_name() {
    assert_eq!(sprite_name("heart.png"), "heart");
    assert_eq!(sprite_name("/tmp/a b/My Sprite.GIF"), "My Sprite");
    assert_eq!(sprite_name("C:\\art\\x.webp"), "x");
    assert_eq!(sprite_name(".png"), "Sprite");
    assert_eq!(sprite_name(""), "Sprite");
    assert_eq!(sprite_name(&format!("{}.png", "x".repeat(80))), "x".repeat(64));
}

#[test]
fn the_default_target_scales_the_long_edge_to_64() {
    assert_eq!(default_target_size(320, 240, None), (64, 48));
    assert_eq!(default_target_size(240, 320, None), (48, 64));
    assert_eq!(default_target_size(16, 16, None), (16, 16));
    assert_eq!(default_target_size(320, 240, Some((32, 32))), (32, 24));
    assert_eq!(default_target_size(320, 240, Some((128, 128))), (64, 48));
    assert_eq!(default_target_size(1, 4000, None), (1, 64));
}

#[test]
fn an_over_cap_import_is_reported_and_the_knobs_fix_it() {
    let src = SourceImage {
        w: 64,
        h: 64,
        frames: (0..40)
            .map(|i| SourceFrame { rgba: vec![200 + (i % 40) as u8; 64 * 64 * 4], delay_ms: 100 })
            .collect(),
    };
    let o = resolve_options(&Options {
        name: String::from("big"),
        w: 64,
        h: 64,
        fit: FitMode::Fill,
        ..Options::default()
    });
    let r = import_sprite(&src, &o);
    assert!(r.over_cap, "{} B", r.bytes);
    assert_eq!(r.sprite.frames, 40, "nothing was quietly dropped");
    let fix = fit_under_cap(&src, &o, 16 * 1024);
    assert!(!fix.said.is_empty());
    let fixed = import_sprite(&src, &fix.options);
    assert!(!fixed.over_cap, "{} B after: {}", fixed.bytes, fix.said);
    assert_eq!(check_record(&encode(&fixed.sprite)), Ok(()));
}

#[test]
fn planned_bytes_matches_what_is_written() {
    let o = resolve_options(&Options {
        name: String::from("abc"),
        w: 5,
        h: 7,
        fit: FitMode::Fill,
        colors: 4,
        ..Options::default()
    });
    let src = SourceImage {
        w: 5,
        h: 7,
        frames: vec![SourceFrame {
            rgba: (0..5 * 7)
                .flat_map(|i: usize| [(i * 7) as u8, 255 - (i * 7) as u8, 128, 255])
                .collect(),
            delay_ms: 0,
        }],
    };
    let r = import_sprite(&src, &o);
    assert_eq!(planned_bytes(&o, 1), r.bytes);
}

#[test]
fn nearest_index_breaks_ties_towards_the_lower_entry() {
    let pal = [[0, 0, 0], [10, 0, 0], [0, 0, 0]];
    assert_eq!(nearest_index(&pal, 5, 0, 0), 1);
    assert_eq!(nearest_index(&pal, 10, 0, 0), 2);
    assert_eq!(nearest_index(&[], 1, 2, 3), 0);
}

/// The CLI end to end: the subcommand writes the golden, and refuses a file
/// that is not an image rather than writing something broken.
#[test]
fn the_subcommand_writes_the_record_and_refuses_a_non_image() {
    let exe = env!("CARGO_BIN_EXE_luxel");
    let dir = std::env::temp_dir().join(format!("luxel-sprite-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let out = dir.join("spin.lxsp");

    let st = Command::new(exe)
        .args([
            "sprite",
            "import",
            fixtures().join("spin.gif").to_str().unwrap(),
            "--size",
            "8x8",
            "--fit",
            "fit",
            "--resample",
            "nearest",
            "--colors",
            "255",
            "--name",
            "spin",
            "-o",
            out.to_str().unwrap(),
        ])
        .status()
        .expect("run luxel");
    assert!(st.success(), "sprite import exited {st:?}");
    let got = std::fs::read(&out).expect("the record");
    let want = std::fs::read(fixtures().join("spin.lxsp")).expect("golden");
    assert_eq!(got, want, "the subcommand's record is the golden");

    // a text file is refused, with a reason and a non-zero status
    let bad = dir.join("not-an-image.txt");
    std::fs::write(&bad, b"this is not a PNG\n").expect("write");
    let st = Command::new(exe)
        .args(["sprite", "import", bad.to_str().unwrap(), "-o", out.to_str().unwrap()])
        .output()
        .expect("run luxel");
    assert!(!st.status.success(), "a text file must not import");
    let said = String::from_utf8_lossy(&st.stderr);
    assert!(said.contains("not an image"), "stderr said: {said}");

    let _ = std::fs::remove_dir_all(&dir);
}
