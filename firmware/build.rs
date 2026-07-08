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
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("default.lxbc");
    std::fs::write(out, blob).expect("write default.lxbc");
}
