//! What the playground editor tells you about the JIT (Gitea #627,
//! docs/jit-design.md §4a).
//!
//! Two questions, one module, so the web UI, the CLI and (later) the
//! device-side report all read the same answers:
//!
//! - [`jit_eligibility`] — will this program be refused by the JIT for a
//!   reason the COMPILER can see? A refusal is whole-program: the pattern
//!   runs in the interpreter, at the interpreter's speed, with the same
//!   pixels. This is the single place later phases add their reasons
//!   (`TooLarge`, …); the device-only reasons (`psram`, `debug`) never
//!   appear here because no compiler can know them.
//! - [`dyn_lints`] — every `Dyn` (boxed) slot of
//!   [`crate::kinds::explain`], enriched with the VARIABLE NAME and a
//!   SOURCE POSITION so the editor can anchor a warning on the line that
//!   caused it, plus user-facing wording for the cause.
//!
//! Nothing here is on any hot path: it runs once per compile, in the
//! browser (`lx_kinds`) or in the CLI.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::bytecode::{enc, op};
use crate::kinds::{ilen, DynCause, DynSlot, Kind, Kinds};
use crate::vm::{builtin_sig, Program, SigRet, BUILTINS};

// ------------------------------------------------------------- refusals

/// A compile-time reason the JIT will refuse the whole program
/// (docs/jit-design.md §4a). One variant today; the list is expected to
/// grow, and the editor renders whatever it is handed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JitRefusal {
    /// The program reaches one of the callback-taking builtins
    /// (`arrayForEach`, `arrayMutate`, `arrayMapTo`, `arrayReduce`,
    /// `arraySortBy`, `mapPixels`) — `/api/status`'s `callbacks` reason.
    ///
    /// Detected from [`builtin_sig`]'s `callback` field, never from a
    /// hardcoded name list: Gitea #626 turns these into prelude functions
    /// written in the pattern language, and on the day it lands there is no
    /// `CallBuiltin` to find and this variant simply stops occurring.
    Callbacks {
        /// Builtin name as [`BUILTINS`] spells it.
        name: &'static str,
        /// Function holding the call site, and its fn-relative word index.
        fn_idx: u16,
        word: u32,
        line: u32,
        col: u32,
    },
}

impl JitRefusal {
    /// Stable machine id — the same spelling `/api/status`'s `jit.reason`
    /// uses, so one vocabulary covers compile-time and device-time refusals.
    pub fn id(&self) -> &'static str {
        match self {
            JitRefusal::Callbacks { .. } => "callbacks",
        }
    }

    /// The name the refusal is about (a builtin, later maybe a function).
    pub fn name(&self) -> &str {
        match self {
            JitRefusal::Callbacks { name, .. } => name,
        }
    }

    /// 1-based source position of the construct, `(0, 0)` when the program
    /// carries no debug info (a lean decode — never the browser).
    pub fn pos(&self) -> (u32, u32) {
        match self {
            JitRefusal::Callbacks { line, col, .. } => (*line, *col),
        }
    }

    /// Human wording, phrased for the editor's warning row. The UI prefixes
    /// it ("Runs in the interpreter on JIT boards: …").
    pub fn text(&self) -> String {
        match self {
            JitRefusal::Callbacks { name, .. } => {
                format!("`{name}` takes a callback, which the JIT does not compile yet")
            }
        }
    }
}

/// Can the JIT compile this program, as far as the COMPILER can tell?
///
/// `Ok(())` means nothing in the source forces interpreter mode — the
/// device may still refuse it for a reason only the device knows (image
/// too large for the code arena, no PSRAM, a debugger attached).
///
/// `kinds` is not read yet: a kind-verifier failure is a compiler bug and
/// is raised as a hard error long before this, and the size-based refusals
/// that will read it are a later phase. It is in the signature because
/// every future reason wants it and the callers should not have to change.
pub fn jit_eligibility(prog: &Program, kinds: &Kinds) -> Result<(), JitRefusal> {
    let _ = kinds;
    for (fi, f) in prog.fns.iter().enumerate() {
        let s = f.code_start as usize;
        let code = &prog.words[s..s + f.code_len as usize];
        let mut at = 0usize;
        while at < code.len() {
            let w = code[at];
            let o = enc::opcode(w);
            // A callback builtin reached either as a call or as a VALUE
            // (`arrayMutate` handed to something else) is equally fatal.
            let id = match o {
                op::CALL_BUILTIN | op::CALL_BUILTIN_C | op::CALL_BUILTIN_CC | op::CONST_BUILTIN => {
                    Some(enc::imm16(w))
                }
                _ => None,
            };
            if let Some(id) = id {
                if builtin_sig(id).callback.is_some() {
                    let name = BUILTINS.get(id as usize).map(|b| b.name).unwrap_or("?");
                    let (line, col) = f.pos_at(at as u32);
                    return Err(JitRefusal::Callbacks {
                        name,
                        fn_idx: fi as u16,
                        word: at as u32,
                        line,
                        col,
                    });
                }
            }
            at += ilen(o);
        }
    }
    Ok(())
}

// ----------------------------------------------------------- boxed slots

/// Which kind of slot a [`DynLint`] is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynScope {
    Global,
    Local,
    /// A function's RETURN value (not a variable; the editor words it as
    /// "what `foo()` returns").
    Ret,
}

impl DynScope {
    pub fn id(self) -> &'static str {
        match self {
            DynScope::Global => "global",
            DynScope::Local => "local",
            DynScope::Ret => "ret",
        }
    }
}

/// One boxed slot, ready to render: what it is called, where to point, and
/// why it is boxed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynLint {
    pub slot: DynSlot,
    pub cause: DynCause,
    pub scope: DynScope,
    /// Variable name (`heat`, `i`), or `local 3` when the blob carries no
    /// local names, or the function's name for a [`DynScope::Ret`].
    pub name: String,
    /// Function the slot lives in — empty for a global.
    pub fn_name: String,
    /// 1-based; `(0, 0)` when the program carries no debug info.
    pub line: u32,
    pub col: u32,
    /// User-facing wording ([`cause_message`]).
    pub message: String,
}

/// Machine id for a cause — kebab-case, stable, what the web tests assert
/// on. The human text lives in [`DynCause::text`] (the census wording) and
/// [`cause_message`] (the editor wording).
pub fn cause_id(c: DynCause) -> &'static str {
    match c {
        DynCause::CallbackParam => "callback-param",
        DynCause::ArrayElem => "array-elem",
        DynCause::CallValueResult => "call-value-result",
        DynCause::UnknownArray => "unknown-array",
        DynCause::Poison => "poison",
        DynCause::AssignMerge => "assign-merge",
        DynCause::JoinMerge => "join-merge",
        DynCause::BuiltinDyn => "builtin-dyn",
        DynCause::CallArgMerge => "call-arg-merge",
        DynCause::ElemMerge => "elem-merge",
        DynCause::HostWrite => "host-write",
    }
}

/// The census wording of [`DynCause::text`], rephrased around a name for a
/// person reading their own pattern. Every one ends in the consequence,
/// because that is the part that matters: the slot stays boxed.
pub fn cause_message(name: &str, c: DynCause) -> String {
    let why = match c {
        DynCause::CallbackParam => {
            "is a parameter of a function that is only ever called through a callback"
        }
        DynCause::ArrayElem => "is read out of an array that does not hold only numbers",
        DynCause::CallValueResult => "holds the result of calling a function value",
        DynCause::UnknownArray => "is read out of an array whose origin the compiler cannot follow",
        DynCause::Poison => "is an array written through a reference the compiler cannot follow",
        DynCause::AssignMerge => {
            "is assigned two different kinds of value (a number and an array or a function)"
        }
        DynCause::JoinMerge => {
            "takes its value from a conditional whose branches have different kinds"
        }
        DynCause::BuiltinDyn => {
            "holds the result of `arrayReduce`, whose kind the compiler cannot pin down"
        }
        DynCause::CallArgMerge => "is passed different kinds of value at different call sites",
        DynCause::ElemMerge => "holds both numbers and non-numbers",
        DynCause::HostWrite => {
            "is exported, so the UI can write a number into it while it also holds something else"
        }
    };
    format!("`{name}` {why}, so it runs boxed")
}

/// Typed-slot census of one program: how many storage slots the inference
/// proved a single representation for. Globals the engine predefines (`PI`,
/// `pixelCount`, the GPIO names) are excluded — they are not the pattern's
/// variables and they are always `Num`. Return kinds are not slots and are
/// not counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotStats {
    pub typed: usize,
    pub total: usize,
}

pub fn slot_stats(prog: &Program, kinds: &Kinds) -> SlotStats {
    let mut s = SlotStats::default();
    for (g, def) in prog.globals.iter().enumerate() {
        if def.predefined {
            continue;
        }
        s.total += 1;
        s.typed += usize::from(kinds.global(g) != Kind::Dyn);
    }
    for (f, fd) in prog.fns.iter().enumerate() {
        for i in 0..fd.locals as usize {
            s.total += 1;
            s.typed += usize::from(kinds.slot(f, i) != Kind::Dyn);
        }
    }
    s
}

/// Every `Dyn` slot, named and anchored — one lint per slot (the first
/// cause the inference recorded for it; a slot with two causes is still one
/// boxed slot and one warning).
pub fn dyn_lints(prog: &Program, kinds: &Kinds) -> Vec<DynLint> {
    let stores = StoreIndex::build(prog);
    let mut out: Vec<DynLint> = Vec::new();
    for r in crate::kinds::explain(prog, kinds) {
        if out.iter().any(|l| l.slot == r.slot) {
            continue;
        }
        let (scope, name, fn_name) = match r.slot {
            DynSlot::Global(g) => {
                let n = prog
                    .globals
                    .get(g as usize)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| format!("global {g}"));
                (DynScope::Global, n, String::new())
            }
            DynSlot::Local { fn_idx, slot } => {
                let f = prog.fns.get(fn_idx as usize);
                let n = f
                    .and_then(|f| f.local_names.get(slot as usize))
                    .cloned()
                    .unwrap_or_else(|| format!("local {slot}"));
                let owner = f.map(|f| fn_display(fn_idx, &f.name)).unwrap_or_default();
                (DynScope::Local, n, owner)
            }
            DynSlot::Ret(f) => {
                let n = prog
                    .fns
                    .get(f as usize)
                    .map(|fd| fn_display(f, &fd.name))
                    .unwrap_or_else(|| format!("fn {f}"));
                (DynScope::Ret, n.clone(), n)
            }
        };
        let (line, col) = stores.anchor(prog, kinds, r.slot);
        out.push(DynLint {
            slot: r.slot,
            cause: r.cause,
            scope,
            message: cause_message(&name, r.cause),
            name,
            fn_name,
            line,
            col,
        });
    }
    out.sort_by_key(|l| (l.line, l.col, l.name.clone()));
    out
}

fn fn_display(idx: u16, name: &str) -> String {
    if idx == 0 || name.is_empty() {
        "top level".to_string()
    } else {
        name.to_string()
    }
}

// ------------------------------------------------------------- anchoring

/// Every store instruction in the program, with the instruction that
/// precedes it — enough to pick the line a boxed slot should be reported
/// on without re-running the inference.
struct StoreIndex {
    /// (global, fn, word, previous word) in program order.
    globals: Vec<(u16, usize, u32, Option<u32>)>,
    /// (fn, slot, word, previous word) in program order.
    locals: Vec<(usize, u8, u32, Option<u32>)>,
    /// First `Ret` of each function, if any.
    rets: Vec<Option<u32>>,
}

impl StoreIndex {
    fn build(prog: &Program) -> StoreIndex {
        let mut idx = StoreIndex {
            globals: Vec::new(),
            locals: Vec::new(),
            rets: alloc::vec![None; prog.fns.len()],
        };
        for (fi, f) in prog.fns.iter().enumerate() {
            let s = f.code_start as usize;
            let code = &prog.words[s..s + f.code_len as usize];
            let mut at = 0usize;
            let mut prev: Option<u32> = None;
            while at < code.len() {
                let w = code[at];
                let o = enc::opcode(w);
                match o {
                    op::STORE_G | op::STORE_G_POP => {
                        idx.globals.push((enc::imm16(w), fi, at as u32, prev))
                    }
                    op::STORE_L | op::STORE_L_POP => {
                        idx.locals.push((fi, enc::imm8(w), at as u32, prev))
                    }
                    op::RET if idx.rets[fi].is_none() => idx.rets[fi] = Some(at as u32),
                    _ => {}
                }
                prev = Some(w);
                at += ilen(o);
            }
        }
        idx
    }

    /// The (line, col) to hang a lint on.
    ///
    /// Heuristic, and deliberately so — this anchors a warning, it does not
    /// decide semantics. In order of preference: the first store to the
    /// slot whose value is visibly not a number (that is the store that
    /// WIDENED it — `heat = array(8)` after `var heat = 0`); else the first
    /// store outside top-level init (the declared initializer is rarely the
    /// interesting half of a merge); else the first store anywhere; else the
    /// function's first word, which is where a callback parameter lives.
    fn anchor(&self, prog: &Program, kinds: &Kinds, slot: DynSlot) -> (u32, u32) {
        let pos = |fi: usize, word: u32| -> (u32, u32) {
            prog.fns.get(fi).map(|f| f.pos_at(word)).unwrap_or((0, 0))
        };
        match slot {
            DynSlot::Global(g) => {
                let mine = self.globals.iter().filter(|s| s.0 == g);
                let widen = mine
                    .clone()
                    .find(|(_, fi, _, prev)| prev.is_some_and(|p| pushes_non_num(kinds, *fi, p)));
                let outside_init = mine.clone().find(|(_, fi, _, _)| *fi != 0);
                match widen.or(outside_init).or_else(|| mine.clone().next()) {
                    Some(&(_, fi, at, _)) => pos(fi, at),
                    None => pos(0, 0),
                }
            }
            DynSlot::Local { fn_idx, slot } => {
                let fi = fn_idx as usize;
                let mine = self.locals.iter().filter(|s| s.0 == fi && s.1 == slot);
                let widen = mine
                    .clone()
                    .find(|(fi, _, _, prev)| prev.is_some_and(|p| pushes_non_num(kinds, *fi, p)));
                match widen.or_else(|| mine.clone().next()) {
                    Some(&(_, _, at, _)) => pos(fi, at),
                    None => pos(fi, 0),
                }
            }
            DynSlot::Ret(f) => {
                let fi = f as usize;
                pos(fi, self.rets.get(fi).copied().flatten().unwrap_or(0))
            }
        }
    }
}

/// Does this instruction visibly push something that is not a number? Used
/// only to pick which store widened a slot (see [`StoreIndex::anchor`]);
/// "no" is always a safe answer.
fn pushes_non_num(kinds: &Kinds, fi: usize, w: u32) -> bool {
    match enc::opcode(w) {
        op::NEW_ARRAY | op::CONST_ARR | op::CONST_FUN | op::CONST_BUILTIN | op::BOX => true,
        op::LOAD_IDX | op::LOAD_L_IDX | op::CALL_VALUE => true,
        op::LOAD_G_L_IDX => true,
        op::LOAD_G => kinds.global(enc::imm16(w) as usize) != Kind::Num,
        op::LOAD_L => kinds.slot(fi, enc::imm8(w) as usize) != Kind::Num,
        op::CALL_FN => kinds.ret(enc::imm16(w) as usize) != Kind::Num,
        op::CALL_BUILTIN | op::CALL_BUILTIN_C | op::CALL_BUILTIN_CC => {
            !matches!(builtin_sig(enc::imm16(w)).ret, SigRet::Num)
        }
        _ => false,
    }
}
