//! Compile the built-in default pattern (library/rainbow.js) to LXBC at
//! build time. The firmware links no compiler — it boots straight into the
//! precompiled blob (main.rs includes OUT_DIR/default.lxbc).

fn main() {
    // Which pattern boots. `LUXEL_DEFAULT_PATTERN` is a path relative to
    // firmware/ (or absolute); unset means `library/rainbow.js`, which is
    // what every shipped image carries.
    //
    // It exists because the pattern a device boots into is the ONLY one
    // some environments can select: tools/qemu/jit-test.py drives the
    // emulator, where the network never comes up and the playlist task
    // never runs, so the boot default is the whole reachable corpus
    // (Gitea #658). A general knob, not an emulator one — nothing in the
    // firmware ever asks whether it was set.
    println!("cargo:rerun-if-env-changed=LUXEL_DEFAULT_PATTERN");
    let src_path =
        std::env::var("LUXEL_DEFAULT_PATTERN").unwrap_or_else(|_| "../library/rainbow.js".into());
    println!("cargo:rerun-if-changed={src_path}");
    let src = std::fs::read_to_string(&src_path)
        .unwrap_or_else(|e| panic!("read default pattern {src_path}: {e}"));
    let prog = match luxel_core::compile::compile(&src) {
        Ok(p) => p,
        Err(d) => panic!("default pattern {src_path} does not compile: {}", d.message),
    };
    let blob = luxel_core::bytecode::serialize(&prog).expect("serialize default pattern");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("default.lxbc"), blob).expect("write default.lxbc");
    // The SOURCE too, so main.rs includes the file this bytecode was
    // compiled from rather than naming `library/rainbow.js` a second time
    // — two independent paths to the same fact is a drift waiting to
    // happen, and with the override above it would be a live one.
    std::fs::write(out_dir.join("default.js"), &src).expect("write default.js");

    // Partition table binary for src/parttab.rs — the WLED takeover
    // (src/takeover.rs) and the layout migrator (src/migrate.rs) both write
    // it: the exact bytes a serial flash puts at 0x8000, entries plus the
    // trailing MD5 row the bootloader verifies. esp-idf-part is the same
    // crate espflash uses, so the output matches byte for byte.
    //
    // WHICH table is a property of the BOARD, not of the environment: the
    // Seengreat panel driver carries an ESP32-S3-WROOM-1-N16R8 (16 MB), so it
    // gets the 16 MB layout; everything else — board-s3-devkit included, since
    // a generic S3 devkit may be a 4 MB part and a 16 MB table would brick it
    // (see partitions-16mb.csv) — gets the 4 MB one. Keyed off the cargo
    // feature so it cannot disagree with the image it is compiled into, and
    // so flake.nix / build-esp32.sh passing the wrong --partition-table shows
    // up as a mismatch rather than as a device that silently OTAs into a slot
    // that is not there. firmware/board-target.sh's `board_partitions` is the
    // shell-side copy of this same map.
    println!("cargo:rerun-if-changed=partitions.csv");
    println!("cargo:rerun-if-changed=partitions-16mb.csv");
    // (csv name, flash size, expected end of the last partition — see
    // `serialize_table` for why the end is declared rather than derived).
    let big = std::env::var_os("CARGO_FEATURE_BOARD_SEENGREAT_HUB75").is_some();
    let (partitions, flash_size, expect_end) = if big {
        ("partitions-16mb.csv", 0x100_0000u32, 0xE0_0000u32)
    } else {
        ("partitions.csv", 0x40_0000u32, 0x40_0000u32)
    };
    serialize_table(&out_dir, "partition-table.bin", partitions, flash_size, expect_end);
    // So the image can print the table it embeds (firmware/src).
    println!("cargo::rustc-env=LUXEL_PARTITIONS={partitions}");

    // A board on the 16 MB table embeds the 4 MB one as well (Gitea #634).
    //
    // `g_rom_flashchip.chip_size` comes from the BOOTLOADER's image header,
    // and an OTA replaces the app but never the bootloader — so a 16 MB
    // board serially flashed back when its table was 4 MB has a ROM that
    // bounds-checks every flash op at 4 MB and a bootloader that refuses to
    // boot under a table reaching past it. Such a board migrates to the
    // LARGEST embedded table its bootloader can back (src/parttab.rs
    // `target_table`) instead of refusing outright, and migrates again to
    // the big one after the bootloader is re-flashed over serial.
    //
    // Cost is one 0x100-byte table plus the selection branch, and it is
    // keyed off the same board feature as the table itself, so every 4 MB
    // board's image is byte-identical to what it was.
    println!("cargo::rustc-check-cfg=cfg(fallback_table)");
    if big {
        serialize_table(
            &out_dir,
            "partition-table-fallback.bin",
            "partitions.csv",
            0x40_0000,
            0x40_0000,
        );
        println!("cargo::rustc-env=LUXEL_PARTITIONS_FALLBACK=partitions.csv");
        println!("cargo:rustc-cfg=fallback_table");
    }

    build_index_html(&out_dir);

    // Dual-core chips get the second-core render executor and the flash
    // fence (src/core1.rs). Mirrors esp-hal's own `multi_core` cfg, which
    // is private to the esp-* crates' build scripts.
    println!("cargo::rustc-check-cfg=cfg(multi_core)");
    let multi_core = std::env::var_os("CARGO_FEATURE_ESP32").is_some()
        || std::env::var_os("CARGO_FEATURE_ESP32S3").is_some();
    if multi_core {
        println!("cargo:rustc-cfg=multi_core");
    }

    // The frame pipeline (src/pipeline.rs): the compose + output stages run
    // on the ProCpu while the AppCpu renders the next frame, so the frame
    // period is max(vm, out) instead of vm + out. Only worth it where the
    // output stage costs real CPU on a core that has nothing else to do -
    // the HUB75 bitplane compose is a flat ~6 ms at 4096 px. Strips at
    // 60-2048 px are wire-bound (SPI DMA), so they keep the single-task
    // path and pay neither the extra frame buffer nor the hand-off.
    println!("cargo::rustc-check-cfg=cfg(pipelined)");
    if multi_core && std::env::var_os("CARGO_FEATURE_HUB75").is_some() {
        println!("cargo:rustc-cfg=pipelined");
    }

    // Boards with a SECOND physical strip output get a second driver
    // instance (src/output.rs, Gitea #474): another SPI peripheral, another
    // encode buffer, another run of the one pixel space. Only the Athom has
    // one today. Gated so every other board's image is byte-identical —
    // board::OUTPUTS is the same fact and board.rs asserts the two agree.
    println!("cargo::rustc-check-cfg=cfg(multi_output)");
    if std::env::var_os("CARGO_FEATURE_BOARD_ATHOM_MUSIC").is_some() {
        println!("cargo:rustc-cfg=multi_output");
    }
}

/// Serialize one `firmware/partitions*.csv` into `OUT_DIR/<name>` as the exact
/// bytes espflash would write at 0x8000 — entries plus the trailing MD5 row the
/// bootloader verifies.
///
/// `expect_end` is declared rather than derived: a fat-fingered offset or size
/// in the csv then fails HERE, at build time, instead of on a device that can no
/// longer be reached over the network. The 4 MB table fills its part exactly;
/// the 16 MB one deliberately leaves its top 2 MiB unallocated.
fn serialize_table(
    out_dir: &std::path::Path,
    name: &str,
    partitions: &str,
    flash_size: u32,
    expect_end: u32,
) {
    let csv = std::fs::read_to_string(partitions)
        .unwrap_or_else(|e| panic!("read {partitions}: {e}"));
    let table = esp_idf_part::PartitionTable::try_from(csv)
        .unwrap_or_else(|e| panic!("parse {partitions}: {e}"));
    let end = table
        .partitions()
        .iter()
        .map(|p| p.offset() + p.size())
        .max()
        .expect("empty partition table");
    assert!(
        end <= flash_size,
        "{partitions} runs past the end of flash: last byte {end:#x} > {flash_size:#x}"
    );
    assert_eq!(
        end, expect_end,
        "{partitions} ends at {end:#x}, not the {expect_end:#x} this board expects — \
         if the layout really changed, update the constant in firmware/build.rs"
    );
    let bin = table.to_bin().expect("serialize partition table");
    assert_eq!(bin[0..2], [0xAA, 0x50], "unexpected partition-table binary layout");
    assert!(
        bin.windows(2).any(|w| w == [0xEB, 0xEB]),
        "partition-table binary missing its MD5 row"
    );
    std::fs::write(out_dir.join(name), bin).unwrap_or_else(|e| panic!("write {name}: {e}"));
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
