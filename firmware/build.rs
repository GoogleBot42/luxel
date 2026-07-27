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
}
