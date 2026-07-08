//! LXBC — the serialized form of a compiled [`Program`].
//!
//! See docs/spec/bytecode.md for the wire format. Two properties matter
//! here:
//!
//! - The decoder fully validates untrusted bytes. The VM indexes functions,
//!   globals, locals, and builtins without bounds checks (it trusts
//!   `Program`), so everything the VM would trust is proven at decode time —
//!   a hostile or corrupt blob is a `BcError`, never a device panic.
//! - Builtins are referenced by *name* through a per-blob import table and
//!   resolved to runtime ids at load, so growing the builtin table never
//!   invalidates existing blobs.
//!
//! `serialize` → `deserialize` → `serialize` is byte-identical; the corpus
//! round-trip test relies on that to prove decode fidelity.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::fixed::Fx;
use crate::vm::{lookup_builtin, FnDef, GlobalDef, Program, Value, BUILTINS};

pub const MAGIC: [u8; 4] = *b"LXBC";
/// v2: jump operands are function-relative BYTE offsets (v1 used
/// instruction indices) and debug positions are offset-keyed runs — the
/// encoding the VM executes in place.
/// v3: const-array data section (deduplicated all-numeric literals) +
/// the `ConstArr` opcode — a pattern's `.rodata`.
/// v4: assert-message table + the `Assert` opcode (`assert()` invariants
/// run inline in init; the message must survive to compiler-less devices).
pub const FORMAT_VERSION: u16 = 4;

/// Decoder hard limits — bound allocations before trusting any count field.
const MAX_BLOB: usize = 256 * 1024;
const MAX_FNS: usize = 1024;
const MAX_EXPORTS: usize = 1024;
const MAX_IMPORTS: usize = 512;
const MAX_GLOBALS: usize = 256;
const MAX_LOCALS: usize = 255;
const MAX_CODE: usize = 65_536;
const MAX_ARGC: u8 = 16;
const MAX_DATA_ARRAYS: usize = 4096;
const MAX_DATA_ELEMS: usize = 65_536;
const MAX_ASSERT_MSGS: usize = 4096;

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
    /// into the data section) — copy-on-write on first mutation.
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

/// One instruction, walked in its byte encoding: where it ends and which
/// operands need validation or import-slot translation. Shared by the
/// serializer (runtime builtin id → import slot) and the decoder (the
/// reverse, plus full validation).
struct Walk {
    /// Offset just past this instruction.
    next: usize,
    /// Offset of a u16 builtin operand (Const Builtin / CallBuiltin).
    builtin_at: Option<usize>,
    fn_ref: Option<u16>,
    global_ref: Option<u16>,
    local_ref: Option<u8>,
    /// Const-pool index (ConstArr).
    data_ref: Option<u16>,
    /// Assert-message-table index (Assert).
    msg_ref: Option<u16>,
    jump: Option<u32>,
    argc: Option<u8>,
}

/// Walk the instruction starting at `at`. Errors on an unknown opcode or an
/// instruction truncated by the end of `code`.
fn walk_insn(code: &[u8], at: usize) -> Result<Walk, BcError> {
    let mut w = Walk {
        next: at + 1,
        builtin_at: None,
        fn_ref: None,
        global_ref: None,
        local_ref: None,
        data_ref: None,
        msg_ref: None,
        jump: None,
        argc: None,
    };
    let need = |n: usize| -> Result<(), BcError> {
        if at + 1 + n <= code.len() {
            Ok(())
        } else {
            err("truncated instruction")
        }
    };
    let u16_at = |p: usize| u16::from_le_bytes([code[p], code[p + 1]]);
    let u32_at = |p: usize| {
        u32::from_le_bytes([code[p], code[p + 1], code[p + 2], code[p + 3]])
    };
    match *code.get(at).ok_or_else(|| BcError::Malformed("truncated instruction".to_string()))? {
        op::CONST_NUM => {
            need(4)?;
            w.next = at + 5;
        }
        op::CONST_FUN => {
            need(2)?;
            w.fn_ref = Some(u16_at(at + 1));
            w.next = at + 3;
        }
        op::CONST_BUILTIN => {
            need(2)?;
            w.builtin_at = Some(at + 1);
            w.next = at + 3;
        }
        op::LOAD_G | op::STORE_G => {
            need(2)?;
            w.global_ref = Some(u16_at(at + 1));
            w.next = at + 3;
        }
        op::LOAD_L | op::STORE_L => {
            need(1)?;
            w.local_ref = Some(code[at + 1]);
            w.next = at + 2;
        }
        op::NEW_ARRAY => {
            need(2)?;
            w.next = at + 3;
        }
        op::CONST_ARR => {
            need(2)?;
            w.data_ref = Some(u16_at(at + 1));
            w.next = at + 3;
        }
        op::ASSERT => {
            need(2)?;
            w.msg_ref = Some(u16_at(at + 1));
            w.next = at + 3;
        }
        op::JMP | op::JMP_IF_FALSE | op::JMP_IF_TRUE_PEEK | op::JMP_IF_FALSE_PEEK => {
            need(4)?;
            w.jump = Some(u32_at(at + 1));
            w.next = at + 5;
        }
        op::CALL_FN => {
            need(3)?;
            w.fn_ref = Some(u16_at(at + 1));
            w.argc = Some(code[at + 3]);
            w.next = at + 4;
        }
        op::CALL_BUILTIN => {
            need(3)?;
            w.builtin_at = Some(at + 1);
            w.argc = Some(code[at + 3]);
            w.next = at + 4;
        }
        op::CALL_VALUE => {
            need(1)?;
            w.argc = Some(code[at + 1]);
            w.next = at + 2;
        }
        op::LOAD_IDX | op::STORE_IDX | op::ARR_LEN | op::DUP | op::DUP2 | op::POP | op::ADD
        | op::SUB | op::MUL | op::DIV | op::REM | op::POW | op::NEG | op::NOT | op::BIT_NOT
        | op::BIT_AND | op::BIT_OR | op::BIT_XOR | op::SHL | op::SHR | op::LT | op::LE
        | op::GT | op::GE | op::EQ | op::NE | op::RET | op::RET_NULL => {}
        _ => return err("unknown opcode"),
    }
    Ok(w)
}

/// Serialize a compiled program (with debug info — positions + local names).
///
/// Fails only on a `Program` the compiler could not have produced (e.g. a
/// hand-built one referencing a builtin id past the table).
pub fn serialize(prog: &Program) -> Result<Vec<u8>, BcError> {
    let fn_code = |f: &FnDef| -> Result<core::ops::Range<usize>, BcError> {
        let s = f.code_start as usize;
        let e = s + f.code_len as usize;
        if e <= prog.code.len() && s <= e {
            Ok(s..e)
        } else {
            err("function code range out of bounds")
        }
    };

    // Builtin import table: unique RUNTIME ids in first-appearance order.
    let mut imports: Vec<u16> = Vec::new();
    for f in &prog.fns {
        let code = &prog.code[fn_code(f)?];
        let mut at = 0;
        while at < code.len() {
            let w = walk_insn(code, at)?;
            if let Some(p) = w.builtin_at {
                let b = u16::from_le_bytes([code[p], code[p + 1]]);
                if b as usize >= BUILTINS.len() {
                    return Err(BcError::Malformed(format!("builtin id {b} out of range")));
                }
                if !imports.contains(&b) {
                    imports.push(b);
                }
            }
            at = w.next;
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
    w.u16(prog.data_arrays.len() as u16); // n_data (was reserved pre-v3)
    w.u16(prog.assert_msgs.len() as u16); // n_msgs (v4)

    for &b in &imports {
        w.str8(BUILTINS[b as usize].name)?;
    }

    for g in &prog.globals {
        w.str8(&g.name)?;
        w.u8((g.export as u8) | ((g.predefined as u8) << 1));
        w.i32(g.init.raw());
    }

    // const-array data section (the pattern's .rodata, deduplicated)
    for d in &prog.data_arrays {
        w.u16(d.len() as u16);
        for v in d.iter() {
            w.i32(v.num().raw());
        }
    }

    // assert-message table (user-facing invariant text, deduplicated)
    for m in &prog.assert_msgs {
        w.str8(m)?;
    }

    for f in &prog.fns {
        w.str8(&f.name)?;
        w.u8(f.params);
        w.u16(f.locals as u16);
        w.u32(f.code_len);
        // code bytes verbatim, then builtin operands rewritten to slots
        let out_base = w.out.len();
        let range = fn_code(f)?;
        w.out.extend_from_slice(&prog.code[range.clone()]);
        let code = &prog.code[range];
        let mut at = 0;
        while at < code.len() {
            let walk = walk_insn(code, at)?;
            if let Some(p) = walk.builtin_at {
                let b = u16::from_le_bytes([code[p], code[p + 1]]);
                let slot = imports.iter().position(|&x| x == b).unwrap() as u16;
                w.out[out_base + p..out_base + p + 2].copy_from_slice(&slot.to_le_bytes());
            }
            at = walk.next;
        }
        // debug: offset-keyed source-position runs
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
/// invariant the VM trusts (see module docs).
pub fn deserialize(bytes: &[u8]) -> Result<Program, BcError> {
    Ok(decode(bytes, Mode::Full)?.expect("collecting mode returns a program"))
}

/// Like [`deserialize`] but skips debug info (per-instruction source
/// positions + local names) — roughly HALF the decoded `Program`'s RAM.
/// Small-heap devices run on this: runtime errors keep the function name
/// and pc but report line/col (0, 0); by-name vars/controls/exports are
/// unaffected (those names are not debug info).
pub fn deserialize_lean(bytes: &[u8]) -> Result<Program, BcError> {
    Ok(decode(bytes, Mode::Lean)?.expect("collecting mode returns a program"))
}

/// Validate a blob without materializing the `Program` — same checks as
/// [`deserialize`], near-zero allocation. This is what request handlers
/// (HTTP upload, MQTT activate, sync adopt) call on small-heap devices:
/// a full `Program` is only ever built once, by the render task.
pub fn validate(bytes: &[u8]) -> Result<(), BcError> {
    decode(bytes, Mode::Validate).map(|_| ())
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

fn decode(bytes: &[u8], mode: Mode) -> Result<Option<Program>, BcError> {
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

    if n_globals > MAX_GLOBALS || n_fns > MAX_FNS || n_exports > MAX_EXPORTS || n_imports > MAX_IMPORTS || n_data > MAX_DATA_ARRAYS || n_msgs > MAX_ASSERT_MSGS {
        return err("section count over limit");
    }
    if n_fns == 0 {
        return err("no functions (fn 0 must be init)");
    }
    if pixel_count_g as usize >= n_globals {
        return err("pixel_count_g out of range");
    }

    // builtin imports, resolved by name to runtime ids
    let mut imports: Vec<u16> = Vec::new();
    reserve(&mut imports, n_imports)?;
    for _ in 0..n_imports {
        let name = r.str8()?;
        match lookup_builtin(name) {
            Some(b) => imports.push(b),
            None => {
                return Err(BcError::Malformed(format!(
                    "unknown builtin `{name}` (pattern compiled by a newer compiler?)"
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

    // const-array data section (deduped all-numeric literals)
    let mut data_arrays: Vec<alloc::boxed::Box<[Value]>> = Vec::new();
    if collect {
        reserve(&mut data_arrays, n_data)?;
    }
    let mut data_total = 0usize;
    for _ in 0..n_data {
        let len = r.u16()? as usize;
        data_total += len;
        if data_total > MAX_DATA_ELEMS {
            return err("const-array data section over limit");
        }
        if collect {
            let mut values: Vec<Value> = Vec::new();
            reserve(&mut values, len)?;
            for _ in 0..len {
                values.push(Value::Num(Fx::from_raw(r.i32()?)));
            }
            data_arrays.push(values.into());
        } else {
            r.take(len * 4)?;
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

    let mut prog_code: Vec<u8> = Vec::new();
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
        let code_len = r.u32()? as usize;
        if code_len > MAX_CODE {
            return err("function too long");
        }
        let sect = r.take(code_len)?;

        // walk 1: instruction boundaries (also proves decodability)
        let words = code_len / 64 + 1;
        bits.clear();
        reserve(&mut bits, words)?;
        bits.resize(words, 0);
        let mut at = 0usize;
        while at < code_len {
            bits[at / 64] |= 1u64 << (at % 64);
            at = walk_insn(sect, at)?.next;
        }
        if at != code_len {
            return err("instruction overruns function end");
        }

        // collect: append the section now; builtin operands are patched in
        // the copy during walk 2
        let code_start = prog_code.len();
        if collect {
            reserve(&mut prog_code, code_len)?;
            prog_code.extend_from_slice(sect);
        }

        // walk 2: operand validation (+ builtin slot → runtime id rewrite)
        let mut at = 0usize;
        while at < code_len {
            let w = walk_insn(sect, at)?;
            if let Some(i) = w.fn_ref {
                if i as usize >= n_fns {
                    return err("function index out of range");
                }
            }
            if let Some(i) = w.global_ref {
                if i as usize >= n_globals {
                    return err("global index out of range");
                }
            }
            if let Some(i) = w.local_ref {
                if i as usize >= locals {
                    return err("local slot out of range");
                }
            }
            if let Some(d) = w.data_ref {
                if d as usize >= n_data {
                    return err("const-array index out of range");
                }
            }
            if let Some(m) = w.msg_ref {
                if m as usize >= n_msgs {
                    return err("assert message index out of range");
                }
            }
            if let Some(a) = w.argc {
                if a > MAX_ARGC {
                    return err("argc too large");
                }
            }
            if let Some(t) = w.jump {
                let t = t as usize;
                // == code_len is a valid "fall off the end" target
                if t > code_len || (t < code_len && bits[t / 64] & (1u64 << (t % 64)) == 0) {
                    return err("jump target not on an instruction boundary");
                }
            }
            if let Some(p) = w.builtin_at {
                let slot = u16::from_le_bytes([sect[p], sect[p + 1]]) as usize;
                let Some(&b) = imports.get(slot) else {
                    return err("builtin import slot out of range");
                };
                if collect {
                    prog_code[code_start + p..code_start + p + 2]
                        .copy_from_slice(&b.to_le_bytes());
                }
            }
            at = w.next;
        }

        // debug info: offset-keyed source-position runs + local names
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

    if r.at != bytes.len() {
        return err("trailing bytes");
    }

    Ok(collect.then(|| Program {
        code: prog_code,
        data_arrays,
        fns,
        globals,
        exported_fns,
        assert_msgs,
        pixel_count_g,
    }))
}
