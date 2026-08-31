//! Compile the built-in default pattern (library/rainbow.js) to LXBC at
//! build time. The firmware links no compiler — it boots straight into the
//! precompiled blob (main.rs includes OUT_DIR/default.lxbc).

fn main() {
    let src_path = "../library/rainbow.js";
    println!("cargo:rerun-if-changed={src_path}");
    let src = std::fs::read_to_string(src_path).expect("read default pattern");
    let prog = match luxel_core::compile::compile(&src) {
        Ok(p) => p,
        Err(d) => panic!("default pattern does not compile: {}", d.message),
    };
    let blob = luxel_core::bytecode::serialize(&prog).expect("serialize default pattern");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("default.lxbc"), blob).expect("write default.lxbc");

    // Partition table binary for the wled-takeover feature (src/takeover.rs):
    // the exact bytes a serial flash puts at 0x8000 — entries plus the
    // trailing MD5 row the bootloader verifies. esp-idf-part is the same
    // crate espflash uses, so the output matches byte for byte.
    println!("cargo:rerun-if-changed=partitions.csv");
    let csv = std::fs::read_to_string("partitions.csv").expect("read partitions.csv");
    let table = esp_idf_part::PartitionTable::try_from(csv).expect("parse partitions.csv");
    let bin = table.to_bin().expect("serialize partition table");
    assert_eq!(bin[0..2], [0xAA, 0x50], "unexpected partition-table binary layout");
    assert!(
        bin.windows(2).any(|w| w == [0xEB, 0xEB]),
        "partition-table binary missing its MD5 row"
    );
    std::fs::write(out_dir.join("partition-table.bin"), bin).expect("write partition-table.bin");

    build_index_html(&out_dir);
}

/// Resolve the embedded fallback page's build-mode blocks (src/index.html,
/// served at `/min` and at `/` when no playground is installed).
///
/// The page differs by one paragraph between the normal build (the UI *can*
/// be installed onto the device — tell the reader how) and a `hosted-ui`
/// build (there is no `POST /api/assets` route at all, so that instruction
/// would be a lie). Keeping ONE html file and stripping the block that does
/// not apply avoids a second near-identical page drifting out of sync.
///
/// Syntax, deliberately dumber than a template engine — literal line-anchored
/// markers, no nesting: `#if assets` … `#endif` is kept unless `hosted-ui`,
/// `#if hosted` … `#endif` only with it (each wrapped in an html comment on a
/// line of its own). Whole-line html comments are dropped too, so the page
/// can carry build notes for free.
fn build_index_html(out_dir: &std::path::Path) {
    println!("cargo:rerun-if-changed=src/index.html");
    let src = std::fs::read_to_string("src/index.html").expect("read src/index.html");
    let hosted = std::env::var_os("CARGO_FEATURE_HOSTED_UI").is_some();

    let mut out = String::with_capacity(src.len());
    // None = emitting; Some(keep) = inside a block we are keeping/dropping
    let mut block: Option<bool> = None;
    let mut in_comment = false;
    let mut seen = 0usize;
    for line in src.lines() {
        let t = line.trim();
        if in_comment {
            in_comment = !t.ends_with("-->");
            continue;
        }
        match t {
            "<!--#if assets-->" | "<!--#if hosted-->" => {
                assert!(block.is_none(), "src/index.html: nested #if block");
                seen += 1;
                block = Some((t == "<!--#if hosted-->") == hosted);
                continue;
            }
            "<!--#endif-->" => {
                assert!(block.is_some(), "src/index.html: stray #endif");
                block = None;
                continue;
            }
            _ => {}
        }
        // whole-line comment (possibly multi-line) — a build-time note
        if t.starts_with("<!--") {
            in_comment = !t.ends_with("-->");
            continue;
        }
        if block.unwrap_or(true) {
            out.push_str(line);
            out.push('\n');
        }
    }
    assert!(block.is_none(), "src/index.html: unterminated #if block");
    assert!(!in_comment, "src/index.html: unterminated html comment");
    // If the markers are ever dropped, a hosted-ui image would silently ship
    // "install the UI with tools/deploy.sh" on a device that has no
    // /api/assets route. Fail the build instead.
    assert!(
        seen >= 2,
        "src/index.html lost its #if assets / #if hosted blocks ({seen} found)"
    );
    std::fs::write(out_dir.join("index.html"), out).expect("write index.html");
}
