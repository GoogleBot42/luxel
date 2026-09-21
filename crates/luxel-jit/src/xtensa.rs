//! A hand-written Xtensa LX7 encoder — exactly the forms
//! docs/jit-design.md §3.5 needs, and nothing else.
//!
//! **Every form here is pinned by `tests/objdump.rs`**, which emits it into
//! a buffer and disassembles the buffer with the devshell's
//! `xtensa-esp32s3-elf-objdump -D -b binary -m xtensa -EL`. That vendor
//! disassembler is the oracle for the whole "did I get the bit layout
//! right" question; nothing in this file was trusted to a manual.
//!
//! # Encoding shapes
//!
//! Xtensa is little-endian with 24-bit and 16-bit ("density", `.n`) forms
//! packed byte-wise, so the code stream is BYTES and an instruction is
//! never aligned. A 24-bit instruction's fields, numbering the
//! instruction's bits from its first byte:
//!
//! ```text
//!   bits  3..0   op0        bits 11..8   s
//!   bits  7..4   t          bits 15..12  r
//!   bits 19..16  op1        bits 23..20  op2
//! ```
//!
//! and a 16-bit one uses `op0` (3..0), `t` (7..4), `s` (11..8), `r`
//! (15..12). Immediate forms overlay those fields: `RRI8` puts an 8-bit
//! immediate in 23..16 and uses `r` as a sub-opcode, `BRI12` an
//! (11..0)-shifted 12-bit one, `CALL` an 18-bit one in 23..6.
//!
//! # Addressing
//!
//! Branch, jump and `l32r` targets are BYTE OFFSETS INTO THE IMAGE
//! (`NativeImage::words` viewed as bytes), and so is [`Asm::here`]: the
//! whole image is one buffer whose first bytes are the literal pool
//! (§3.7), so an offset and an address differ only by `Env::code_base`,
//! which is required to be 4-aligned. That keeps the encoder free of
//! absolute addresses and makes the golden tests position-independent.

use alloc::vec::Vec;

// ------------------------------------------------------------- registers

/// An address register, `0..=15` (the CALLEE's view — see §3.2 for what
/// the window rotation does to these on a `callx8`).
pub type Reg = u8;

pub const A0: Reg = 0;
pub const A1: Reg = 1;
pub const A2: Reg = 2;
pub const A3: Reg = 3;
pub const A4: Reg = 4;
pub const A5: Reg = 5;
pub const A6: Reg = 6;
pub const A7: Reg = 7;
pub const A8: Reg = 8;
pub const A9: Reg = 9;
pub const A10: Reg = 10;
pub const A11: Reg = 11;
pub const A12: Reg = 12;
pub const A13: Reg = 13;
pub const A14: Reg = 14;
pub const A15: Reg = 15;

// ------------------------------------------------------------ reach errors

/// A target the chosen instruction form cannot reach. The emitter turns
/// this into a [`crate::Refusal`]; it is never a panic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfReach {
    /// Byte offset of the instruction.
    pub at: usize,
    /// Byte offset of the target.
    pub target: usize,
    /// Signed distance the form would have had to encode.
    pub distance: isize,
    /// Which form ran out of reach.
    pub form: Form,
}

/// Which instruction form a [`OutOfReach`] is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// `beq`/`bne`/`blt`/`bge`/`bltu`/`bgeu`/`beqi`/`bnei` — ±128 B.
    Bri8,
    /// `beqz`/`bnez` — ±2 KB.
    Bri12,
    /// `j` — ±128 KB.
    Jump,
    /// `l32r` — 256 KB BACKWARDS ONLY, 4-aligned.
    L32r,
}

// ------------------------------------------------------ condition codes

/// A two-register conditional branch. The discriminant IS the `r` field of
/// the `BRI8` encoding (op0 = 7), pinned by the objdump test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Cond {
    Eq = 1,
    Ne = 9,
    Lt = 2,
    Ge = 0xa,
    Ltu = 3,
    Geu = 0xb,
}

impl Cond {
    /// The branch that is taken exactly when `self` is not.
    ///
    /// Forward conditional branches are emitted as
    /// `b<inverted> +6; j target` (§3.7), which is the only thing this is
    /// for — and the reason the set above is closed under inversion. Note
    /// there is no `ble`/`bgt`: the emitter swaps the operands instead.
    #[inline]
    pub fn invert(self) -> Cond {
        match self {
            Cond::Eq => Cond::Ne,
            Cond::Ne => Cond::Eq,
            Cond::Lt => Cond::Ge,
            Cond::Ge => Cond::Lt,
            Cond::Ltu => Cond::Geu,
            Cond::Geu => Cond::Ltu,
        }
    }
}

/// `beqz` / `bnez`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZCond {
    /// `beqz` — BRI12 with `m:n` = `0:1`.
    Eqz,
    /// `bnez` — BRI12 with `m:n` = `1:1`.
    Nez,
}

impl ZCond {
    #[inline]
    pub fn invert(self) -> ZCond {
        match self {
            ZCond::Eqz => ZCond::Nez,
            ZCond::Nez => ZCond::Eqz,
        }
    }
    #[inline]
    fn mn(self) -> u32 {
        match self {
            ZCond::Eqz => 0b0001,
            ZCond::Nez => 0b0101,
        }
    }
}

/// The 16 values `beqi`/`bnei` can compare against (the ISA's `B4CONST`
/// table); the index is the encoding's `r` field.
pub const B4CONST: [i32; 16] = [
    -1, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 16, 32, 64, 128, 256,
];

/// The `B4CONST` index for `v`, if it is one of the sixteen.
#[inline]
pub fn b4const(v: i32) -> Option<u8> {
    let mut i = 0;
    while i < B4CONST.len() {
        if B4CONST[i] == v {
            return Some(i as u8);
        }
        i += 1;
    }
    None
}

// ------------------------------------------------------------- the buffer

/// The code buffer. Bytes in, bytes out; every method appends, and
/// [`Asm::here`] is the byte offset the next instruction will start at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Asm {
    bytes: Vec<u8>,
}

impl Asm {
    pub fn new() -> Asm {
        Asm { bytes: Vec::new() }
    }

    /// Start with `n` bytes of zeroed literal pool already in place, so
    /// offsets are image offsets from the first instruction on.
    pub fn with_pool(n_bytes: usize) -> Asm {
        Asm {
            bytes: alloc::vec![0u8; n_bytes],
        }
    }

    /// Byte offset the next emitted instruction will start at.
    #[inline]
    pub fn here(&self) -> usize {
        self.bytes.len()
    }

    #[inline]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The buffer as little-endian words, zero-padded to a word boundary.
    pub fn into_words(mut self) -> Vec<u32> {
        while self.bytes.len() % 4 != 0 {
            self.bytes.push(0);
        }
        self.bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    /// Overwrite a word of the literal pool (or anywhere else) in place.
    pub fn put_word(&mut self, at: usize, w: u32) {
        debug_assert!(at % 4 == 0);
        self.bytes[at..at + 4].copy_from_slice(&w.to_le_bytes());
    }

    // ------------------------------------------------------ raw emitters

    /// One 24-bit instruction, little-endian.
    fn w24(&mut self, v: u32) {
        debug_assert!(v < 1 << 24);
        self.bytes.push(v as u8);
        self.bytes.push((v >> 8) as u8);
        self.bytes.push((v >> 16) as u8);
    }

    /// One 16-bit (density) instruction, little-endian.
    fn w16(&mut self, v: u32) {
        debug_assert!(v < 1 << 16);
        self.bytes.push(v as u8);
        self.bytes.push((v >> 8) as u8);
    }

    /// `RRR`: `op2 | op1 | r | s | t | op0`.
    fn rrr(&mut self, op2: u32, op1: u32, r: u32, s: u32, t: u32, op0: u32) {
        self.w24(op2 << 20 | op1 << 16 | r << 12 | s << 8 | t << 4 | op0);
    }

    /// `RRI8`: an 8-bit immediate in bits 23..16 over `r | s | t | op0`.
    fn rri8(&mut self, imm8: u32, r: u32, s: u32, t: u32, op0: u32) {
        self.w24((imm8 & 0xff) << 16 | r << 12 | s << 8 | t << 4 | op0);
    }

    /// Pad to a 4-byte boundary with `nop.n` (2 B) and, for an odd gap, a
    /// 3-byte `nop`. `l32r` literals, `callx8` targets and `entry` all want
    /// 4-aligned addresses.
    /// A one-byte gap cannot be filled — the shortest instruction is two
    /// bytes — so that case pads with `nop.n` first and lands on a
    /// three-byte gap, five bytes in total.
    pub fn align4(&mut self) {
        loop {
            match (4 - self.here() % 4) % 4 {
                0 => return,
                3 => self.nop(),
                _ => self.nop_n(),
            }
        }
    }

    // ------------------------------------------------------------- window

    /// `entry as, frame` — open a window and reserve `frame` bytes.
    ///
    /// `frame` must be a non-negative multiple of 8 and at most 32 760
    /// (the 12-bit field is scaled by 8); the emitter rounds frames up to
    /// 16 anyway (§3.3). Returns `false` when the size does not fit, which
    /// the caller turns into [`crate::Refusal::FrameTooLarge`].
    #[must_use]
    pub fn entry(&mut self, s: Reg, frame: u32) -> bool {
        if frame % 8 != 0 || frame > 32_760 {
            return false;
        }
        let imm12 = frame / 8;
        // BRI12 field layout with m:n = 0b0011 selecting ENTRY.
        self.w24(imm12 << 12 | (s as u32) << 8 | 0b0011 << 4 | 0x6);
        true
    }

    /// `retw.n`.
    pub fn retw_n(&mut self) {
        self.w16(0xf << 12 | 1 << 4 | 0xd);
    }

    /// `callx8 as` — call through a register, rotating the window by 8.
    pub fn callx8(&mut self, s: Reg) {
        self.rrr(0, 0, 0, s as u32, 0b1110, 0);
    }

    /// `nop.n`.
    pub fn nop_n(&mut self) {
        self.w16(0xf << 12 | 3 << 4 | 0xd);
    }

    /// `nop` — the 24-bit form, for an odd alignment gap.
    pub fn nop(&mut self) {
        // RRR op2 = 0, op1 = 0, r = 2, s = 0, t = 0xf. objdump-pinned: with
        // r = 0 the same word is `callx12 a0`, which would have been a wild
        // jump instead of a pad.
        self.rrr(0, 0, 2, 0, 0xf, 0);
    }

    // --------------------------------------------------------- moves

    /// `mov.n ar, as`.
    pub fn mov_n(&mut self, r: Reg, s: Reg) {
        self.w16((s as u32) << 8 | (r as u32) << 4 | 0xd);
    }

    /// `movi at, imm` — 12-bit signed immediate, −2048..=2047.
    #[must_use]
    pub fn movi(&mut self, t: Reg, imm: i32) -> bool {
        if !(-2048..=2047).contains(&imm) {
            return false;
        }
        let u = (imm as u32) & 0xfff;
        // `s` carries the immediate's HIGH nibble and the RRI8 byte its low
        // eight bits (objdump-pinned; the other way round silently encodes
        // a different number).
        self.rri8(u & 0xff, 0xa, u >> 8, t as u32, 0x2);
        true
    }

    /// `movi.n as, imm` — 7-bit immediate, −32..=95.
    #[must_use]
    pub fn movi_n(&mut self, s: Reg, imm: i32) -> bool {
        if !(-32..=95).contains(&imm) {
            return false;
        }
        let f = (imm as u32) & 0x7f;
        self.w16((f & 0xf) << 12 | (s as u32) << 8 | (f >> 4) << 4 | 0xc);
        true
    }

    /// `movi` or `movi.n`, whichever fits — `movi.n` first because it is
    /// two bytes. Returns `false` when neither reaches (use `l32r`).
    #[must_use]
    pub fn movi_best(&mut self, r: Reg, imm: i32) -> bool {
        if (-32..=95).contains(&imm) {
            self.movi_n(r, imm)
        } else {
            self.movi(r, imm)
        }
    }

    /// `l32r at, lit` — load a 32-bit literal. `target` is the 4-aligned
    /// byte offset of the literal, which must be BEHIND this instruction
    /// (the field is a negative word offset; there is no LITBASE on these
    /// cores, §3.7).
    pub fn l32r(&mut self, t: Reg, target: usize) -> Result<(), OutOfReach> {
        let at = self.here();
        let base = (at + 3) & !3;
        let dist = target as isize - base as isize;
        if target % 4 != 0 || dist >= 0 || dist < -(1 << 18) || dist % 4 != 0 {
            return Err(OutOfReach {
                at,
                target,
                distance: dist,
                form: Form::L32r,
            });
        }
        let imm16 = ((dist / 4) as u32) & 0xffff;
        self.w24(imm16 << 8 | (t as u32) << 4 | 0x1);
        Ok(())
    }

    // ------------------------------------------------------- arithmetic

    /// `add ar, as, at`.
    pub fn add(&mut self, r: Reg, s: Reg, t: Reg) {
        self.rrr(0x8, 0, r as u32, s as u32, t as u32, 0);
    }
    /// `add.n ar, as, at`.
    pub fn add_n(&mut self, r: Reg, s: Reg, t: Reg) {
        self.w16((r as u32) << 12 | (s as u32) << 8 | (t as u32) << 4 | 0xa);
    }
    /// `sub ar, as, at`.
    pub fn sub(&mut self, r: Reg, s: Reg, t: Reg) {
        self.rrr(0xc, 0, r as u32, s as u32, t as u32, 0);
    }
    /// `neg ar, at`.
    pub fn neg(&mut self, r: Reg, t: Reg) {
        self.rrr(0x6, 0, r as u32, 0, t as u32, 0);
    }
    /// `abs ar, at`.
    pub fn abs(&mut self, r: Reg, t: Reg) {
        self.rrr(0x6, 0, r as u32, 1, t as u32, 0);
    }
    /// `and ar, as, at`.
    pub fn and(&mut self, r: Reg, s: Reg, t: Reg) {
        self.rrr(0x1, 0, r as u32, s as u32, t as u32, 0);
    }
    /// `or ar, as, at`.
    pub fn or(&mut self, r: Reg, s: Reg, t: Reg) {
        self.rrr(0x2, 0, r as u32, s as u32, t as u32, 0);
    }
    /// `xor ar, as, at`.
    pub fn xor(&mut self, r: Reg, s: Reg, t: Reg) {
        self.rrr(0x3, 0, r as u32, s as u32, t as u32, 0);
    }
    /// `mull ar, as, at` — low 32 bits of the 32×32 product (`MUL32`).
    pub fn mull(&mut self, r: Reg, s: Reg, t: Reg) {
        self.rrr(0x8, 0x2, r as u32, s as u32, t as u32, 0);
    }
    /// `mulsh ar, as, at` — high 32 bits, signed (`MUL32_HIGH`).
    pub fn mulsh(&mut self, r: Reg, s: Reg, t: Reg) {
        self.rrr(0xb, 0x2, r as u32, s as u32, t as u32, 0);
    }
    /// `quos ar, as, at` — signed 32-bit quotient (`DIV32`). Traps on a
    /// zero divisor, so the emitter always guards it.
    pub fn quos(&mut self, r: Reg, s: Reg, t: Reg) {
        self.rrr(0xd, 0x2, r as u32, s as u32, t as u32, 0);
    }
    /// `rems ar, as, at` — signed 32-bit remainder. Same zero-divisor trap.
    pub fn rems(&mut self, r: Reg, s: Reg, t: Reg) {
        self.rrr(0xf, 0x2, r as u32, s as u32, t as u32, 0);
    }

    /// `addi at, as, imm` — 8-bit signed.
    #[must_use]
    pub fn addi(&mut self, t: Reg, s: Reg, imm: i32) -> bool {
        if !(-128..=127).contains(&imm) {
            return false;
        }
        self.rri8((imm as u32) & 0xff, 0xc, s as u32, t as u32, 0x2);
        true
    }
    /// `addi.n ar, as, imm` — `imm` in −1..=15, excluding 0.
    #[must_use]
    pub fn addi_n(&mut self, r: Reg, s: Reg, imm: i32) -> bool {
        if imm == 0 || imm < -1 || imm > 15 {
            return false;
        }
        let f = if imm == -1 { 0u32 } else { imm as u32 };
        self.w16((r as u32) << 12 | (s as u32) << 8 | f << 4 | 0xb);
        true
    }
    /// `addmi at, as, imm` — 8-bit signed immediate scaled by 256.
    #[must_use]
    pub fn addmi(&mut self, t: Reg, s: Reg, imm: i32) -> bool {
        if imm % 256 != 0 || !(-32768..=32512).contains(&imm) {
            return false;
        }
        self.rri8(((imm / 256) as u32) & 0xff, 0xd, s as u32, t as u32, 0x2);
        true
    }

    // ---------------------------------------------------------- shifts

    /// `ssai sa` — set SAR to a constant 0..=31.
    pub fn ssai(&mut self, sa: u32) {
        debug_assert!(sa < 32);
        self.rrr(0x4, 0, 0x4, sa & 0xf, sa >> 4, 0);
    }
    /// `ssl as` — SAR := 32 − (as & 31), the setup for `sll`.
    pub fn ssl(&mut self, s: Reg) {
        self.rrr(0x4, 0, 0x1, s as u32, 0, 0);
    }
    /// `ssr as` — SAR := as & 31, the setup for `sra`/`srl`.
    pub fn ssr(&mut self, s: Reg) {
        self.rrr(0x4, 0, 0x0, s as u32, 0, 0);
    }
    /// `src ar, as, at` — funnel shift right of `as:at` by SAR.
    pub fn src(&mut self, r: Reg, s: Reg, t: Reg) {
        self.rrr(0x8, 0x1, r as u32, s as u32, t as u32, 0);
    }
    /// `sll ar, as` — `as << (32 − SAR)`; pair with [`Asm::ssl`].
    pub fn sll(&mut self, r: Reg, s: Reg) {
        self.rrr(0xa, 0x1, r as u32, s as u32, 0, 0);
    }
    /// `sra ar, at` — arithmetic `at >> SAR`; pair with [`Asm::ssr`].
    pub fn sra(&mut self, r: Reg, t: Reg) {
        self.rrr(0xb, 0x1, r as u32, 0, t as u32, 0);
    }
    /// `srai ar, at, sa` — arithmetic shift right by a constant 0..=31.
    pub fn srai(&mut self, r: Reg, t: Reg, sa: u32) {
        debug_assert!(sa < 32);
        self.rrr(0x2 | (sa >> 4), 0x1, r as u32, sa & 0xf, t as u32, 0);
    }
    /// `srli ar, at, sa` — logical shift right by a constant 0..=15.
    pub fn srli(&mut self, r: Reg, t: Reg, sa: u32) {
        debug_assert!(sa < 16);
        self.rrr(0x4, 0x1, r as u32, sa, t as u32, 0);
    }
    /// `slli ar, as, sa` — shift left by a constant 1..=31.
    pub fn slli(&mut self, r: Reg, s: Reg, sa: u32) {
        debug_assert!(sa >= 1 && sa < 32);
        let f = 32 - sa;
        // op2 is bit 4 of `32 - sa`, NOT `1 | …`: sa 1..16 encodes op2 = 1
        // and sa 17..31 encodes op2 = 0. objdump-pinned.
        self.rrr(f >> 4, 0x1, r as u32, s as u32, f & 0xf, 0);
    }

    // ----------------------------------------------------------- memory

    /// `l32i at, as, off` — `off` a multiple of 4 in 0..=1020.
    #[must_use]
    pub fn l32i(&mut self, t: Reg, s: Reg, off: u32) -> bool {
        if off % 4 != 0 || off > 1020 {
            return false;
        }
        self.rri8(off / 4, 0x2, s as u32, t as u32, 0x2);
        true
    }
    /// `l32i.n at, as, off` — `off` a multiple of 4 in 0..=60.
    #[must_use]
    pub fn l32i_n(&mut self, t: Reg, s: Reg, off: u32) -> bool {
        if off % 4 != 0 || off > 60 {
            return false;
        }
        self.w16((off / 4) << 12 | (s as u32) << 8 | (t as u32) << 4 | 0x8);
        true
    }
    /// `s32i at, as, off` — `off` a multiple of 4 in 0..=1020.
    #[must_use]
    pub fn s32i(&mut self, t: Reg, s: Reg, off: u32) -> bool {
        if off % 4 != 0 || off > 1020 {
            return false;
        }
        self.rri8(off / 4, 0x6, s as u32, t as u32, 0x2);
        true
    }
    /// `s32i.n at, as, off` — `off` a multiple of 4 in 0..=60.
    #[must_use]
    pub fn s32i_n(&mut self, t: Reg, s: Reg, off: u32) -> bool {
        if off % 4 != 0 || off > 60 {
            return false;
        }
        self.w16((off / 4) << 12 | (s as u32) << 8 | (t as u32) << 4 | 0x9);
        true
    }

    /// The narrow load when it fits, the wide one otherwise.
    #[must_use]
    pub fn load(&mut self, t: Reg, s: Reg, off: u32) -> bool {
        if off <= 60 && off % 4 == 0 {
            self.l32i_n(t, s, off)
        } else {
            self.l32i(t, s, off)
        }
    }
    /// The narrow store when it fits, the wide one otherwise.
    #[must_use]
    pub fn store(&mut self, t: Reg, s: Reg, off: u32) -> bool {
        if off <= 60 && off % 4 == 0 {
            self.s32i_n(t, s, off)
        } else {
            self.s32i(t, s, off)
        }
    }

    // --------------------------------------------------------- branches

    /// `b<cond> as, at, target` — a two-register conditional branch,
    /// reach ±128 B.
    pub fn branch(
        &mut self,
        cond: Cond,
        s: Reg,
        t: Reg,
        target: usize,
    ) -> Result<(), OutOfReach> {
        let at = self.here();
        let d = target as isize - (at as isize + 4);
        if !(-128..=127).contains(&d) {
            return Err(OutOfReach {
                at,
                target,
                distance: d,
                form: Form::Bri8,
            });
        }
        self.rri8((d as u32) & 0xff, cond as u32, s as u32, t as u32, 0x7);
        Ok(())
    }

    /// `beqz`/`bnez as, target` — reach ±2 KB.
    pub fn branch_z(&mut self, cond: ZCond, s: Reg, target: usize) -> Result<(), OutOfReach> {
        let at = self.here();
        let d = target as isize - (at as isize + 4);
        if !(-2048..=2047).contains(&d) {
            return Err(OutOfReach {
                at,
                target,
                distance: d,
                form: Form::Bri12,
            });
        }
        self.w24(((d as u32) & 0xfff) << 12 | (s as u32) << 8 | cond.mn() << 4 | 0x6);
        Ok(())
    }

    /// `beqi`/`bnei as, imm, target` — `imm` must be in [`B4CONST`].
    pub fn branch_i(
        &mut self,
        eq: bool,
        s: Reg,
        imm: i32,
        target: usize,
    ) -> Result<(), OutOfReach> {
        let at = self.here();
        let d = target as isize - (at as isize + 4);
        let Some(k) = b4const(imm) else {
            // Not a reach problem, but the only error channel this form
            // has; the emitter never asks for a value outside B4CONST.
            return Err(OutOfReach {
                at,
                target,
                distance: d,
                form: Form::Bri8,
            });
        };
        if !(-128..=127).contains(&d) {
            return Err(OutOfReach {
                at,
                target,
                distance: d,
                form: Form::Bri8,
            });
        }
        // `beqi`/`bnei` are op0 = 6 (the BRI8 member of the B group), not
        // op0 = 7 like the two-register branches: the B4CONST index goes in
        // `r` and the `m:n` selector in `t`. objdump-pinned — the wrong op0
        // decodes as `bnone`/`bbsi`, which is how this was caught.
        let mn: u32 = if eq { 0b0010 } else { 0b0110 };
        self.rri8((d as u32) & 0xff, k as u32, s as u32, mn, 0x6);
        Ok(())
    }

    /// `j target` — reach ±128 KB.
    pub fn j(&mut self, target: usize) -> Result<(), OutOfReach> {
        let at = self.here();
        let d = target as isize - (at as isize + 4);
        if !(-(1 << 17)..=(1 << 17) - 1).contains(&d) {
            return Err(OutOfReach {
                at,
                target,
                distance: d,
                form: Form::Jump,
            });
        }
        self.w24(((d as u32) & 0x3ffff) << 6 | 0x6);
        Ok(())
    }

    /// A `j` whose target is not known yet: emits three bytes and returns
    /// the site for [`Asm::patch_j`].
    pub fn j_forward(&mut self) -> usize {
        let at = self.here();
        self.w24(0x6);
        at
    }

    /// Fill in a `j` emitted by [`Asm::j_forward`].
    pub fn patch_j(&mut self, at: usize, target: usize) -> Result<(), OutOfReach> {
        let d = target as isize - (at as isize + 4);
        if !(-(1 << 17)..=(1 << 17) - 1).contains(&d) {
            return Err(OutOfReach {
                at,
                target,
                distance: d,
                form: Form::Jump,
            });
        }
        let v = ((d as u32) & 0x3ffff) << 6 | 0x6;
        self.bytes[at] = v as u8;
        self.bytes[at + 1] = (v >> 8) as u8;
        self.bytes[at + 2] = (v >> 16) as u8;
        Ok(())
    }
}
