//! The encoder gate of docs/jit-design.md §7.1: every form
//! `crate::xtensa` can emit is emitted into a buffer, the buffer is
//! written to a file, and the DEVSHELL'S OWN disassembler is asked what it
//! says. Mnemonic and operands are compared against what the emitter
//! claimed to encode.
//!
//! **Deviation from §7.1, and it matters.** The design names
//! `xtensa-esp-elf-objdump`. That binary is built for a *generic* Xtensa
//! configuration and it desynchronises on our byte stream — it decodes
//! `entry a1, 32` as `excw`, invents FLIX bundles and drifts by a byte,
//! because the instruction lengths of an unknown configuration are not our
//! configuration's. `xtensa-esp32s3-elf-objdump`, from the same toolchain
//! derivation, carries the ESP32-S3 config and disassembles the same bytes
//! exactly. **The S3 variant is the oracle**; §7.1 has been corrected.
//!
//! If the toolchain is missing entirely the test FAILS rather than skips:
//! an encoder no one checked is worse than no encoder.

use std::fmt::Write as _;
use std::process::Command;

use luxel_jit::xtensa::{Asm, Cond, ZCond, A1, A10, A15, A2, A3, A4, A5, A6, A8, A9};

/// One emitted instruction and the disassembly it must produce.
struct Case {
    /// `objdump`'s spelling, normalised by [`norm`].
    want: String,
    /// Byte offset the instruction starts at.
    at: usize,
}

fn norm(s: &str) -> String {
    // objdump writes `l32i a2, a3, 0x3fc` for a decimal 1020 and prints
    // `.n` suffixes; fold whitespace, lower-case hex and drop the `.n` so
    // a test can name either spelling.
    let s = s.trim().replace('\t', " ");
    let mut out = String::new();
    let mut last_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c.to_ascii_lowercase());
            last_space = false;
        }
    }
    out.trim().replace(".n ", " ").replace(".n", "")
}

/// Disassemble a raw byte buffer with the S3 objdump. Returns
/// `(byte offset, text)` per instruction, in order. `origin` is added to
/// every address (`--adjust-vma`), so a buffer that starts partway into an
/// image still reports image offsets.
fn disassemble_at(bytes: &[u8], origin: usize) -> Vec<(usize, String)> {
    let dir = std::env::temp_dir().join(format!("luxel-jit-objdump-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    // One file per call: the tests run on separate threads in one process,
    // so a shared name races (the first finisher deletes the other's
    // buffer, and the disassembly that came back was someone else's code).
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let bin = dir.join(format!("code-{n}.bin"));
    std::fs::write(&bin, bytes).expect("write the code buffer");

    let objdump = find_objdump();
    let out = Command::new(&objdump)
        .args(["-D", "-b", "binary", "-m", "xtensa", "-EL"])
        .arg(format!("--adjust-vma={origin:#x}"))
        .arg(&bin)
        .output()
        .unwrap_or_else(|e| panic!("running {objdump}: {e}"));
    assert!(
        out.status.success(),
        "{objdump} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let _ = std::fs::remove_file(&bin);

    let mut v = Vec::new();
    for line in text.lines() {
        // "   1c:\tff2322      \tl32i\ta2, a3, 0x3fc" — address, then the
        // raw-byte column, then the mnemonic and operands.
        let Some((addr, rest)) = line.split_once(':') else {
            continue;
        };
        let Ok(off) = usize::from_str_radix(addr.trim(), 16) else {
            continue;
        };
        let cols: Vec<&str> = rest.split('\t').filter(|c| !c.trim().is_empty()).collect();
        if cols.len() < 2 {
            continue;
        }
        v.push((off, cols[1..].join(" ")));
    }
    v
}

fn disassemble(bytes: &[u8]) -> Vec<(usize, String)> {
    disassemble_at(bytes, 0)
}

/// The ESP32-S3 objdump from the devshell. Absent ⇒ the test fails loudly.
fn find_objdump() -> String {
    for name in ["xtensa-esp32s3-elf-objdump", "xtensa-esp-elf-objdump"] {
        if Command::new(name)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            // The generic build desynchronises on our stream (see the
            // module comment); only accept it as a last resort so the
            // failure message is about the toolchain, not the encoder.
            return name.to_string();
        }
    }
    panic!(
        "no Xtensa objdump on PATH. This test IS the encoder's correctness \
         gate and must not be skipped: run it inside `nix develop`, which \
         carries xtensa-esp32s3-elf-objdump."
    );
}

/// Emit every form, then compare the whole buffer's disassembly in one go.
#[test]
fn every_form_matches_the_vendor_disassembler() {
    let mut a = Asm::new();
    let mut cases: Vec<Case> = Vec::new();

    // A 4-aligned literal at offset 0 for the one `l32r` below. The
    // disassembler would try to decode it as an instruction, so put the
    // l32r's literal AFTER all the code instead — see the separate l32r
    // test. Here the buffer is pure code from byte 0.
    macro_rules! case {
        ($want:expr, $emit:expr) => {{
            let at = a.here();
            #[allow(clippy::let_unit_value)]
            let _ = $emit;
            cases.push(Case {
                want: norm($want),
                at,
            });
        }};
    }

    // ---- window / calls
    case!("entry a1, 32", assert!(a.entry(A1, 32)));
    case!("entry a1, 0x7ff8", assert!(a.entry(A1, 32_760)));
    case!("retw.n", a.retw_n());
    case!("callx8 a8", a.callx8(A8));
    case!("callx8 a15", a.callx8(A15));
    case!("nop.n", a.nop_n());
    case!("nop", a.nop());

    // ---- moves
    case!("mov.n a5, a6", a.mov_n(A5, A6));
    case!("mov.n a15, a2", a.mov_n(A15, A2));
    case!("movi a3, 0xfffff800", assert!(a.movi(A3, -2048)));
    case!("movi a3, 0x7ff", assert!(a.movi(A3, 2047)));
    case!("movi a4, 0x400", assert!(a.movi(A4, 1024)));
    case!("movi.n a4, 7", assert!(a.movi_n(A4, 7)));
    case!("movi.n a4, 95", assert!(a.movi_n(A4, 95)));
    case!("movi.n a4, -32", assert!(a.movi_n(A4, -32)));
    case!("movi.n a4, -1", assert!(a.movi_n(A4, -1)));
    case!("movi.n a4, 0", assert!(a.movi_n(A4, 0)));

    // ---- arithmetic / logic
    case!("add a7, a8, a9", a.add(A6 + 1, A8, A9));
    case!("add.n a2, a3, a4", a.add_n(A2, A3, A4));
    case!("sub a2, a3, a4", a.sub(A2, A3, A4));
    case!("neg a2, a3", a.neg(A2, A3));
    case!("abs a2, a3", a.abs(A2, A3));
    case!("and a2, a3, a4", a.and(A2, A3, A4));
    case!("or a2, a3, a4", a.or(A2, A3, A4));
    case!("xor a2, a3, a4", a.xor(A2, A3, A4));
    case!("mull a2, a3, a4", a.mull(A2, A3, A4));
    case!("mulsh a2, a3, a4", a.mulsh(A2, A3, A4));
    case!("quos a2, a3, a4", a.quos(A2, A3, A4));
    case!("rems a2, a3, a4", a.rems(A2, A3, A4));
    case!("addi a2, a3, -128", assert!(a.addi(A2, A3, -128)));
    case!("addi a2, a3, 127", assert!(a.addi(A2, A3, 127)));
    case!("addi.n a2, a3, 1", assert!(a.addi_n(A2, A3, 1)));
    case!("addi.n a2, a3, 15", assert!(a.addi_n(A2, A3, 15)));
    case!("addi.n a2, a3, -1", assert!(a.addi_n(A2, A3, -1)));
    case!("addmi a2, a3, 0x400", assert!(a.addmi(A2, A3, 1024)));
    case!("addmi a2, a3, 0xffffff00", assert!(a.addmi(A2, A3, -256)));

    // ---- shifts
    case!("ssai 16", a.ssai(16));
    case!("ssai 0", a.ssai(0));
    case!("ssai 31", a.ssai(31));
    case!("ssl a3", a.ssl(A3));
    case!("ssr a3", a.ssr(A3));
    case!("src a2, a3, a4", a.src(A2, A3, A4));
    case!("sll a2, a3", a.sll(A2, A3));
    case!("sra a2, a3", a.sra(A2, A3));
    for sa in 0..32u32 {
        let want = format!("srai a2, a3, {sa}");
        case!(&want, a.srai(A2, A3, sa));
    }
    for sa in 0..16u32 {
        let want = format!("srli a2, a3, {sa}");
        case!(&want, a.srli(A2, A3, sa));
    }
    for sa in 1..32u32 {
        let want = format!("slli a2, a3, {sa}");
        case!(&want, a.slli(A2, A3, sa));
    }

    // ---- memory
    case!("l32i a2, a3, 0x3fc", assert!(a.l32i(A2, A3, 1020)));
    case!("l32i a2, a3, 0", assert!(a.l32i(A2, A3, 0)));
    case!("l32i.n a2, a3, 60", assert!(a.l32i_n(A2, A3, 60)));
    case!("l32i.n a2, a3, 0", assert!(a.l32i_n(A2, A3, 0)));
    case!("s32i a2, a3, 0x3fc", assert!(a.s32i(A2, A3, 1020)));
    case!("s32i.n a2, a3, 60", assert!(a.s32i_n(A2, A3, 60)));
    case!("s32i.n a10, a1, 16", assert!(a.s32i_n(A10, A1, 16)));

    // ---- branches. Every one targets its own address, which is the one
    // offset that is always in reach and always unambiguous in the
    // disassembly.
    for (cond, name) in [
        (Cond::Eq, "beq"),
        (Cond::Ne, "bne"),
        (Cond::Lt, "blt"),
        (Cond::Ge, "bge"),
        (Cond::Ltu, "bltu"),
        (Cond::Geu, "bgeu"),
    ] {
        let here = a.here();
        let want = format!("{name} a2, a3, {here:#x}");
        case!(&want, a.branch(cond, A2, A3, here).unwrap());
    }
    for (z, name) in [(ZCond::Eqz, "beqz"), (ZCond::Nez, "bnez")] {
        let here = a.here();
        let want = format!("{name} a2, {here:#x}");
        case!(&want, a.branch_z(z, A2, here).unwrap());
    }
    for (eq, name) in [(true, "beqi"), (false, "bnei")] {
        for v in [-1i32, 1, 8, 128, 256] {
            let here = a.here();
            let shown = if v == 256 { "0x100".to_string() } else { v.to_string() };
            let want = format!("{name} a2, {shown}, {here:#x}");
            case!(&want, a.branch_i(eq, A2, v, here).unwrap());
        }
    }
    {
        let here = a.here();
        let want = format!("j {here:#x}");
        case!(&want, a.j(here).unwrap());
    }
    // a forward `j`, patched
    {
        let at = a.j_forward();
        let target = at + 64;
        let want = format!("j {target:#x}");
        cases.push(Case { want: norm(&want), at });
        // fill to the target with nop.n so the disassembly stays in sync
        while a.here() < target {
            a.nop_n();
        }
        a.patch_j(at, target).unwrap();
    }

    let bytes = a.bytes().to_vec();
    let got = disassemble(&bytes);
    let mut by_off = std::collections::BTreeMap::new();
    for (off, text) in got {
        by_off.insert(off, norm(&text));
    }

    let mut bad = String::new();
    for c in &cases {
        match by_off.get(&c.at) {
            Some(g) if *g == c.want => {}
            Some(g) => {
                let _ = writeln!(bad, "  at {:#x}: encoder meant `{}`, objdump reads `{}`", c.at, c.want, g);
            }
            None => {
                let _ = writeln!(
                    bad,
                    "  at {:#x}: no instruction boundary there (the stream desynchronised); meant `{}`",
                    c.at, c.want
                );
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} forms disagree with xtensa-esp32s3-elf-objdump:\n{bad}",
        bad.lines().count(),
        cases.len()
    );
    // And the count, so a silently dropped case cannot pass.
    assert!(cases.len() >= 120, "only {} forms pinned", cases.len());
}

/// `l32r` separately: its literal has to sit BEHIND the instruction, and
/// the disassembler would decode a literal in the code stream as garbage.
/// So the buffer here is `[4 literal bytes][code]` and the expected
/// operand is the literal's address.
#[test]
fn l32r_reaches_backwards_only() {
    let mut a = Asm::with_pool(8);
    a.put_word(0, 0x1234_5678);
    a.put_word(4, 0xdead_beef);
    let at0 = a.here();
    a.l32r(A2, 0).unwrap();
    let at1 = a.here();
    a.l32r(A3, 4).unwrap();
    // The literal pool would be decoded as instructions, so hand objdump
    // only the code and tell it where that code lives (`--adjust-vma`), so
    // the addresses it prints ARE image offsets.
    let texts: Vec<String> = disassemble_at(&a.bytes()[8..], 8)
        .iter()
        .map(|(_, t)| norm(t))
        .collect();
    assert_eq!(texts[0], "l32r a2, 0x0", "got `{}`", texts[0]);
    assert_eq!(texts[1], "l32r a3, 0x4", "got `{}`", texts[1]);
    assert_eq!(at1 - at0, 3, "l32r is a 3-byte instruction");

    // Forward and unaligned targets are refused, not encoded.
    let mut b = Asm::with_pool(4);
    let here = b.here();
    assert!(b.l32r(A2, here + 64).is_err(), "forward l32r must be refused");
    assert!(b.l32r(A2, 2).is_err(), "unaligned l32r must be refused");
    assert!(b.l32r(A2, 0).is_ok());
}

/// Reach limits are refusals, never wrong bytes.
#[test]
fn reach_limits_are_refusals() {
    let mut a = Asm::new();
    let here = a.here();
    assert!(a.branch(Cond::Eq, A2, A3, here + 200).is_err());
    assert!(a.branch_z(ZCond::Eqz, A2, here + 200).is_ok());
    let here = a.here();
    assert!(a.branch_z(ZCond::Eqz, A2, here + 5000).is_err());
    let here = a.here();
    assert!(a.j(here + 100_000).is_ok());
    let here = a.here();
    assert!(a.j(here + 200_000).is_err());
    assert!(!a.entry(A1, 32_768), "frame over the field must be refused");
    assert!(!a.entry(A1, 12), "a frame must be a multiple of 8");
    assert!(!a.movi(A2, 2048));
    assert!(!a.movi_n(A2, 96));
    assert!(!a.l32i(A2, A3, 1024));
    assert!(!a.l32i(A2, A3, 2));
    assert!(!a.addi(A2, A3, 128));
    assert!(!a.addi_n(A2, A3, 0), "addi.n cannot encode 0");
}

/// `align4` pads to a word boundary with nothing but nops, including the
/// one-byte gap that needs five bytes of padding.
#[test]
fn align4_pads_with_nops() {
    for residue in 0..4usize {
        let mut a = Asm::new();
        // Reach each residue out of 2- and 3-byte instructions: 0 = none,
        // 2 = one nop.n, 3 = one nop, 1 = nop + nop.n (five bytes).
        match residue {
            0 => {}
            1 => {
                a.nop();
                a.nop_n();
            }
            2 => a.nop_n(),
            _ => a.nop(),
        }
        let before = a.here();
        assert_eq!(before % 4, residue);
        a.align4();
        assert_eq!(a.here() % 4, 0, "residue={residue} start={before}");
        assert!(a.here() - before <= 5);
        if a.bytes().is_empty() {
            continue; // residue 0: nothing emitted, nothing to disassemble
        }
        let text: Vec<String> = disassemble(a.bytes())
            .iter()
            .map(|(_, t)| norm(t))
            .collect();
        for t in &text {
            assert!(t.starts_with("nop"), "padding produced `{t}`");
        }
    }
}
