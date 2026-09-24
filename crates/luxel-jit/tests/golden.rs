//! Golden bytes: the compiled image of five patterns, pinned so an
//! accidental codegen change is a visible diff rather than a silent one
//! (docs/jit-design.md §7.1, "Emitter as pure function").
//!
//! What is pinned is a HASH plus the sizes, not the byte dump: a 30 KB
//! array of hex in a source file is not reviewable, and the two numbers
//! that a reviewer can actually judge — how much native code a pattern
//! costs, and how much of it is literal pool — are exactly the ones phase
//! 4's code-cache sizing needs. When one of these fails, run
//! `cargo test -p luxel-jit --test golden -- --nocapture` and the new
//! values are printed ready to paste.
//!
//! The image is position-independent apart from the function-address
//! literals (`compile_all.rs` pins that), so these numbers are stable
//! across hosts.

mod common;

use common::{env_with_fake_addresses, library, program_of};

/// FNV-1a over the image bytes. Not cryptographic — this is a change
/// detector, and one that is trivial to recompute by hand if anyone ever
/// wants to.
fn digest(words: &[u32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for w in words {
        for b in w.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

struct Golden {
    /// `library/` file name.
    name: &'static str,
    /// Total image size in bytes, pool included.
    bytes: usize,
    /// Literal pool, in words.
    pool: u32,
    /// Functions compiled.
    fns: usize,
    digest: u64,
}

/// The five of docs/jit-design.md §7.1: two strip patterns, a 2D one, the
/// noise-heavy probe #260 uses as its stateless benchmark, and a
/// `renderFrame` pattern.
const GOLDENS: [Golden; 5] = [
    // Re-pinned for the corrected frame layout (Gitea #658): the window
    // save areas moved from `a1+0` to the top of the frame, so every
    // local, stack home and scratch slot shifted DOWN by 16 bytes. Four
    // of the five images shrank — offsets now start at 0, and more of
    // them fit `l32i.n`/`s32i.n`'s 4-bit scaled field.
    Golden { name: "rainbow.js", bytes: 160, pool: 4, fns: 2, digest: 0x18af292747f563fd },
    Golden { name: "snake.js", bytes: 1172, pool: 12, fns: 8, digest: 0xb659400b6f6cff15 },
    Golden { name: "snake-2d.js", bytes: 11256, pool: 51, fns: 19, digest: 0xd8a73584af0e02d7 },
    Golden { name: "perlin-fire-wind-tunnel.js", bytes: 2536, pool: 22, fns: 10, digest: 0x7acb31222a526137 },
    Golden { name: "bulk-canvas-ripples-2d.js", bytes: 3576, pool: 19, fns: 11, digest: 0xafeb58cfc53c5133 },
];

#[test]
fn golden_images_are_unchanged() {
    let lib = library();
    let env = env_with_fake_addresses();
    let mut fresh = String::new();
    let mut bad = Vec::new();
    for g in &GOLDENS {
        let src = lib
            .iter()
            .find(|(n, _)| n == g.name)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| panic!("library/{} is gone", g.name));
        let (prog, kinds) = program_of(&src).unwrap();
        let img = luxel_jit::compile(&prog, &kinds, &env)
            .unwrap_or_else(|e| panic!("{}: {}", g.name, e.reason()));
        let got = Golden {
            name: g.name,
            bytes: img.len_bytes(),
            pool: img.pool_len,
            fns: img.entries.len(),
            digest: digest(&img.words()),
        };
        fresh.push_str(&format!(
            "    Golden {{ name: {:?}, bytes: {}, pool: {}, fns: {}, digest: {:#018x} }},\n",
            got.name, got.bytes, got.pool, got.fns, got.digest
        ));
        if (got.bytes, got.pool, got.fns, got.digest) != (g.bytes, g.pool, g.fns, g.digest) {
            bad.push(format!(
                "  {}: {} B / pool {} / {} fns / {:#018x}  (was {} B / pool {} / {} fns / {:#018x})",
                g.name, got.bytes, got.pool, got.fns, got.digest, g.bytes, g.pool, g.fns, g.digest
            ));
        }
    }
    if !bad.is_empty() {
        eprintln!("current values:\n{fresh}");
        panic!("golden images changed:\n{}", bad.join("\n"));
    }
}

/// The number phase 4 needs: how much native code the whole library costs,
/// and the worst single pattern. Pinned loosely — this is a budget, not a
/// contract, so the assertion is a ceiling rather than an equality.
#[test]
fn library_code_size_stays_within_the_cache_budget() {
    let env = env_with_fake_addresses();
    let mut total = 0usize;
    let mut worst = (0usize, String::new());
    let mut n = 0usize;
    for (name, src) in library() {
        let (prog, kinds) = program_of(&src).unwrap();
        let Ok(img) = luxel_jit::compile(&prog, &kinds, &env) else {
            continue;
        };
        n += 1;
        total += img.len_bytes();
        if img.len_bytes() > worst.0 {
            worst = (img.len_bytes(), name.clone());
        }
    }
    eprintln!(
        "library native code: {total} B over {n} patterns, mean {} B, worst {} at {} B",
        total / n.max(1),
        worst.1,
        worst.0
    );
    assert!(
        worst.0 <= 128 * 1024,
        "{} needs {} B, over JIT_MAX_CODE",
        worst.1,
        worst.0
    );
    assert!(
        total / n.max(1) < 8 * 1024,
        "mean image grew to {} B",
        total / n.max(1)
    );
}
