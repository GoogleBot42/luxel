//! LXBC — the serialized form of a compiled [`Program`].
//!
//! See docs/spec/bytecode.md for the wire format. Three properties matter
//! here:
//!
//! - The decoder fully validates untrusted bytes. The VM indexes functions,
//!   globals, locals, and builtins without bounds checks (it trusts
//!   `Program`), so everything the VM would trust is proven at decode time —
//!   a hostile or corrupt blob is a `BcError`, never a device panic.
//! - The blob is **execution-ready**: the code and constant pool are one
//!   4-byte-aligned region of little-endian `u32` words that the VM runs
//!   exactly as stored. A device that memory-maps its pattern store
//!   ([`deserialize_lean_static`]) executes straight from flash — no RAM
//!   copy of the code or the constants, ever. Nothing in the word region is
//!   rewritten at load (v4 patched builtin ids into its RAM copy; v5 emits
//!   the runtime ids and *checks* them instead).
//! - Builtins are referenced by their runtime id (`BUILTINS` is append-only)
//!   and every id a blob uses is also listed by NAME in its import table, so
//!   a blob that names a builtin this build lacks fails with the name, not a
//!   bare index.
//!
//! `serialize` → `deserialize` → `serialize` is byte-identical; the corpus
//! round-trip test relies on that to prove decode fidelity.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::fixed::Fx;
use crate::vm::{lookup_builtin, FnDef, GlobalDef, PoolEntry, Program, Words, BUILTINS};

pub const MAGIC: [u8; 4] = *b"LXBC";
/// v2: jump operands are function-relative BYTE offsets (v1 used
/// instruction indices) and debug positions are offset-keyed runs — the
/// encoding the VM executes in place.
/// v3: const-array data section (deduplicated all-numeric literals) +
/// the `ConstArr` opcode — a pattern's `.rodata`.
/// v4: assert-message table + the `Assert` opcode (`assert()` invariants
/// run inline in init; the message must survive to compiler-less devices).
/// v5: fixed-width **u32 word** instructions in one 4-aligned word region
/// shared with the constant pool; pcs, jump targets and debug offsets are
/// word indices; builtin operands are runtime ids (import table kept for
/// validation only). Executable in place from memory-mapped flash.
pub const FORMAT_VERSION: u16 = 5;

/// Fixed header size (bytes) before the variable-length tables.
const HEADER_LEN: usize = 30;

/// Decoder hard limits — bound allocations before trusting any count field.
const MAX_BLOB: usize = 256 * 1024;
const MAX_FNS: usize = 1024;
const MAX_EXPORTS: usize = 1024;
const MAX_IMPORTS: usize = 512;
const MAX_GLOBALS: usize = 256;
const MAX_LOCALS: usize = 255;
/// Per-function code limit in WORDS — the same 64 KiB byte budget v4 had.
const MAX_CODE: usize = 65_536 / 4;
const MAX_ARGC: u8 = 16;
const MAX_DATA_ARRAYS: usize = 4096;
const MAX_DATA_ELEMS: usize = 65_536;
const MAX_ASSERT_MSGS: usize = 4096;
/// Whole word region (code + pool); the blob cap already implies it.
const MAX_WORDS: usize = MAX_BLOB / 4;

const FLAG_DEBUG: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BcError {
    /// The blob was produced for a different format version. Hosts with a
    /// compiler react by recompiling from source; devices surface it.
    Version { found: u16 },
    Malformed(String),
}

impl core::fmt::Display for BcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BcError::Version { found } => write!(
                f,
                "bytecode format v{found} (this build reads v{FORMAT_VERSION}) — recompile the pattern"
            ),
            BcError::Malformed(m) => write!(f, "invalid bytecode: {m}"),
        }
    }
}

fn err<T>(m: &str) -> Result<T, BcError> {
    Err(BcError::Malformed(m.to_string()))
}

// ---- LXP1 envelope ----
//
// The HTTP framing that carries a pattern to a device: name (empty for
// ad-hoc code pushes), source text, and the LXBC blob. Also served back by
// GET /api/pattern.lxp so sync followers can adopt the running pattern
// without owning a compiler.

pub const ENVELOPE_MAGIC: [u8; 4] = *b"LXP1";

pub struct Envelope<'a> {
    pub name: &'a str,
    pub source: &'a str,
    pub bytecode: &'a [u8],
}

pub fn encode_envelope(name: &str, source: &str, bytecode: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 1 + name.len() + 8 + source.len() + bytecode.len());
    out.extend_from_slice(&ENVELOPE_MAGIC);
    out.push(name.len().min(255) as u8);
    out.extend_from_slice(&name.as_bytes()[..name.len().min(255)]);
    out.extend_from_slice(&(source.len() as u32).to_le_bytes());
    out.extend_from_slice(source.as_bytes());
    out.extend_from_slice(&(bytecode.len() as u32).to_le_bytes());
    out.extend_from_slice(bytecode);
    out
}

/// Zero-copy decode. Does NOT validate the bytecode blob — callers run
/// [`deserialize`] on `envelope.bytecode` for that.
pub fn decode_envelope(bytes: &[u8]) -> Result<Envelope<'_>, BcError> {
    let mut r = Reader { buf: bytes, at: 0 };
    if r.take(4)? != ENVELOPE_MAGIC {
        return err("bad envelope magic (expected LXP1 — old client?)");
    }
    let nlen = r.u8()? as usize;
    let name = match core::str::from_utf8(r.take(nlen)?) {
        Ok(s) => s,
        Err(_) => return err("envelope name is not UTF-8"),
    };
    let slen = r.u32()? as usize;
    let source = match core::str::from_utf8(r.take(slen)?) {
        Ok(s) => s,
        Err(_) => return err("envelope source is not UTF-8"),
    };
    let blen = r.u32()? as usize;
    let bytecode = r.take(blen)?;
    if r.at != bytes.len() {
        return err("trailing bytes after envelope");
    }
    Ok(Envelope {
        name,
        source,
        bytecode,
    })
}

// ---- opcodes ----

pub(crate) mod op {
    pub const CONST_NUM: u8 = 0x01;
    pub const CONST_FUN: u8 = 0x02;
    pub const CONST_BUILTIN: u8 = 0x03;
    pub const LOAD_G: u8 = 0x04;
    pub const STORE_G: u8 = 0x05;
    pub const LOAD_L: u8 = 0x06;
    pub const STORE_L: u8 = 0x07;
    pub const LOAD_IDX: u8 = 0x08;
    pub const STORE_IDX: u8 = 0x09;
    pub const ARR_LEN: u8 = 0x0A;
    pub const NEW_ARRAY: u8 = 0x0B;
    /// v3: allocate an arena array sharing a const-pool entry (u16 index
    /// into the pool table) — copy-on-write on first mutation.
    pub const CONST_ARR: u8 = 0x0F;
    pub const DUP: u8 = 0x0C;
    pub const DUP2: u8 = 0x0D;
    pub const POP: u8 = 0x0E;
    pub const ADD: u8 = 0x10;
    pub const SUB: u8 = 0x11;
    pub const MUL: u8 = 0x12;
    pub const DIV: u8 = 0x13;
    pub const REM: u8 = 0x14;
    pub const POW: u8 = 0x15;
    pub const NEG: u8 = 0x16;
    pub const NOT: u8 = 0x17;
    pub const BIT_NOT: u8 = 0x18;
    pub const BIT_AND: u8 = 0x19;
    pub const BIT_OR: u8 = 0x1A;
    pub const BIT_XOR: u8 = 0x1B;
    pub const SHL: u8 = 0x1C;
    pub const SHR: u8 = 0x1D;
    pub const LT: u8 = 0x20;
    pub const LE: u8 = 0x21;
    pub const GT: u8 = 0x22;
    pub const GE: u8 = 0x23;
    pub const EQ: u8 = 0x24;
    pub const NE: u8 = 0x25;
    pub const JMP: u8 = 0x30;
    pub const JMP_IF_FALSE: u8 = 0x31;
    pub const JMP_IF_TRUE_PEEK: u8 = 0x32;
    pub const JMP_IF_FALSE_PEEK: u8 = 0x33;
    pub const CALL_FN: u8 = 0x38;
    pub const CALL_BUILTIN: u8 = 0x39;
    pub const CALL_VALUE: u8 = 0x3A;
    pub const RET: u8 = 0x3E;
    pub const RET_NULL: u8 = 0x3F;
    /// v4: pop the condition; falsy aborts with message-table entry (u16).
    pub const ASSERT: u8 = 0x40;

    // ---- superinstructions (v5, Gitea #261) ----
    //
    // Each one is EXACTLY the sequence of base opcodes named in its
    // comment, fused by the compiler's peephole (compile::peephole) so the
    // interpreter dispatches once instead of two or three times. Chosen
    // from `luxel bench --profile` counts over all 299 library patterns.
    // Appending them does not change the format version: a v5 blob that
    // uses none of them decodes and runs exactly as before.

    /// `StoreL n; Pop` — a local assignment in statement context.
    pub const STORE_L_POP: u8 = 0x41;
    /// `StoreG n; Pop` — a global assignment in statement context.
    pub const STORE_G_POP: u8 = 0x42;
    /// `LoadL a; LoadL b` (u8 a, u8 b).
    pub const LOAD_LL: u8 = 0x43;
    /// `LoadL a; LoadG g` (u8 a, u16 g).
    pub const LOAD_LG: u8 = 0x44;
    /// `LoadG g; LoadL a` (u16 g, u8 a).
    pub const LOAD_GL: u8 = 0x45;
    /// `LoadL a; LoadIdx` — `arr[i]` with the index in a local.
    pub const LOAD_L_IDX: u8 = 0x46;
    /// `LoadG g; LoadL a; LoadIdx` — the global-array read idiom.
    pub const LOAD_G_L_IDX: u8 = 0x47;
    /// `Const c; <binop>` — u8 sub-opcode, immediate in the next word.
    pub const CONST_OP: u8 = 0x48;
    /// `LoadL a; Const c; <binop>` — u8 local, u8 sub-opcode, immediate next.
    pub const LOAD_L_CONST_OP: u8 = 0x49;
    /// `LoadG g; Const c; <binop>` — u16 global, u8 sub-opcode, immediate next.
    pub const LOAD_G_CONST_OP: u8 = 0x4A;
    /// `Const c; CallBuiltin b, argc` — u16 b, u8 argc, immediate next.
    pub const CALL_BUILTIN_C: u8 = 0x4B;
    /// `Const c1; Const c2; CallBuiltin b, argc` — u16 b, u8 argc, two
    /// immediates in the next two words (`hsv(h, 1, 1)`).
    pub const CALL_BUILTIN_CC: u8 = 0x4C;
    /// `<cmp>; JmpIfFalse t` — u8 sub-opcode, target word index in the
    /// next word (loop and `if` headers).
    pub const CMP_JF: u8 = 0x4D;
    /// `Pop; RetNull` — the tail of every void function body.
    pub const POP_RET_NULL: u8 = 0x4E;
    // 0x4F..=0xFF: free for further superinstructions.
}

/// Sub-opcodes accepted by [`op::CONST_OP`] / [`op::LOAD_L_CONST_OP`] /
/// [`op::LOAD_G_CONST_OP`]: the two-operand value ops, which are exactly
/// the base opcodes they stand for.
pub(crate) fn is_binop_sub(o: u8) -> bool {
    matches!(
        o,
        op::ADD
            | op::SUB
            | op::MUL
            | op::DIV
            | op::REM
            | op::POW
            | op::BIT_AND
            | op::BIT_OR
            | op::BIT_XOR
            | op::SHL
            | op::SHR
            | op::LT
            | op::LE
            | op::GT
            | op::GE
            | op::EQ
            | op::NE
    )
}

/// Sub-opcodes accepted by [`op::CMP_JF`].
pub(crate) fn is_cmp_sub(o: u8) -> bool {
    matches!(
        o,
        op::LT | op::LE | op::GT | op::GE | op::EQ | op::NE
    )
}

/// Instruction-word field layout (v5). One `u32` per instruction:
/// bits 0..8 opcode, bits 8..32 a 24-bit operand field, which is a u8 in
/// bits 8..16, a u16 in bits 8..24, a `u16 index + u8 argc` pair (bits
/// 8..24 / 24..32) or a 24-bit word index depending on the opcode.
/// `CONST_NUM` is the only two-word instruction: its immediate (raw
/// 16.16 `i32`) is the following word.
pub(crate) mod enc {
    #[inline(always)]
    pub const fn opcode(w: u32) -> u8 {
        w as u8
    }
    #[inline(always)]
    pub const fn imm8(w: u32) -> u8 {
        (w >> 8) as u8
    }
    #[inline(always)]
    pub const fn imm16(w: u32) -> u16 {
        (w >> 8) as u16
    }
    #[inline(always)]
    pub const fn imm24(w: u32) -> u32 {
        w >> 8
    }
    #[inline(always)]
    pub const fn argc(w: u32) -> u8 {
        (w >> 24) as u8
    }
    /// Opcode with no operand.
    pub const fn bare(op: u8) -> u32 {
        op as u32
    }
    pub const fn with_u8(op: u8, v: u8) -> u32 {
        op as u32 | (v as u32) << 8
    }
    pub const fn with_u16(op: u8, v: u16) -> u32 {
        op as u32 | (v as u32) << 8
    }
    pub const fn with_u24(op: u8, v: u32) -> u32 {
        op as u32 | (v & 0x00FF_FFFF) << 8
    }
    pub const fn call(op: u8, idx: u16, argc: u8) -> u32 {
        op as u32 | (idx as u32) << 8 | (argc as u32) << 24
    }

    // ---- superinstruction operand layouts (Gitea #261) ----
    // A second u8 in bits 16..24, or a u16 in bits 16..32 — the halves the
    // base encoding leaves unused. `with_u16_u8` is `call` under a name
    // that says what it holds.

    /// Second u8 operand (bits 16..24).
    #[inline(always)]
    pub const fn imm8b(w: u32) -> u8 {
        (w >> 16) as u8
    }
    /// u16 operand in the high half (bits 16..32).
    #[inline(always)]
    pub const fn imm16hi(w: u32) -> u16 {
        (w >> 16) as u16
    }
    pub const fn with_u8_u8(op: u8, a: u8, b: u8) -> u32 {
        op as u32 | (a as u32) << 8 | (b as u32) << 16
    }
    pub const fn with_u8_u16(op: u8, a: u8, b: u16) -> u32 {
        op as u32 | (a as u32) << 8 | (b as u32) << 16
    }
    pub const fn with_u16_u8(op: u8, a: u16, b: u8) -> u32 {
        op as u32 | (a as u32) << 8 | (b as u32) << 24
    }
}

// ---- serialize ----

struct Writer {
    out: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn str8(&mut self, s: &str) -> Result<(), BcError> {
        if s.len() > u8::MAX as usize {
            return Err(BcError::Malformed(format!("name too long: `{s}`")));
        }
        self.u8(s.len() as u8);
        self.out.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

/// True when any function of `prog` calls one of the given runtime builtin
/// ids. Walks the instruction stream the same way the serializer's import
/// collection does, so an immediate operand word can never be mistaken for
/// an opcode. Code that doesn't walk (which the decoder would already have
/// rejected) reads as "no".
///
/// Used by [`crate::engine`]'s default-map heuristic: a `renderFrame`
/// pattern that never asks for a coordinate is a strip pattern and must
/// not be handed a square grid it didn't ask for.
#[inline(never)]
pub fn calls_any_builtin(prog: &Program, ids: &[u16]) -> bool {
    for f in &prog.fns {
        let s = f.code_start as usize;
        let Some(e) = s.checked_add(f.code_len as usize) else {
            return false;
        };
        if e > prog.words.len() {
            return false;
        }
        let code = &prog.words[s..e];
        let mut at = 0;
        while at < code.len() {
            let Ok(k) = walk_word(code[at]) else {
                return false;
            };
            if k.builtin.is_some_and(|b| ids.contains(&b)) {
                return true;
            }
            at += k.len;
        }
    }
    false
}

/// One instruction, decoded from its opcode word: how many words it spans
/// and which operands need validation. Shared by the serializer (import
/// table collection) and the decoder (full validation).
struct Walk {
    /// Words this instruction occupies (1, or 2 for `CONST_NUM`).
    len: usize,
    /// Runtime builtin id operand (Const Builtin / CallBuiltin).
    builtin: Option<u16>,
    fn_ref: Option<u16>,
    global_ref: Option<u16>,
    local_ref: Option<u8>,
    /// Second local operand (LoadLL).
    local_ref2: Option<u8>,
    /// Const-pool index (ConstArr).
    data_ref: Option<u16>,
    /// Assert-message-table index (Assert).
    msg_ref: Option<u16>,
    /// Function-relative word index (Jmp*).
    jump: Option<u32>,
    /// CmpJf: the word FOLLOWING the opcode word is a jump target (the
    /// 24-bit field is spoken for by the sub-opcode).
    jump_next: bool,
    argc: Option<u8>,
}

/// Decode one opcode word. Errors on an unknown opcode or on operand bits
/// the opcode does not use being set (the encoding is canonical: a blob
/// re-encodes byte-identically, and the VM masks nothing it need not).
fn walk_word(w: u32) -> Result<Walk, BcError> {
    let mut k = Walk {
        len: 1,
        builtin: None,
        fn_ref: None,
        global_ref: None,
        local_ref: None,
        local_ref2: None,
        data_ref: None,
        msg_ref: None,
        jump: None,
        jump_next: false,
        argc: None,
    };
    // operand-width masks: the bits an opcode may carry
    const NONE: u32 = 0x0000_00FF;
    const U8: u32 = 0x0000_FFFF;
    const U16: u32 = 0x00FF_FFFF;
    const ALL: u32 = 0xFFFF_FFFF;
    let used = match enc::opcode(w) {
        op::CONST_NUM => {
            k.len = 2;
            NONE
        }
        op::CONST_FUN => {
            k.fn_ref = Some(enc::imm16(w));
            U16
        }
        op::CONST_BUILTIN => {
            k.builtin = Some(enc::imm16(w));
            U16
        }
        op::LOAD_G | op::STORE_G => {
            k.global_ref = Some(enc::imm16(w));
            U16
        }
        op::LOAD_L | op::STORE_L => {
            k.local_ref = Some(enc::imm8(w));
            U8
        }
        op::NEW_ARRAY => U16,
        op::CONST_ARR => {
            k.data_ref = Some(enc::imm16(w));
            U16
        }
        op::ASSERT => {
            k.msg_ref = Some(enc::imm16(w));
            U16
        }
        op::JMP | op::JMP_IF_FALSE | op::JMP_IF_TRUE_PEEK | op::JMP_IF_FALSE_PEEK => {
            k.jump = Some(enc::imm24(w));
            ALL
        }
        op::CALL_FN => {
            k.fn_ref = Some(enc::imm16(w));
            k.argc = Some(enc::argc(w));
            ALL
        }
        op::CALL_BUILTIN => {
            k.builtin = Some(enc::imm16(w));
            k.argc = Some(enc::argc(w));
            ALL
        }
        op::CALL_VALUE => {
            k.argc = Some(enc::imm8(w));
            U8
        }
        // ---- superinstructions (Gitea #261) ----
        op::STORE_L_POP => {
            k.local_ref = Some(enc::imm8(w));
            U8
        }
        op::STORE_G_POP => {
            k.global_ref = Some(enc::imm16(w));
            U16
        }
        op::LOAD_LL => {
            k.local_ref = Some(enc::imm8(w));
            k.local_ref2 = Some(enc::imm8b(w));
            U16
        }
        op::LOAD_LG => {
            k.local_ref = Some(enc::imm8(w));
            k.global_ref = Some(enc::imm16hi(w));
            ALL
        }
        op::LOAD_GL | op::LOAD_G_L_IDX => {
            k.global_ref = Some(enc::imm16(w));
            k.local_ref = Some(enc::argc(w));
            ALL
        }
        op::LOAD_L_IDX => {
            k.local_ref = Some(enc::imm8(w));
            U8
        }
        op::CONST_OP => {
            if !is_binop_sub(enc::imm8(w)) {
                return err("bad sub-opcode");
            }
            k.len = 2;
            U8
        }
        op::LOAD_L_CONST_OP => {
            k.local_ref = Some(enc::imm8(w));
            if !is_binop_sub(enc::imm8b(w)) {
                return err("bad sub-opcode");
            }
            k.len = 2;
            U16
        }
        op::LOAD_G_CONST_OP => {
            k.global_ref = Some(enc::imm16(w));
            if !is_binop_sub(enc::argc(w)) {
                return err("bad sub-opcode");
            }
            k.len = 2;
            ALL
        }
        op::CALL_BUILTIN_C | op::CALL_BUILTIN_CC => {
            k.builtin = Some(enc::imm16(w));
            k.argc = Some(enc::argc(w));
            k.len = if enc::opcode(w) == op::CALL_BUILTIN_C {
                2
            } else {
                3
            };
            ALL
        }
        op::CMP_JF => {
            if !is_cmp_sub(enc::imm8(w)) {
                return err("bad sub-opcode");
            }
            // the branch target is the FOLLOWING word, not an operand field
            k.len = 2;
            k.jump_next = true;
            U8
        }
        op::POP_RET_NULL => NONE,
        op::LOAD_IDX | op::STORE_IDX | op::ARR_LEN | op::DUP | op::DUP2 | op::POP | op::ADD
        | op::SUB | op::MUL | op::DIV | op::REM | op::POW | op::NEG | op::NOT | op::BIT_NOT
        | op::BIT_AND | op::BIT_OR | op::BIT_XOR | op::SHL | op::SHR | op::LT | op::LE
        | op::GT | op::GE | op::EQ | op::NE | op::RET | op::RET_NULL => NONE,
        _ => return err("unknown opcode"),
    };
    if w & !used != 0 {
        return err("reserved operand bits set");
    }
    Ok(k)
}

/// Serialize a compiled program (with debug info — positions + local names).
///
/// Fails only on a `Program` the compiler could not have produced (e.g. a
/// hand-built one referencing a builtin id past the table).
pub fn serialize(prog: &Program) -> Result<Vec<u8>, BcError> {
    let words: &[u32] = &prog.words;
    let fn_code = |f: &FnDef| -> Result<&[u32], BcError> {
        let s = f.code_start as usize;
        match s.checked_add(f.code_len as usize) {
            Some(e) if e <= words.len() => Ok(&words[s..e]),
            _ => err("function code range out of bounds"),
        }
    };

    // Builtin import table: unique RUNTIME ids in first-appearance order.
    let mut imports: Vec<u16> = Vec::new();
    for f in &prog.fns {
        let code = fn_code(f)?;
        let mut at = 0;
        while at < code.len() {
            let k = walk_word(code[at])?;
            if let Some(b) = k.builtin {
                if b as usize >= BUILTINS.len() {
                    return Err(BcError::Malformed(format!("builtin id {b} out of range")));
                }
                if !imports.contains(&b) {
                    imports.push(b);
                }
            }
            at += k.len;
        }
    }
    for p in &prog.pool {
        let s = p.start as usize;
        if s.checked_add(p.len as usize).is_none_or(|e| e > words.len()) {
            return err("const-pool range out of bounds");
        }
    }

    let mut w = Writer { out: Vec::new() };
    w.out.extend_from_slice(&MAGIC);
    w.u16(FORMAT_VERSION);
    w.u16(FLAG_DEBUG);
    w.u16(prog.pixel_count_g);
    w.u16(prog.globals.len() as u16);
    w.u16(prog.fns.len() as u16);
    w.u16(prog.exported_fns.len() as u16);
    w.u16(imports.len() as u16);
    w.u16(prog.pool.len() as u16);
    w.u16(prog.assert_msgs.len() as u16);
    let words_off_at = w.out.len();
    w.u32(0); // words_off, patched below
    w.u32(words.len() as u32);
    debug_assert_eq!(w.out.len(), HEADER_LEN);

    for &b in &imports {
        w.str8(BUILTINS[b as usize].name)?;
        w.u16(b);
    }

    for g in &prog.globals {
        w.str8(&g.name)?;
        w.u8((g.export as u8) | ((g.predefined as u8) << 1));
        w.i32(g.init.raw());
    }

    // const-pool table: (word offset, len) per array — the words themselves
    // live in the word region
    for p in &prog.pool {
        w.u32(p.start);
        w.u16(p.len as u16);
    }

    // assert-message table (user-facing invariant text, deduplicated)
    for m in &prog.assert_msgs {
        w.str8(m)?;
    }

    for f in &prog.fns {
        w.str8(&f.name)?;
        w.u8(f.params);
        w.u16(f.locals as u16);
        w.u32(f.code_start);
        w.u32(f.code_len);
        // debug: word-index-keyed source-position runs
        w.u32(f.pos.len() as u32);
        for &(off, line, col) in &f.pos {
            w.u32(off);
            w.u32(line);
            w.u32(col);
        }
        for name in &f.local_names {
            w.str8(name)?;
        }
    }

    for (name, idx) in &prog.exported_fns {
        w.str8(name)?;
        w.u16(*idx);
    }

    // pad to a word boundary, then the word region verbatim
    while !w.out.len().is_multiple_of(4) {
        w.u8(0);
    }
    let words_off = w.out.len() as u32;
    w.out[words_off_at..words_off_at + 4].copy_from_slice(&words_off.to_le_bytes());
    w.out.reserve(words.len() * 4);
    for &x in words {
        w.u32(x);
    }

    Ok(w.out)
}

// ---- deserialize ----

struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BcError> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.buf.len());
        match end {
            Some(e) => {
                let s = &self.buf[self.at..e];
                self.at = e;
                Ok(s)
            }
            None => err("truncated"),
        }
    }
    fn u8(&mut self) -> Result<u8, BcError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, BcError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, BcError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, BcError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    /// Borrowed name — allocation (String::from) is the caller's choice, so
    /// the validate-only path stays allocation-free.
    fn str8(&mut self) -> Result<&'a str, BcError> {
        let n = self.u8()? as usize;
        let bytes = self.take(n)?;
        match core::str::from_utf8(bytes) {
            Ok(s) => Ok(s),
            Err(_) => err("name is not UTF-8"),
        }
    }
}

/// Decode and fully validate a blob. The returned `Program` upholds every
/// invariant the VM trusts (see module docs). Copies the word region.
pub fn deserialize(bytes: &[u8]) -> Result<Program, BcError> {
    Ok(decode(bytes, Mode::Full, None)?.expect("collecting mode returns a program"))
}

/// Like [`deserialize`] but skips debug info (per-instruction source
/// positions + local names). Small-heap devices run on this: runtime
/// errors keep the function name and pc but report line/col (0, 0);
/// by-name vars/controls/exports are unaffected (those names are not
/// debug info). Copies the word region — see [`deserialize_lean_static`]
/// for the zero-copy path.
pub fn deserialize_lean(bytes: &[u8]) -> Result<Program, BcError> {
    Ok(decode(bytes, Mode::Lean, None)?.expect("collecting mode returns a program"))
}

/// [`deserialize_lean`] over a blob that lives forever — a memory-mapped
/// flash slot, or a leaked Vec. Validates in place and, when the blob's
/// word region is 4-byte aligned in memory, BORROWS it: the `Program`'s
/// code and constant pool are then the caller's bytes and cost no RAM.
/// An unaligned input silently takes the copying path (never an error).
pub fn deserialize_lean_static(bytes: &'static [u8]) -> Result<Program, BcError> {
    Ok(decode(bytes, Mode::Lean, Some(bytes))?.expect("collecting mode returns a program"))
}

/// Validate a blob without materializing the `Program` — same checks as
/// [`deserialize`], near-zero allocation. This is what request handlers
/// (HTTP upload, MQTT activate, sync adopt) call on small-heap devices:
/// a full `Program` is only ever built once, by the render task.
pub fn validate(bytes: &[u8]) -> Result<(), BcError> {
    decode(bytes, Mode::Validate, None).map(|_| ())
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Full,
    Lean,
    Validate,
}

/// `try_reserve` wrapper: an oversized pattern on an exhausted heap must be
/// a clean decode error, never an allocation panic (= device reboot).
fn reserve<T>(v: &mut Vec<T>, n: usize) -> Result<(), BcError> {
    v.try_reserve_exact(n)
        .map_err(|_| BcError::Malformed("not enough memory for this pattern".to_string()))
}

/// The word region as read from the raw bytes — no alignment requirement,
/// no allocation; validation runs over this in every mode.
#[derive(Clone, Copy)]
struct WordBytes<'a>(&'a [u8]);

impl WordBytes<'_> {
    #[inline]
    fn get(&self, i: usize) -> u32 {
        u32::from_le_bytes(self.0[i * 4..i * 4 + 4].try_into().unwrap())
    }
}

/// `borrow`: the same bytes with a `'static` lifetime, when the caller can
/// promise them — the word region is then referenced in place if aligned.
fn decode(
    bytes: &[u8],
    mode: Mode,
    borrow: Option<&'static [u8]>,
) -> Result<Option<Program>, BcError> {
    let collect = mode != Mode::Validate;
    let keep_debug = mode == Mode::Full;
    if bytes.len() > MAX_BLOB {
        return err("blob too large");
    }
    let mut r = Reader { buf: bytes, at: 0 };
    if r.take(4)? != MAGIC {
        return err("bad magic (not LXBC)");
    }
    let version = r.u16()?;
    if version != FORMAT_VERSION {
        return Err(BcError::Version { found: version });
    }
    let flags = r.u16()?;
    let debug = flags & FLAG_DEBUG != 0;
    let pixel_count_g = r.u16()?;
    let n_globals = r.u16()? as usize;
    let n_fns = r.u16()? as usize;
    let n_exports = r.u16()? as usize;
    let n_imports = r.u16()? as usize;
    let n_data = r.u16()? as usize;
    let n_msgs = r.u16()? as usize;
    let words_off = r.u32()? as usize;
    let n_words = r.u32()? as usize;
    debug_assert_eq!(r.at, HEADER_LEN);

    if n_globals > MAX_GLOBALS
        || n_fns > MAX_FNS
        || n_exports > MAX_EXPORTS
        || n_imports > MAX_IMPORTS
        || n_data > MAX_DATA_ARRAYS
        || n_msgs > MAX_ASSERT_MSGS
        || n_words > MAX_WORDS
    {
        return err("section count over limit");
    }
    if n_fns == 0 {
        return err("no functions (fn 0 must be init)");
    }
    if pixel_count_g as usize >= n_globals {
        return err("pixel_count_g out of range");
    }
    // the word region is the tail of the blob, 4-aligned within it
    if !words_off.is_multiple_of(4) || words_off < HEADER_LEN {
        return err("word region misaligned");
    }
    if words_off.checked_add(n_words * 4) != Some(bytes.len()) {
        return err("word region does not end the blob");
    }
    let wb = WordBytes(&bytes[words_off..]);
    // the tables may not run into the word region
    r.buf = &bytes[..words_off];

    // builtin import table: (name, runtime id) — every id must be exactly
    // what this build's BUILTINS gives the name, so a blob from a newer
    // compiler fails by NAME here rather than as a bare index below
    let mut imports: Vec<u16> = Vec::new();
    reserve(&mut imports, n_imports)?;
    for _ in 0..n_imports {
        let name = r.str8()?;
        let id = r.u16()?;
        match lookup_builtin(name) {
            Some(b) if b == id => imports.push(id),
            Some(b) => {
                return Err(BcError::Malformed(format!(
                    "builtin `{name}` is id {b} on this firmware, the pattern expects {id} — recompile the pattern"
                )))
            }
            None => {
                return Err(BcError::Malformed(format!(
                    "builtin `{name}` is not available on this firmware — recompile the pattern"
                )))
            }
        }
    }

    let mut globals: Vec<GlobalDef> = Vec::new();
    if collect {
        reserve(&mut globals, n_globals)?;
    }
    for _ in 0..n_globals {
        let name = r.str8()?;
        let flags = r.u8()?;
        let init = Fx::from_raw(r.i32()?);
        if collect {
            globals.push(GlobalDef {
                name: String::from(name),
                export: flags & 1 != 0,
                predefined: flags & 2 != 0,
                init,
            });
        }
    }

    // const-pool table: (word offset, len) into the word region
    let mut pool: Vec<PoolEntry> = Vec::new();
    if collect {
        reserve(&mut pool, n_data)?;
    }
    let mut data_total = 0usize;
    for _ in 0..n_data {
        let start = r.u32()? as usize;
        let len = r.u16()? as usize;
        data_total += len;
        if data_total > MAX_DATA_ELEMS {
            return err("const-array data section over limit");
        }
        if start.checked_add(len).is_none_or(|e| e > n_words) {
            return err("const-array range out of bounds");
        }
        if collect {
            pool.push(PoolEntry {
                start: start as u32,
                len: len as u32,
            });
        }
    }

    // assert-message table: kept even by lean decodes — user-facing error
    // text, not debug info
    let mut assert_msgs: Vec<String> = Vec::new();
    if collect {
        reserve(&mut assert_msgs, n_msgs)?;
    }
    for _ in 0..n_msgs {
        let m = r.str8()?;
        if collect {
            assert_msgs.push(String::from(m));
        }
    }

    let mut fns: Vec<FnDef> = Vec::new();
    if collect {
        reserve(&mut fns, n_fns)?;
    }
    // instruction-boundary bitmap, reused across functions (transient)
    let mut bits: Vec<u64> = Vec::new();
    for _ in 0..n_fns {
        let name = r.str8()?;
        let params = r.u8()?;
        let locals = r.u16()? as usize;
        if locals > MAX_LOCALS {
            return err("too many locals");
        }
        if params as usize > locals {
            return err("params exceed locals");
        }
        let code_start = r.u32()? as usize;
        let code_len = r.u32()? as usize;
        if code_len > MAX_CODE {
            return err("function too long");
        }
        if code_start.checked_add(code_len).is_none_or(|e| e > n_words) {
            return err("function code range out of bounds");
        }
        let word = |i: usize| wb.get(code_start + i);

        // walk 1: instruction boundaries (also proves decodability)
        let nbits = code_len / 64 + 1;
        bits.clear();
        reserve(&mut bits, nbits)?;
        bits.resize(nbits, 0);
        let mut at = 0usize;
        while at < code_len {
            bits[at / 64] |= 1u64 << (at % 64);
            at += walk_word(word(at))?.len;
        }
        if at != code_len {
            return err("instruction overruns function end");
        }

        // walk 2: operand validation
        let mut at = 0usize;
        while at < code_len {
            let k = walk_word(word(at))?;
            if let Some(i) = k.fn_ref {
                if i as usize >= n_fns {
                    return err("function index out of range");
                }
            }
            if let Some(i) = k.global_ref {
                if i as usize >= n_globals {
                    return err("global index out of range");
                }
            }
            if let Some(i) = k.local_ref {
                if i as usize >= locals {
                    return err("local slot out of range");
                }
            }
            if let Some(i) = k.local_ref2 {
                if i as usize >= locals {
                    return err("local slot out of range");
                }
            }
            if let Some(d) = k.data_ref {
                if d as usize >= n_data {
                    return err("const-array index out of range");
                }
            }
            if let Some(m) = k.msg_ref {
                if m as usize >= n_msgs {
                    return err("assert message index out of range");
                }
            }
            if let Some(a) = k.argc {
                if a > MAX_ARGC {
                    return err("argc too large");
                }
            }
            // CmpJf carries its target in the following word.
            let jump = k.jump.or_else(|| k.jump_next.then(|| word(at + 1)));
            if let Some(t) = jump {
                let t = t as usize;
                // == code_len is a valid "fall off the end" target
                if t > code_len || (t < code_len && bits[t / 64] & (1u64 << (t % 64)) == 0) {
                    return err("jump target not on an instruction boundary");
                }
            }
            if let Some(b) = k.builtin {
                // proven by name above: the import table lists every id the
                // code may use, and each resolved to itself on this build
                if !imports.contains(&b) {
                    return err("builtin id not in the import table");
                }
            }
            at += k.len;
        }

        // debug info: word-index-keyed source-position runs + local names
        let mut pos: Vec<(u32, u32, u32)> = Vec::new();
        let mut local_names: Vec<String> = Vec::new();
        if debug {
            let n_runs = r.u32()? as usize;
            if n_runs > code_len + 1 {
                return err("bad debug runs");
            }
            if keep_debug {
                reserve(&mut pos, n_runs)?;
            }
            let mut prev: Option<u32> = None;
            for _ in 0..n_runs {
                let off = r.u32()?;
                let line = r.u32()?;
                let col = r.u32()?;
                if off as usize >= code_len.max(1) || prev.is_some_and(|p| off <= p) {
                    return err("debug runs not ascending");
                }
                prev = Some(off);
                if keep_debug {
                    pos.push((off, line, col));
                }
            }
            if keep_debug {
                reserve(&mut local_names, locals)?;
            }
            for _ in 0..locals {
                let n = r.str8()?;
                if keep_debug {
                    local_names.push(String::from(n));
                }
            }
        }
        if collect {
            fns.push(FnDef {
                name: String::from(name),
                params,
                locals: locals as u8,
                code_start: code_start as u32,
                code_len: code_len as u32,
                pos,
                local_names,
            });
        }
    }

    let mut exported_fns: Vec<(String, u16)> = Vec::new();
    if collect {
        reserve(&mut exported_fns, n_exports)?;
    }
    for _ in 0..n_exports {
        let name = r.str8()?;
        let idx = r.u16()?;
        if idx as usize >= n_fns {
            return err("export function index out of range");
        }
        if collect {
            exported_fns.push((String::from(name), idx));
        }
    }

    // only zero padding (< 4 bytes) may separate the tables from the words
    if words_off - r.at >= 4 || bytes[r.at..words_off].iter().any(|&b| b != 0) {
        return err("trailing bytes");
    }

    if !collect {
        return Ok(None);
    }

    // The word region: borrowed in place when the caller owns it forever
    // and it is 4-byte aligned in memory (a mapped flash slot is), else
    // copied. The in-place view assumes the host is little-endian, like
    // every target Luxel runs on; a big-endian host copies.
    let words = match borrow {
        Some(src)
            if cfg!(target_endian = "little")
                && (src.as_ptr() as usize + words_off).is_multiple_of(4) =>
        {
            let tail = &src[words_off..];
            debug_assert_eq!(tail.len(), n_words * 4);
            // SAFETY: `tail` is a `'static` byte slice of exactly
            // `n_words * 4` bytes whose start is 4-byte aligned (checked
            // above); `u32` has no invalid bit patterns; the slice is
            // shared-only, so no aliasing rule is violated.
            let s: &'static [u32] =
                unsafe { core::slice::from_raw_parts(tail.as_ptr() as *const u32, n_words) };
            Words::Static(s)
        }
        _ => {
            let mut v: Vec<u32> = Vec::new();
            reserve(&mut v, n_words)?;
            v.extend((0..n_words).map(|i| wb.get(i)));
            Words::Owned(v)
        }
    };

    Ok(Some(Program {
        words,
        pool,
        fns,
        globals,
        exported_fns,
        assert_msgs,
        pixel_count_g,
    }))
}

/// How many INSTRUCTIONS a function's code words hold — multi-word
/// instructions (`CONST_NUM`, the fused `*_CONST_OP`/`CALL_BUILTIN_C*`
/// forms, `CMP_JF`) count once, so this is the number of dispatches the VM
/// would make walking the range straight through, not `FnDef::code_len`.
///
/// `code` is `Program.words[code_start..code_start + code_len]`. Errors on
/// an undecodable word or an instruction that overruns the range — the same
/// checks [`validate`] makes, so any blob that validated counts cleanly.
///
/// Host tooling only (`luxel compile --stats`, `tools/oracle/opcount.mjs`);
/// the static count is what the Pixelblaze's own compiler output is compared
/// against (Gitea #312).
pub fn insn_count(code: &[u32]) -> Result<u32, BcError> {
    let mut at = 0usize;
    let mut n = 0u32;
    while at < code.len() {
        at += walk_word(code[at])?.len;
        n += 1;
    }
    if at != code.len() {
        return err("instruction overruns function end");
    }
    Ok(n)
}

/// Mnemonic for an opcode byte — host tooling only (`luxel bench
/// --profile`; Gitea #261). Gated so no device image carries the strings.
#[cfg(feature = "profile")]
pub fn op_name(o: u8) -> &'static str {
    match o {
        op::CONST_NUM => "CONST_NUM",
        op::CONST_FUN => "CONST_FUN",
        op::CONST_BUILTIN => "CONST_BUILTIN",
        op::LOAD_G => "LOAD_G",
        op::STORE_G => "STORE_G",
        op::LOAD_L => "LOAD_L",
        op::STORE_L => "STORE_L",
        op::LOAD_IDX => "LOAD_IDX",
        op::STORE_IDX => "STORE_IDX",
        op::ARR_LEN => "ARR_LEN",
        op::NEW_ARRAY => "NEW_ARRAY",
        op::CONST_ARR => "CONST_ARR",
        op::DUP => "DUP",
        op::DUP2 => "DUP2",
        op::POP => "POP",
        op::ADD => "ADD",
        op::SUB => "SUB",
        op::MUL => "MUL",
        op::DIV => "DIV",
        op::REM => "REM",
        op::POW => "POW",
        op::NEG => "NEG",
        op::NOT => "NOT",
        op::BIT_NOT => "BIT_NOT",
        op::BIT_AND => "BIT_AND",
        op::BIT_OR => "BIT_OR",
        op::BIT_XOR => "BIT_XOR",
        op::SHL => "SHL",
        op::SHR => "SHR",
        op::LT => "LT",
        op::LE => "LE",
        op::GT => "GT",
        op::GE => "GE",
        op::EQ => "EQ",
        op::NE => "NE",
        op::JMP => "JMP",
        op::JMP_IF_FALSE => "JMP_IF_FALSE",
        op::JMP_IF_TRUE_PEEK => "JMP_IF_TRUE_PEEK",
        op::JMP_IF_FALSE_PEEK => "JMP_IF_FALSE_PEEK",
        op::CALL_FN => "CALL_FN",
        op::CALL_BUILTIN => "CALL_BUILTIN",
        op::CALL_VALUE => "CALL_VALUE",
        op::RET => "RET",
        op::RET_NULL => "RET_NULL",
        op::ASSERT => "ASSERT",
        op::STORE_L_POP => "STORE_L_POP",
        op::STORE_G_POP => "STORE_G_POP",
        op::LOAD_LL => "LOAD_LL",
        op::LOAD_LG => "LOAD_LG",
        op::LOAD_GL => "LOAD_GL",
        op::LOAD_L_IDX => "LOAD_L_IDX",
        op::LOAD_G_L_IDX => "LOAD_G_L_IDX",
        op::CONST_OP => "CONST_OP",
        op::LOAD_L_CONST_OP => "LOAD_L_CONST_OP",
        op::LOAD_G_CONST_OP => "LOAD_G_CONST_OP",
        op::CALL_BUILTIN_C => "CALL_BUILTIN_C",
        op::CALL_BUILTIN_CC => "CALL_BUILTIN_CC",
        op::CMP_JF => "CMP_JF",
        op::POP_RET_NULL => "POP_RET_NULL",
        _ => "?",
    }
}
