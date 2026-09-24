//! Every `library/*.js` goes through the emitter. Nothing is executed
//! here — `library_diff.rs` does that — but this is where a panic, a
//! scratch-register overflow or an unhandled opcode shows up, and it is
//! where the phase-4 code-cache sizing numbers come from.

mod common;

use common::{env_with_fake_addresses, library, program_of};

#[test]
fn every_library_pattern_compiles() {
    let env = env_with_fake_addresses();
    let mut refusals: Vec<(String, String)> = Vec::new();
    let mut total = 0usize;
    let mut worst = (0usize, String::new());
    let mut n = 0usize;
    let mut sizes: Vec<usize> = Vec::new();

    for (name, src) in library() {
        let (prog, kinds) = program_of(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        match luxel_jit::compile(&prog, &kinds, &env) {
            Ok(img) => {
                n += 1;
                total += img.len_bytes();
                sizes.push(img.len_bytes());
                if img.len_bytes() > worst.0 {
                    worst = (img.len_bytes(), name.clone());
                }
                assert_eq!(img.bytes.len() % 4, 0);
                assert!(
                    img.pool_len as usize * 4 <= img.len_bytes(),
                    "{name}: pool larger than the image"
                );
                // Every entry is 4-aligned and inside the image (`entry`
                // and `callx8` targets must be, §3.7).
                for (i, e) in img.entries.iter().enumerate() {
                    assert_eq!(*e % 4, 0, "{name}: fn {i} entry {e} is not 4-aligned");
                    assert!((*e as usize) < img.len_bytes(), "{name}: fn {i} entry past the end");
                    assert!(
                        *e >= img.pool_len * 4,
                        "{name}: fn {i} entry is inside the literal pool"
                    );
                }
                for (ename, off) in &img.exports {
                    assert!(
                        (*off as usize) < img.len_bytes(),
                        "{name}: export {ename} past the end"
                    );
                }
            }
            Err(r) => refusals.push((name.clone(), r.reason())),
        }
    }

    eprintln!(
        "compiled {n}/{} patterns, {total} B total, largest {} at {} B",
        n + refusals.len(),
        worst.1,
        worst.0
    );
    // The DISTRIBUTION, not just the total: an exec buffer is sized per
    // board (firmware/src/jit.rs `JIT_STATIC_KB`) and what matters there
    // is how much of the library a given cap covers, which a mean skewed
    // by one 32 KB outlier does not say. Phase 4's code cache wants the
    // same numbers. Printed, never asserted — pinning a percentile would
    // make every codegen improvement a test failure.
    sizes.sort_unstable();
    let pct = |p: usize| sizes[(sizes.len() * p / 100).min(sizes.len() - 1)];
    eprintln!(
        "image size: p50 {} B, p75 {} B, p90 {} B, p99 {} B, mean {} B",
        pct(50),
        pct(75),
        pct(90),
        pct(99),
        total / n.max(1)
    );
    for cap in [2048usize, 4096, 7168, 8192, 16384, 32768] {
        let fit = sizes.iter().filter(|s| **s <= cap).count();
        eprintln!(
            "  a {:>5} B buffer holds {fit}/{n} ({} %)",
            cap,
            fit * 100 / n.max(1)
        );
    }
    assert!(
        refusals.is_empty(),
        "{} patterns refused:\n{}",
        refusals.len(),
        refusals
            .iter()
            .map(|(n, r)| format!("  {n}: {r}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The image is position-independent except for the function-address
/// literals, so moving `code_base` must change exactly those words and
/// nothing else. That is the property phase 3's "write through the DBUS
/// alias, execute through the IBUS one" placement depends on.
#[test]
fn only_fn_address_literals_depend_on_code_base() {
    let (prog, kinds) = program_of(
        "export function render(i) { hsv(i, 1, helper(i)) }\n\
         function helper(x) { return x * 2 }\n",
    )
    .unwrap();
    let mut a = env_with_fake_addresses();
    a.code_base = 0x4000_0000;
    let mut b = env_with_fake_addresses();
    b.code_base = 0x4100_0000;
    let ia = luxel_jit::compile(&prog, &kinds, &a).unwrap();
    let ib = luxel_jit::compile(&prog, &kinds, &b).unwrap();
    let (wa, wb) = (ia.words(), ib.words());
    assert_eq!(wa.len(), wb.len());
    assert_eq!(ia.entries, ib.entries);
    let differ: Vec<usize> = (0..wa.len()).filter(|&i| wa[i] != wb[i]).collect();
    assert!(!differ.is_empty(), "the callee address must be in the pool");
    for i in differ {
        assert!(
            i < ia.pool_len as usize,
            "word {i} is code and must not depend on code_base"
        );
        assert_eq!(wb[i] - wa[i], 0x0100_0000);
    }
}

/// `max_code` is a refusal, not a truncation.
#[test]
fn over_the_cap_is_a_refusal() {
    let (prog, kinds) = program_of("export function render(i) { hsv(i, 1, 1) }").unwrap();
    let mut env = env_with_fake_addresses();
    env.max_code = 8;
    let r = luxel_jit::compile(&prog, &kinds, &env).unwrap_err();
    assert_eq!(r.id(), "too-large", "{}", r.reason());
}

/// Every refusal the backend can produce maps cleanly onto the editor's
/// lint vocabulary (`luxel_core::jitlint::JitRefusal::Backend`), so phase
/// 3 can surface one without inventing wording.
#[test]
fn refusals_carry_into_the_editor_lint() {
    use luxel_core::jitlint::JitRefusal;
    let (prog, kinds) = program_of("export function render(i) { hsv(i, 1, 1) }").unwrap();
    let mut env = env_with_fake_addresses();
    env.max_code = 8;
    let r = luxel_jit::compile(&prog, &kinds, &env).unwrap_err();
    let (fn_idx, word) = r.site();
    let lint = JitRefusal::Backend {
        id: r.id(),
        detail: r.reason(),
        fn_idx,
        word,
        line: 0,
        col: 0,
    };
    assert_eq!(lint.id(), "too-large");
    assert!(lint.text().contains("over the 8 B cap"), "{}", lint.text());
    assert_eq!(lint.pos(), (0, 0));

    // And a helper in the wrong gigabyte, which is the refusal the ISA
    // model found (`retw` restores only 30 address bits).
    let mut bad = env_with_fake_addresses();
    bad.helpers.fx_div = 0x1000_0000;
    let r = luxel_jit::compile(&prog, &kinds, &bad).unwrap_err();
    assert_eq!(r.id(), "address-region", "{}", r.reason());
    assert!(r.reason().contains("fx_div"), "{}", r.reason());
}
