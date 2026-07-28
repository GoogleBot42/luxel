//! Pack a gallery pattern into an LXP1 envelope (what POST /api/code takes).
//!
//!   cargo run -p luxel-cli --example mkenvelope -- "<gallery name substring>" out.lxp1

fn main() {
    let mut args = std::env::args().skip(1);
    let sel = args.next().expect("pattern name substring");
    let out = args.next().expect("output path");

    // "@path" packs a local source file instead of a gallery entry.
    if let Some(path) = sel.strip_prefix('@') {
        let source = std::fs::read_to_string(path).expect("source file");
        let prog = luxel_core::compile::compile(&source).expect("compiles");
        let blob = luxel_core::bytecode::serialize(&prog).unwrap();
        let env = luxel_core::bytecode::encode_envelope("Rainbow", &source, &blob);
        std::fs::write(&out, &env).unwrap();
        eprintln!("{path}: envelope {} B -> {out}", env.len());
        return;
    }

    let gallery = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/public/gallery.json"),
    )
    .expect("web/public/gallery.json");
    let gallery: serde_json::Value = serde_json::from_str(&gallery).unwrap();
    let p = gallery
        .as_array()
        .unwrap()
        .iter()
        .find(|p| {
            p["name"].as_str().unwrap().to_lowercase().contains(&sel.to_lowercase())
        })
        .unwrap_or_else(|| panic!("no gallery pattern matching {sel:?}"));
    let name = p["name"].as_str().unwrap();
    let source = p["source"].as_str().unwrap();

    let prog = luxel_core::compile::compile(source).expect("compiles");
    let blob = luxel_core::bytecode::serialize(&prog).unwrap();
    let env = luxel_core::bytecode::encode_envelope(name, source, &blob);
    std::fs::write(&out, &env).unwrap();
    eprintln!(
        "{name}: source {} B, bytecode {} B, envelope {} B -> {out}",
        source.len(),
        blob.len(),
        env.len()
    );
}
