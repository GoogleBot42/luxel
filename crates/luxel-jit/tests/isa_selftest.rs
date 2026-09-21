//! The ISA model's own tests: does `tests/isa/mod.rs` execute what
//! `luxel_jit::xtensa::Asm` encodes?
//!
//! Every test here builds its code with the ENCODER, never with
//! hand-written bytes, so the model is judged against the same assembler
//! that will feed it in the differential gate (docs/jit-design.md §7.1).
//! The encoder is itself pinned to the vendor disassembler by
//! `tests/objdump.rs`, so a green run of both means: these bytes are those
//! instructions, and those instructions do this.
//!
//! The reference for every arithmetic form is a Rust expression computed
//! in the test, not a table of expected words — a model that agreed with a
//! table someone typed would only prove the typing.

#[path = "isa/mod.rs"]
mod isa;

use isa::{Cpu, Trap, CODE_BASE, STACK_TOP};
use luxel_jit::xtensa::{
    Asm, Cond, Reg, ZCond, A0, A1, A10, A11, A12, A13, A14, A15, A2, A3, A4, A5, A6, A7, A8, A9,
    B4CONST,
};

/// The frame [`exec`] opens. A multiple of 16, as §3.3 requires.
const FRAME: u32 = 32;
/// Scratch data, far from the code image and the stack.
const DATA: u32 = 0x3fc0_0000;

/// Operands worth trying on every form: the edges signed arithmetic gets
/// wrong, plus a couple of 16.16 values.
const OPS: [i32; 10] = [
    0,
    1,
    -1,
    2,
    i32::MIN,
    i32::MAX,
    0x10000,
    -0x10000,
    123_456,
    -7_654_321,
];

/// Assemble `entry a1, FRAME` + `body` + `retw.n` at image offset 0 and
/// boot a Cpu on it with `regs` preloaded.
fn build(body: impl FnOnce(&mut Asm), regs: &[(Reg, u32)]) -> Cpu {
    let mut a = Asm::new();
    assert!(a.entry(A1, FRAME));
    body(&mut a);
    a.retw_n();
    let mut cpu = Cpu::boot(a.bytes(), 0);
    for &(r, v) in regs {
        cpu.set_ar(r, v);
    }
    cpu
}

/// [`build`] plus "run it to the `retw.n`"; the registers are then the
/// function's final state, because a `retw.n` with no outstanding call
/// halts WITHOUT rotating the window back.
fn exec(regs: &[(Reg, u32)], body: impl FnOnce(&mut Asm)) -> Cpu {
    let mut cpu = build(body, regs);
    match cpu.run(100_000) {
        Ok(_) => cpu,
        Err(e) => panic!("the snippet trapped: {e:?}"),
    }
}

/// [`build`] plus "run it and expect a trap".
fn exec_trap(regs: &[(Reg, u32)], body: impl FnOnce(&mut Asm)) -> Trap {
    let mut cpu = build(body, regs);
    match cpu.run(100_000) {
        Ok(h) => panic!("expected a trap, halted with {h:?}"),
        Err(e) => e,
    }
}

// ------------------------------------------------------------ the frame

#[test]
fn entry_opens_the_frame_and_retw_halts_at_depth_zero() {
    let mut cpu = build(|_| {}, &[]);
    cpu.step().expect("entry");
    // `entry a1, 32` (§3.3): a1 drops by the frame size, nothing else
    // moves, and the window has not rotated (no call is outstanding).
    assert_eq!(cpu.ar(A1), STACK_TOP - FRAME);
    assert_eq!(cpu.window_base(), 0);
    assert_eq!(cpu.depth(), 0);
    let done = cpu.run(10).expect("retw.n halts");
    assert_eq!(done.steps, 1, "only the retw.n was left");
    assert_eq!(cpu.ar(A1), STACK_TOP - FRAME, "retw.n at depth 0 rotates nothing");
}

#[test]
fn nops_execute_and_align4_padding_is_transparent() {
    for residue in 0..4usize {
        let cpu = exec(&[(A3, 7)], move |a| {
            match residue {
                0 => {}
                1 => {
                    a.nop();
                    a.nop_n();
                }
                2 => a.nop_n(),
                _ => a.nop(),
            }
            a.align4();
            a.add_n(A4, A3, A3);
        });
        assert_eq!(cpu.ar(A4), 14, "residue {residue}");
    }
}

// ------------------------------------------------------- moves, movi

#[test]
fn moves_and_immediates() {
    // movi's full 12-bit range, movi.n's 7-bit one, and the chooser.
    for imm in [-2048i32, -129, -33, -32, -1, 0, 1, 95, 96, 2047] {
        let cpu = exec(&[], move |a| {
            assert!(a.movi(A4, imm));
            if (-32..=95).contains(&imm) {
                assert!(a.movi_n(A5, imm));
            }
            assert!(a.movi_best(A6, imm));
            a.mov_n(A7, A4);
        });
        assert_eq!(cpu.ar(A4) as i32, imm, "movi {imm}");
        if (-32..=95).contains(&imm) {
            assert_eq!(cpu.ar(A5) as i32, imm, "movi.n {imm}");
        }
        assert_eq!(cpu.ar(A6) as i32, imm, "movi_best {imm}");
        assert_eq!(cpu.ar(A7) as i32, imm, "mov.n {imm}");
    }
}

// -------------------------------------------------- arithmetic / logic

#[test]
fn arithmetic_and_logic_match_a_rust_reference() {
    for &x in &OPS {
        for &y in &OPS {
            let cpu = exec(&[(A3, x as u32), (A4, y as u32)], |a| {
                a.add(A5, A3, A4);
                a.sub(A6, A3, A4);
                a.and(A7, A3, A4);
                a.or(A8, A3, A4);
                a.xor(A9, A3, A4);
                a.add_n(A10, A3, A4);
                a.neg(A11, A3);
                a.abs(A12, A3);
            });
            let m = format!("x={x} y={y}");
            assert_eq!(cpu.ar(A5), x.wrapping_add(y) as u32, "add {m}");
            assert_eq!(cpu.ar(A6), x.wrapping_sub(y) as u32, "sub {m}");
            assert_eq!(cpu.ar(A7), (x & y) as u32, "and {m}");
            assert_eq!(cpu.ar(A8), (x | y) as u32, "or {m}");
            assert_eq!(cpu.ar(A9), (x ^ y) as u32, "xor {m}");
            assert_eq!(cpu.ar(A10), x.wrapping_add(y) as u32, "add.n {m}");
            // Both wrap: -i32::MIN and |i32::MIN| are i32::MIN.
            assert_eq!(cpu.ar(A11), x.wrapping_neg() as u32, "neg {m}");
            assert_eq!(cpu.ar(A12), x.wrapping_abs() as u32, "abs {m}");
        }
    }
}

#[test]
fn addi_addi_n_and_addmi_wrap() {
    for &x in &OPS {
        for imm in [-128i32, -1, 0, 1, 15, 127] {
            let cpu = exec(&[(A3, x as u32)], move |a| {
                assert!(a.addi(A4, A3, imm));
                if imm != 0 && (-1..=15).contains(&imm) {
                    assert!(a.addi_n(A5, A3, imm));
                }
            });
            assert_eq!(cpu.ar(A4), x.wrapping_add(imm) as u32, "addi {x}+{imm}");
            if imm != 0 && (-1..=15).contains(&imm) {
                assert_eq!(cpu.ar(A5), x.wrapping_add(imm) as u32, "addi.n {x}+{imm}");
            }
        }
        for imm in [-32768i32, -256, 0, 256, 32512] {
            let cpu = exec(&[(A3, x as u32)], move |a| {
                assert!(a.addmi(A4, A3, imm));
            });
            assert_eq!(cpu.ar(A4), x.wrapping_add(imm) as u32, "addmi {x}+{imm}");
        }
    }
}

#[test]
fn mull_and_mulsh_are_the_halves_of_the_signed_product() {
    for &x in &OPS {
        for &y in &OPS {
            let cpu = exec(&[(A3, x as u32), (A4, y as u32)], |a| {
                a.mull(A5, A3, A4);
                a.mulsh(A6, A3, A4);
            });
            let p = (x as i64).wrapping_mul(y as i64);
            assert_eq!(cpu.ar(A5), p as u32, "mull {x}*{y}");
            assert_eq!(cpu.ar(A6), ((p as u64) >> 32) as u32, "mulsh {x}*{y}");
        }
    }
}

#[test]
fn the_16_16_multiply_sequence_is_exact() {
    // docs/jit-design.md §3.5, the `Mul` row:
    //   mull t, a, b; mulsh r, a, b; ssai 16; src r, r, t
    // is the exact (a*b) >> 16 of the 64-bit product, including the cases
    // where that product does not fit in 32 bits.
    let vals: [i32; 12] = [
        0,
        1 << 16,
        -(1 << 16),
        1,
        -1,
        i32::MIN,
        i32::MAX,
        3 << 16,
        (0.5f32 * 65536.0) as i32,
        -12345,
        1_000_000,
        -999_999,
    ];
    for &x in &vals {
        for &y in &vals {
            let cpu = exec(&[(A3, x as u32), (A4, y as u32)], |a| {
                a.mull(A6, A3, A4);
                a.mulsh(A5, A3, A4);
                assert!(a.ssai(16));
                a.src(A5, A5, A6);
            });
            let want = (((x as i64) * (y as i64)) >> 16) as i32;
            assert_eq!(cpu.ar(A5) as i32, want, "fx mul {x}*{y}");
        }
    }
}

#[test]
fn quos_and_rems_truncate_toward_zero() {
    for &x in &OPS {
        for &y in &OPS {
            if y == 0 {
                continue;
            }
            let cpu = exec(&[(A3, x as u32), (A4, y as u32)], |a| {
                a.quos(A5, A3, A4);
                a.rems(A6, A3, A4);
            });
            // i32::MIN / -1 wraps to i32::MIN on this ISA, which is what
            // wrapping_div does.
            assert_eq!(cpu.ar(A5), x.wrapping_div(y) as u32, "quos {x}/{y}");
            assert_eq!(cpu.ar(A6), x.wrapping_rem(y) as u32, "rems {x}%{y}");
        }
    }
}

#[test]
fn a_zero_divisor_traps() {
    let t = exec_trap(&[(A3, 7), (A4, 0)], |a| a.quos(A5, A3, A4));
    assert!(matches!(t, Trap::DivideByZero { .. }), "{t:?}");
    let t = exec_trap(&[(A3, 7), (A4, 0)], |a| a.rems(A5, A3, A4));
    assert!(matches!(t, Trap::DivideByZero { .. }), "{t:?}");
}

// -------------------------------------------------------------- shifts

#[test]
fn constant_shifts_match_rust() {
    for &x in &OPS {
        for sa in 0..32u32 {
            let cpu = exec(&[(A3, x as u32)], move |a| {
                assert!(a.srai(A4, A3, sa));
                if sa < 16 {
                    assert!(a.srli(A5, A3, sa));
                }
                if sa >= 1 {
                    assert!(a.slli(A6, A3, sa));
                }
            });
            assert_eq!(cpu.ar(A4), (x >> sa) as u32, "srai {x} >> {sa}");
            if sa < 16 {
                assert_eq!(cpu.ar(A5), (x as u32) >> sa, "srli {x} >>u {sa}");
            }
            if sa >= 1 {
                assert_eq!(cpu.ar(A6), (x as u32) << sa, "slli {x} << {sa}");
            }
        }
    }
}

#[test]
fn variable_shift_sequences_cover_counts_0_and_31() {
    // The §3.5 `Shl`/`Shr` rows: the count is a 16.16 number, so the
    // sequence shifts it down first. Count 0 is the trap — `ssl` leaves
    // SAR = 32 there, and `sll` must then shift by zero, not by 32.
    for &x in &OPS {
        for n in 0..32u32 {
            let rhs = (n << 16) as i32;
            let cpu = exec(&[(A3, x as u32), (A4, rhs as u32)], |a| {
                assert!(a.srai(A5, A4, 16));
                a.ssl(A5);
                a.sll(A6, A3);
                a.ssr(A5);
                a.sra(A7, A3);
            });
            assert_eq!(cpu.ar(A6), (x as u32) << n, "shl {x} << {n}");
            assert_eq!(cpu.ar(A7), (x >> n) as u32, "shr {x} >> {n}");
        }
    }
}

#[test]
fn src_is_a_funnel_shift_including_sar_0_and_32() {
    let hi = 0x1234_5678u32;
    let lo = 0x9abc_def0u32;
    let both = ((hi as u64) << 32) | lo as u64;
    for sar in 0..32u32 {
        let cpu = exec(&[(A3, hi), (A4, lo)], move |a| {
            assert!(a.ssai(sar));
            a.src(A5, A3, A4);
        });
        assert_eq!(cpu.ar(A5), (both >> sar) as u32, "src by {sar}");
    }
    // SAR = 32 is only reachable through `ssl` of a multiple of 32, and
    // the funnel then yields the high word untouched.
    let cpu = exec(&[(A3, hi), (A4, lo), (A6, 0)], |a| {
        a.ssl(A6);
        a.src(A5, A3, A4);
    });
    assert_eq!(cpu.ar(A5), hi, "src by SAR=32");
}

// -------------------------------------------------------------- l32r

#[test]
fn l32r_loads_a_literal_from_the_pool() {
    // The §3.7 layout: pool first, code after, `l32r` reaching backwards.
    let mut a = Asm::with_pool(8);
    a.put_word(0, 0xdead_beef);
    a.put_word(4, 0x0001_0000);
    let code = a.here();
    assert!(a.entry(A1, FRAME));
    a.l32r(A4, 0).unwrap();
    a.l32r(A5, 4).unwrap();
    a.retw_n();
    let mut cpu = Cpu::boot(a.bytes(), code);
    cpu.run(100).expect("ran");
    assert_eq!(cpu.ar(A4), 0xdead_beef);
    assert_eq!(cpu.ar(A5), 0x0001_0000);
}

#[test]
fn l32r_reaches_past_the_first_32k_words() {
    // The imm16 field is a ONE-EXTENDED negative word offset, not an i16:
    // beyond 128 KB the two readings diverge in sign, and only the
    // one-extended one matches the encoder (which masks `dist / 4` to 16
    // bits) and the hardware. A pool this far from the code is exactly the
    // §3.7 arrangement at the §5 size cap.
    let pool = 200_000usize;
    let mut a = Asm::with_pool(pool);
    a.put_word(0, 0xcafe_f00d);
    let code = a.here();
    assert!(a.entry(A1, FRAME));
    a.l32r(A4, 0).unwrap();
    a.retw_n();
    // Sanity: the encoded word offset is past i16's range, so a decoder
    // that sign-extended imm16 would read this as a FORWARD reference.
    assert!(code / 4 > 32_768, "the literal must be more than 128 KB back");
    let mut cpu = Cpu::boot(a.bytes(), code);
    cpu.run(100).expect("ran");
    assert_eq!(cpu.ar(A4), 0xcafe_f00d);
}

// ------------------------------------------------------------ memory

#[test]
fn l32i_and_s32i_round_trip_at_every_offset_form() {
    for off in [0u32, 4, 60, 64, 1020] {
        let cpu = exec(&[(A3, DATA), (A4, 0xdead_beef)], move |a| {
            assert!(a.s32i(A4, A3, off));
            assert!(a.l32i(A5, A3, off));
            if off <= 60 {
                // The narrow forms cover 0..=60 only.
                assert!(a.s32i_n(A4, A3, off));
                assert!(a.l32i_n(A6, A3, off));
            }
            // And the chooser the emitter actually calls.
            assert!(a.store(A4, A3, off));
            assert!(a.load(A7, A3, off));
        });
        assert_eq!(cpu.ar(A5), 0xdead_beef, "l32i @{off}");
        assert_eq!(cpu.ar(A7), 0xdead_beef, "load @{off}");
        if off <= 60 {
            assert_eq!(cpu.ar(A6), 0xdead_beef, "l32i.n @{off}");
        }
        assert_eq!(cpu.mem.read32(DATA + off).unwrap(), 0xdead_beef);
        // Nothing splashed into the neighbouring words.
        assert_eq!(cpu.mem.read32(DATA + off + 4).unwrap(), 0);
    }
}

#[test]
fn a_misaligned_base_traps_rather_than_splitting_the_access() {
    let t = exec_trap(&[(A3, DATA + 2)], |a| {
        assert!(a.l32i(A5, A3, 0));
    });
    assert!(matches!(t, Trap::Unaligned { addr, .. } if addr == DATA + 2), "{t:?}");
    let t = exec_trap(&[(A3, DATA + 1), (A4, 1)], |a| {
        assert!(a.s32i_n(A4, A3, 4));
    });
    assert!(matches!(t, Trap::Unaligned { .. }), "{t:?}");
}

// ----------------------------------------------------------- branches

/// Every two-register branch, taken and not taken, forward. The branch
/// jumps over a two-byte `movi.n`, so `a4 == 0` means "taken".
#[test]
fn two_register_branches_forward() {
    let pairs: [(u32, u32); 7] = [
        (5, 5),
        (5, 6),
        (6, 5),
        (0, u32::MAX),
        (u32::MAX, 0),
        (i32::MIN as u32, 1),
        (1, i32::MIN as u32),
    ];
    let conds = [
        (Cond::Eq, "beq"),
        (Cond::Ne, "bne"),
        (Cond::Lt, "blt"),
        (Cond::Ge, "bge"),
        (Cond::Ltu, "bltu"),
        (Cond::Geu, "bgeu"),
    ];
    for (x, y) in pairs {
        for (cond, name) in conds {
            let want = match cond {
                Cond::Eq => x == y,
                Cond::Ne => x != y,
                Cond::Lt => (x as i32) < (y as i32),
                Cond::Ge => (x as i32) >= (y as i32),
                Cond::Ltu => x < y,
                Cond::Geu => x >= y,
            };
            let cpu = exec(&[(A2, x), (A3, y), (A4, 0)], move |a| {
                let at = a.here();
                // Target: past the 3-byte branch and the 2-byte movi.n.
                a.branch(cond, A2, A3, at + 5).unwrap();
                assert!(a.movi_n(A4, 1));
            });
            assert_eq!(
                cpu.ar(A4) == 0,
                want,
                "{name} {x:#x},{y:#x} (a4={})",
                cpu.ar(A4)
            );
            // And the inverse condition must do the opposite — the
            // property `Cond::invert` promises the forward-branch shape.
            let cpu = exec(&[(A2, x), (A3, y), (A4, 0)], move |a| {
                let at = a.here();
                a.branch(cond.invert(), A2, A3, at + 5).unwrap();
                assert!(a.movi_n(A4, 1));
            });
            assert_eq!(cpu.ar(A4) == 0, !want, "inverted {name} {x:#x},{y:#x}");
        }
    }
}

/// A backward branch closing a loop, which is how the emitter compiles a
/// back-edge (§3.7).
#[test]
fn a_backward_branch_closes_a_loop() {
    let cpu = exec(&[(A5, 0), (A6, 7)], |a| {
        let top = a.here();
        assert!(a.addi_n(A5, A5, 1));
        a.branch(Cond::Lt, A5, A6, top).unwrap();
    });
    assert_eq!(cpu.ar(A5), 7, "the loop ran to the bound");
}

#[test]
fn beqz_and_bnez_both_directions() {
    for v in [0u32, 1, u32::MAX] {
        for (z, name) in [(ZCond::Eqz, "beqz"), (ZCond::Nez, "bnez")] {
            let want = match z {
                ZCond::Eqz => v == 0,
                ZCond::Nez => v != 0,
            };
            let cpu = exec(&[(A2, v), (A4, 0)], move |a| {
                let at = a.here();
                a.branch_z(z, A2, at + 5).unwrap();
                assert!(a.movi_n(A4, 1));
            });
            assert_eq!(cpu.ar(A4) == 0, want, "{name} {v:#x}");
        }
    }
    // Backward: count down to zero with `bnez`.
    let cpu = exec(&[(A5, 5)], |a| {
        let top = a.here();
        assert!(a.addi_n(A5, A5, -1));
        a.branch_z(ZCond::Nez, A5, top).unwrap();
    });
    assert_eq!(cpu.ar(A5), 0);
}

#[test]
fn beqi_and_bnei_use_the_b4const_table() {
    for &k in B4CONST.iter() {
        for v in [k, k.wrapping_add(1), 0, -1] {
            for eq in [true, false] {
                let want = if eq { v == k } else { v != k };
                let cpu = exec(&[(A2, v as u32), (A4, 0)], move |a| {
                    let at = a.here();
                    a.branch_i(eq, A2, k, at + 5).unwrap();
                    assert!(a.movi_n(A4, 1));
                });
                assert_eq!(
                    cpu.ar(A4) == 0,
                    want,
                    "{} a2={v}, {k}",
                    if eq { "beqi" } else { "bnei" }
                );
            }
        }
    }
}

#[test]
fn j_jumps_both_directions() {
    // Forward, including the patched form the emitter uses for a label it
    // has not bound yet.
    let cpu = exec(&[(A4, 0), (A5, 0)], |a| {
        let site = a.j_forward();
        assert!(a.movi_n(A4, 1));
        let after = a.here();
        a.patch_j(site, after).unwrap();
        assert!(a.movi_n(A5, 2));
    });
    assert_eq!(cpu.ar(A4), 0, "the skipped movi.n did not run");
    assert_eq!(cpu.ar(A5), 2, "execution resumed at the patch target");

    // Backward, closed by a forward conditional so it terminates.
    let cpu = exec(&[(A4, 0), (A5, 3)], |a| {
        let top = a.here();
        assert!(a.addi_n(A4, A4, 1));
        let at = a.here();
        // bge a4, a5, done  — done is past the 3-byte bge and the 3-byte j
        a.branch(Cond::Ge, A4, A5, at + 6).unwrap();
        a.j(top).unwrap();
    });
    assert_eq!(cpu.ar(A4), 3);
}

/// The placeholder/patch forms rewrite bytes of an already-emitted
/// instruction, which is the easiest place in the encoder to corrupt a
/// neighbouring field (`patch_branch_z` shares a byte with `s`). Executing
/// the patched code is the check that nothing was clobbered.
#[test]
fn patched_forward_branches_execute() {
    let mut a = Asm::with_pool(4);
    let code = a.here();
    assert!(a.entry(A1, FRAME));
    // Each branch is taken, so each `movi.n` below it must be skipped.
    let b0 = a.branch_forward(Cond::Lt, A2, A3);
    assert!(a.movi_n(A6, 1));
    a.patch_branch(b0, a.here()).unwrap();
    let b1 = a.branch_z_forward(ZCond::Eqz, A4);
    assert!(a.movi_n(A7, 1));
    a.patch_branch_z(b1, a.here()).unwrap();
    let b2 = a.branch_i_forward(true, A5, 8);
    assert!(a.movi_n(A8, 1));
    a.patch_branch_i(b2, a.here()).unwrap();
    let l = a.l32r_placeholder(A9);
    a.patch_l32r(l, 0).unwrap();
    a.retw_n();
    a.put_word(0, 0x5150_4c51);

    let mut cpu = Cpu::boot(a.bytes(), code);
    for (r, v) in [(A2, 1u32), (A3, 2), (A4, 0), (A5, 8), (A6, 0), (A7, 0), (A8, 0)] {
        cpu.set_ar(r, v);
    }
    cpu.run(100).expect("ran");
    assert_eq!(cpu.ar(A6), 0, "patched blt was not taken");
    assert_eq!(cpu.ar(A7), 0, "patched beqz was not taken");
    assert_eq!(cpu.ar(A8), 0, "patched beqi was not taken");
    assert_eq!(cpu.ar(A9), 0x5150_4c51, "patched l32r loaded the wrong word");
    // And the registers the patches share bytes with survived.
    assert_eq!(cpu.ar(A2), 1);
    assert_eq!(cpu.ar(A4), 0);
    assert_eq!(cpu.ar(A5), 8);
}

// ------------------------------------------------------ the call window

/// The §3.2 contract, two levels deep: the callee sees the caller's
/// `a10..a15` as `a2..a7`, gets `a1 = caller a1 - frame` and a return
/// address in `a0`, its own `a2..a7` survive its own calls, and the
/// caller's `a0..a7` come back untouched.
#[test]
fn the_window_contract_holds_two_levels_deep() {
    let mut a = Asm::with_pool(8);
    let main_off = a.here();
    assert!(a.entry(A1, 32));
    // Sentinels in the caller's own (callee-view) registers.
    for (r, v) in [(A2, 21), (A3, 22), (A4, 23), (A5, 24), (A6, 25), (A7, 26)] {
        assert!(a.movi_n(r, v));
    }
    // Arguments go in a10.. — the callee's a2...
    assert!(a.movi_n(A10, 0));
    assert!(a.movi_n(A11, 7));
    assert!(a.movi_n(A12, 9));
    // `l32r a8, lit; callx8 a8` is the emitter's call idiom (§3.1). a8 is
    // also where callx8 puts the return address, so the ISA's read-then-
    // write order is load-bearing here.
    a.l32r(A8, 0).unwrap();
    a.callx8(A8);
    a.mov_n(A14, A10); // the result, caller view
    a.retw_n();

    a.align4();
    let f1_off = a.here();
    assert!(a.entry(A1, 48));
    // Prove the frame: store f1's own a1 just above the 16-byte save area.
    assert!(a.s32i_n(A1, A1, 16));
    // Clobber every caller-saved register before setting up the call, so
    // a leak from f2 into f1's a2..a7 would show up.
    for r in [A8, A9, A10, A11, A12, A13, A14, A15] {
        assert!(a.movi_n(r, 55));
    }
    a.l32r(A8, 4).unwrap();
    a.mov_n(A10, A2);
    a.mov_n(A11, A3);
    a.mov_n(A12, A4);
    a.callx8(A8);
    // Stash the raw result, then fold in a3/a4 — which must still be the
    // 7 and 9 f1 was called with.
    assert!(a.s32i_n(A10, A1, 20));
    a.add_n(A2, A10, A3);
    a.add_n(A2, A2, A4);
    a.retw_n();

    a.align4();
    let f2_off = a.here();
    assert!(a.entry(A1, 16));
    a.add_n(A2, A3, A4);
    a.retw_n();

    a.put_word(0, CODE_BASE + f1_off as u32);
    a.put_word(4, CODE_BASE + f2_off as u32);

    let mut cpu = Cpu::boot(a.bytes(), main_off);
    cpu.run(1000).expect("ran to main's retw.n");

    let main_sp = STACK_TOP - 32;
    let f1_sp = main_sp - 48;
    // f2 returned 7 + 9; f1 added its own surviving a3/a4 on top.
    assert_eq!(cpu.mem.read32(f1_sp + 20).unwrap(), 16, "f2's result");
    assert_eq!(cpu.ar(A14), 16 + 7 + 9, "f1's a3/a4 survived its call");
    // The caller's a0..a7 are exactly what they were. (a8..a15 are
    // clobbered by design: a8 takes the return address, a9 the callee's
    // stack pointer and a10/a11 the results.)
    assert_eq!(cpu.ar(A0), 0, "main never wrote a0 and neither did the call");
    assert_eq!(cpu.ar(A1), main_sp, "main's stack pointer came back");
    for (r, v) in [(A2, 21u32), (A3, 22), (A4, 23), (A5, 24), (A6, 25), (A7, 26)] {
        assert_eq!(cpu.ar(r), v, "caller register a{r} was clobbered");
    }
    // f1's frame was the caller's minus its own frame size.
    assert_eq!(cpu.mem.read32(f1_sp + 16).unwrap(), f1_sp, "f1's a1");
    assert_eq!(cpu.window_base(), 0, "the window rotated all the way back");
    assert_eq!(cpu.depth(), 0);
}

#[test]
fn nothing_writes_the_window_save_area() {
    // §3.3 reserves 16 bytes at a1+0 for the window spill the hardware
    // would do; the emitter never touches them and neither may the model.
    let mut a = Asm::with_pool(4);
    let main_off = a.here();
    assert!(a.entry(A1, 32));
    assert!(a.movi_n(A11, 3));
    assert!(a.movi_n(A12, 4));
    a.l32r(A8, 0).unwrap();
    a.callx8(A8);
    a.retw_n();
    a.align4();
    let f_off = a.here();
    assert!(a.entry(A1, 16));
    a.add_n(A2, A3, A4);
    a.retw_n();
    a.put_word(0, CODE_BASE + f_off as u32);

    let mut cpu = Cpu::boot(a.bytes(), main_off);
    let main_sp = STACK_TOP - 32;
    let f_sp = main_sp - 16;
    cpu.mem.watch(main_sp, main_sp + 16);
    cpu.mem.watch(f_sp, f_sp + 16);
    cpu.run(1000).expect("ran");
    assert_eq!(cpu.ar(A10), 7, "the call still worked");
    assert!(
        cpu.mem.watch_hits.is_empty(),
        "writes landed in a save area: {:x?}",
        cpu.mem.watch_hits
    );
}

#[test]
fn nesting_past_max_depth_is_refused_not_spilled() {
    // An unconditionally self-recursive function: the guard is the only
    // thing that stops it.
    let mut a = Asm::with_pool(4);
    let f_off = a.here();
    assert!(a.entry(A1, 32));
    a.l32r(A8, 0).unwrap();
    a.callx8(A8);
    a.retw_n();
    a.put_word(0, CODE_BASE + f_off as u32);

    for limit in [1usize, 2, 4] {
        let mut cpu = Cpu::boot(a.bytes(), f_off);
        cpu.max_depth = limit;
        let t = cpu.run(1000).unwrap_err();
        match t {
            Trap::WindowOverflow { depth, .. } => {
                assert_eq!(depth, limit + 1, "max_depth {limit}")
            }
            other => panic!("max_depth {limit}: expected an overflow, got {other:?}"),
        }
    }
}

// -------------------------------------------------------- native calls

#[test]
fn a_native_call_marshals_arguments_and_results() {
    // The address of a Rust helper: not in the image, and nothing the
    // model could decode (§3.5 — every builtin and helper is reached this
    // way).
    const NATIVE: u32 = 0x4008_0abc;
    let mut a = Asm::with_pool(4);
    let main_off = a.here();
    assert!(a.entry(A1, FRAME));
    assert!(a.movi_n(A12, 7));
    assert!(a.movi_n(A13, 9));
    a.l32r(A8, 0).unwrap();
    a.callx8(A8);
    a.mov_n(A4, A10);
    a.retw_n();
    a.put_word(0, NATIVE);

    let mut cpu = Cpu::boot(a.bytes(), main_off);
    cpu.add_native(NATIVE);
    let t = cpu.run(1000).unwrap_err();
    assert_eq!(t, Trap::NativeCall(NATIVE));
    assert_eq!(cpu.pending_native(), Some(NATIVE));

    // Callee view: the caller's a10..a15 are a2..a7, so the 7 and 9 the
    // caller put in a12/a13 are the helper's third and fourth words.
    let args = cpu.native_args();
    assert_eq!(args[2], 7, "a4 (caller's a12)");
    assert_eq!(args[3], 9, "a5 (caller's a13)");
    // The window rotated, and a1 was staged for the helper.
    assert_eq!(cpu.window_base(), 2, "callx8 rotates eight registers");
    assert_eq!(cpu.ar(A1), STACK_TOP - FRAME);
    // An unserviced call re-traps instead of decoding Rust as Xtensa.
    assert_eq!(cpu.step(), Err(Trap::NativeCall(NATIVE)));

    cpu.return_from_native(16, 0);
    assert_eq!(cpu.window_base(), 0, "the return rotated back");
    assert_eq!(cpu.ar(A10), 16, "the result is the caller's a10");
    let done = cpu.run(1000).expect("resumed after the helper");
    assert_eq!(cpu.ar(A4), 16, "and the code after the call saw it");
    assert_eq!(cpu.depth(), 0);
    assert!(done.steps >= 2);
}

#[test]
fn a_native_call_can_return_two_words() {
    // A `Ret2`/`RetDyn` helper returns in a2:a3, which the caller reads as
    // a10:a11 (§3.2).
    const NATIVE: u32 = 0x4008_1000;
    let mut a = Asm::with_pool(4);
    let main_off = a.here();
    assert!(a.entry(A1, FRAME));
    a.l32r(A8, 0).unwrap();
    a.callx8(A8);
    a.add_n(A4, A10, A11);
    a.retw_n();
    a.put_word(0, NATIVE);

    let mut cpu = Cpu::boot(a.bytes(), main_off);
    cpu.add_native(NATIVE);
    assert_eq!(cpu.run(1000).unwrap_err(), Trap::NativeCall(NATIVE));
    cpu.return_from_native(40, 2);
    cpu.run(1000).expect("resumed");
    assert_eq!(cpu.ar(A10), 40);
    assert_eq!(cpu.ar(A11), 2);
    assert_eq!(cpu.ar(A4), 42);
}

// ------------------------------------------------------- the guardrails

#[test]
fn the_step_limit_fires_instead_of_looping_forever() {
    let mut cpu = build(
        |a| {
            let top = a.here();
            a.j(top).unwrap();
        },
        &[],
    );
    assert_eq!(cpu.run(100), Err(Trap::StepLimit));
    assert_eq!(cpu.steps, 100, "the budget is instructions executed");
    // And it is per-call, so a harness can keep going with the same number.
    assert_eq!(cpu.run(50), Err(Trap::StepLimit));
    assert_eq!(cpu.steps, 150);
}

#[test]
fn unknown_words_trap_instead_of_panicking() {
    // Unmapped memory reads as zero, so running into nothing decodes to
    // nothing.
    let mut cpu = Cpu::boot(&[], 0);
    assert_eq!(
        cpu.step(),
        Err(Trap::UnknownInstruction {
            pc: CODE_BASE,
            word: 0
        })
    );

    // A form the encoder deliberately cannot emit: `callx12 a0`, which is
    // `nop`'s word with the r nibble cleared (xtensa.rs's `Asm::nop`
    // comment). Poked in by hand BECAUSE the encoder refuses to build it.
    let mut cpu = build(|a| a.nop(), &[]);
    cpu.step().expect("entry");
    let at = cpu.pc;
    let b1 = cpu.mem.read8(at + 1);
    cpu.mem.write8(at + 1, b1 & 0x0f);
    assert!(matches!(cpu.step(), Err(Trap::UnknownInstruction { .. })));
}
