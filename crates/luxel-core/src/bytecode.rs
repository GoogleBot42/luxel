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
use crate::vm::{lookup_builtin, FnDef, GlobalDef, Insn, Program, Value, BUILTINS};

pub const MAGIC: [u8; 4] = *b"LXBC";
pub const FORMAT_VERSION: u16 = 1;

/// Decoder hard limits — bound allocations before trusting any count field.
const MAX_BLOB: usize = 256 * 1024;
const MAX_FNS: usize = 1024;
const MAX_EXPORTS: usize = 1024;
const MAX_IMPORTS: usize = 512;
const MAX_GLOBALS: usize = 256;
const MAX_LOCALS: usize = 255;
const MAX_CODE: usize = 65_536;
const MAX_ARGC: u8 = 16;

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

mod op {
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

/// Serialize a compiled program (with debug info — positions + local names).
///
/// Fails only on a `Program` the compiler could not have produced (e.g. a
/// hand-built one referencing a builtin id past the table).
pub fn serialize(prog: &Program) -> Result<Vec<u8>, BcError> {
    // Builtin import table: unique ids in first-appearance order.
    let mut imports: Vec<u16> = Vec::new();
    let slot_of = |imports: &mut Vec<u16>, b: u16| -> u16 {
        match imports.iter().position(|&x| x == b) {
            Some(i) => i as u16,
            None => {
                imports.push(b);
                (imports.len() - 1) as u16
            }
        }
    };
    for f in &prog.fns {
        for insn in &f.code {
            match insn {
                Insn::Const(Value::Builtin(b)) | Insn::CallBuiltin { b, .. } => {
                    slot_of(&mut imports, *b);
                }
                _ => {}
            }
        }
    }
    for &b in &imports {
        if b as usize >= BUILTINS.len() {
            return Err(BcError::Malformed(format!("builtin id {b} out of range")));
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
    w.u16(0); // reserved

    for &b in &imports {
        w.str8(BUILTINS[b as usize].name)?;
    }

    for g in &prog.globals {
        w.str8(&g.name)?;
        w.u8((g.export as u8) | ((g.predefined as u8) << 1));
        w.i32(g.init.raw());
    }

    let import_slot = |b: u16| imports.iter().position(|&x| x == b).unwrap() as u16;
    for f in &prog.fns {
        w.str8(&f.name)?;
        w.u8(f.params);
        w.u16(f.locals as u16);
        w.u32(f.code.len() as u32);
        for insn in &f.code {
            emit_insn(&mut w, insn, &import_slot);
        }
        // debug: RLE positions (statement granularity → long runs)
        let mut runs: Vec<(u16, u32, u32)> = Vec::new();
        for pc in 0..f.code.len() {
            let (line, col) = f.pos_at(pc as u32);
            match runs.last_mut() {
                Some((count, l, c)) if *l == line && *c == col && *count < u16::MAX => *count += 1,
                _ => runs.push((1, line, col)),
            }
        }
        w.u32(runs.len() as u32);
        for (count, line, col) in runs {
            w.u16(count);
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

fn emit_insn(w: &mut Writer, insn: &Insn, import_slot: &dyn Fn(u16) -> u16) {
    use Insn::*;
    match insn {
        Const(Value::Num(v)) => {
            w.u8(op::CONST_NUM);
            w.i32(v.raw());
        }
        Const(Value::Fun(i)) => {
            w.u8(op::CONST_FUN);
            w.u16(*i);
        }
        Const(Value::Builtin(b)) => {
            w.u8(op::CONST_BUILTIN);
            w.u16(import_slot(*b));
        }
        // Arr constants don't exist in compiler output (arrays are built at
        // runtime); encode the handle-less equivalent, zero.
        Const(Value::Arr(_)) => {
            w.u8(op::CONST_NUM);
            w.i32(0);
        }
        LoadG(i) => {
            w.u8(op::LOAD_G);
            w.u16(*i);
        }
        StoreG(i) => {
            w.u8(op::STORE_G);
            w.u16(*i);
        }
        LoadL(i) => {
            w.u8(op::LOAD_L);
            w.u8(*i);
        }
        StoreL(i) => {
            w.u8(op::STORE_L);
            w.u8(*i);
        }
        LoadIdx => w.u8(op::LOAD_IDX),
        StoreIdx => w.u8(op::STORE_IDX),
        ArrLen => w.u8(op::ARR_LEN),
        NewArray(n) => {
            w.u8(op::NEW_ARRAY);
            w.u16(*n);
        }
        Dup => w.u8(op::DUP),
        Dup2 => w.u8(op::DUP2),
        Pop => w.u8(op::POP),
        Add => w.u8(op::ADD),
        Sub => w.u8(op::SUB),
        Mul => w.u8(op::MUL),
        Div => w.u8(op::DIV),
        Rem => w.u8(op::REM),
        Pow => w.u8(op::POW),
        Neg => w.u8(op::NEG),
        Not => w.u8(op::NOT),
        BitNot => w.u8(op::BIT_NOT),
        BitAnd => w.u8(op::BIT_AND),
        BitOr => w.u8(op::BIT_OR),
        BitXor => w.u8(op::BIT_XOR),
        Shl => w.u8(op::SHL),
        Shr => w.u8(op::SHR),
        Lt => w.u8(op::LT),
        Le => w.u8(op::LE),
        Gt => w.u8(op::GT),
        Ge => w.u8(op::GE),
        Eq => w.u8(op::EQ),
        Ne => w.u8(op::NE),
        Jmp(t) => {
            w.u8(op::JMP);
            w.u32(*t);
        }
        JmpIfFalse(t) => {
            w.u8(op::JMP_IF_FALSE);
            w.u32(*t);
        }
        JmpIfTruePeek(t) => {
            w.u8(op::JMP_IF_TRUE_PEEK);
            w.u32(*t);
        }
        JmpIfFalsePeek(t) => {
            w.u8(op::JMP_IF_FALSE_PEEK);
            w.u32(*t);
        }
        CallFn { fn_idx, argc } => {
            w.u8(op::CALL_FN);
            w.u16(*fn_idx);
            w.u8(*argc);
        }
        CallBuiltin { b, argc } => {
            w.u8(op::CALL_BUILTIN);
            w.u16(import_slot(*b));
            w.u8(*argc);
        }
        CallValue { argc } => {
            w.u8(op::CALL_VALUE);
            w.u8(*argc);
        }
        Ret => w.u8(op::RET),
        RetNull => w.u8(op::RET_NULL),
    }
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
    let _reserved = r.u16()?;

    if n_globals > MAX_GLOBALS || n_fns > MAX_FNS || n_exports > MAX_EXPORTS || n_imports > MAX_IMPORTS {
        return err("section count over limit");
    }
    if n_fns == 0 {
        return err("no functions (fn 0 must be init)");
    }
    if pixel_count_g as usize >= n_globals {
        return err("pixel_count_g out of range");
    }

    // builtin imports, resolved by name to runtime ids
    let mut imports: Vec<u16> = Vec::with_capacity(n_imports);
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

    let mut fns: Vec<FnDef> = Vec::new();
    if collect {
        reserve(&mut fns, n_fns)?;
    }
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
        // cheapest possible insn is 1 byte — pre-check before allocating
        if code_len > bytes.len() - r.at {
            return err("truncated code");
        }
        let mut code: Vec<Insn> = Vec::new();
        if collect {
            reserve(&mut code, code_len)?;
        }
        for _ in 0..code_len {
            let insn = read_insn(&mut r, &imports, n_fns, n_globals, locals, code_len)?;
            if collect {
                code.push(insn);
            }
        }
        let mut pos: Vec<(u32, u32)> = Vec::new();
        let mut local_names: Vec<String> = Vec::new();
        if debug {
            let n_runs = r.u32()? as usize;
            if n_runs > code_len {
                return err("bad debug runs");
            }
            if keep_debug {
                reserve(&mut pos, code_len)?;
            }
            let mut covered = 0usize;
            for _ in 0..n_runs {
                let count = r.u16()? as usize;
                let line = r.u32()?;
                let col = r.u32()?;
                if covered + count > code_len {
                    return err("debug runs exceed code length");
                }
                covered += count;
                if keep_debug {
                    for _ in 0..count {
                        pos.push((line, col));
                    }
                }
            }
            if covered != code_len {
                return err("debug runs shorter than code");
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
                code,
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
        fns,
        globals,
        exported_fns,
        pixel_count_g,
    }))
}

fn read_insn(
    r: &mut Reader,
    imports: &[u16],
    n_fns: usize,
    n_globals: usize,
    locals: usize,
    code_len: usize,
) -> Result<Insn, BcError> {
    use Insn::*;
    let opcode = r.u8()?;
    let fn_idx = |i: u16| -> Result<u16, BcError> {
        if (i as usize) < n_fns {
            Ok(i)
        } else {
            err("function index out of range")
        }
    };
    let global = |i: u16| -> Result<u16, BcError> {
        if (i as usize) < n_globals {
            Ok(i)
        } else {
            err("global index out of range")
        }
    };
    let local = |i: u8| -> Result<u8, BcError> {
        if (i as usize) < locals {
            Ok(i)
        } else {
            err("local slot out of range")
        }
    };
    let builtin = |slot: u16| -> Result<u16, BcError> {
        imports
            .get(slot as usize)
            .copied()
            .ok_or_else(|| BcError::Malformed("builtin import slot out of range".to_string()))
    };
    // targets may equal code_len (fetch past the end returns like RetNull),
    // never exceed it
    let target = |t: u32| -> Result<u32, BcError> {
        if t as usize <= code_len {
            Ok(t)
        } else {
            err("jump target out of range")
        }
    };
    let argc = |a: u8| -> Result<u8, BcError> {
        if a <= MAX_ARGC {
            Ok(a)
        } else {
            err("argc too large")
        }
    };
    Ok(match opcode {
        op::CONST_NUM => Const(Value::Num(Fx::from_raw(r.i32()?))),
        op::CONST_FUN => Const(Value::Fun(fn_idx(r.u16()?)?)),
        op::CONST_BUILTIN => Const(Value::Builtin(builtin(r.u16()?)?)),
        op::LOAD_G => LoadG(global(r.u16()?)?),
        op::STORE_G => StoreG(global(r.u16()?)?),
        op::LOAD_L => LoadL(local(r.u8()?)?),
        op::STORE_L => StoreL(local(r.u8()?)?),
        op::LOAD_IDX => LoadIdx,
        op::STORE_IDX => StoreIdx,
        op::ARR_LEN => ArrLen,
        op::NEW_ARRAY => NewArray(r.u16()?),
        op::DUP => Dup,
        op::DUP2 => Dup2,
        op::POP => Pop,
        op::ADD => Add,
        op::SUB => Sub,
        op::MUL => Mul,
        op::DIV => Div,
        op::REM => Rem,
        op::POW => Pow,
        op::NEG => Neg,
        op::NOT => Not,
        op::BIT_NOT => BitNot,
        op::BIT_AND => BitAnd,
        op::BIT_OR => BitOr,
        op::BIT_XOR => BitXor,
        op::SHL => Shl,
        op::SHR => Shr,
        op::LT => Lt,
        op::LE => Le,
        op::GT => Gt,
        op::GE => Ge,
        op::EQ => Eq,
        op::NE => Ne,
        op::JMP => Jmp(target(r.u32()?)?),
        op::JMP_IF_FALSE => JmpIfFalse(target(r.u32()?)?),
        op::JMP_IF_TRUE_PEEK => JmpIfTruePeek(target(r.u32()?)?),
        op::JMP_IF_FALSE_PEEK => JmpIfFalsePeek(target(r.u32()?)?),
        op::CALL_FN => {
            let f = fn_idx(r.u16()?)?;
            CallFn {
                fn_idx: f,
                argc: argc(r.u8()?)?,
            }
        }
        op::CALL_BUILTIN => {
            let b = builtin(r.u16()?)?;
            CallBuiltin {
                b,
                argc: argc(r.u8()?)?,
            }
        }
        op::CALL_VALUE => CallValue {
            argc: argc(r.u8()?)?,
        },
        op::RET => Ret,
        op::RET_NULL => RetNull,
        _ => return err("unknown opcode"),
    })
}
