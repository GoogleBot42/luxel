//! AST → bytecode compiler.
//!
//! Scoping rules (matching the documented PB semantics):
//! - `var` is function-scoped and hoisted (JS `var` rules); `var` at top
//!   level declares a global.
//! - Assignment to an undeclared name creates a global — even from inside a
//!   function or lambda. A collection pre-pass finds every such name so
//!   forward references across functions resolve.
//! - No closures: a lambda sees only its own params/locals and globals.
//! - `pixelCount` and the math constants are predefined globals.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ast::*;
use crate::bytecode::op;
use crate::diag::{line_col, Diagnostic, Span};
use crate::fixed::Fx;
use crate::parse::parse_program;
use crate::vm::{lookup_builtin, lookup_method, FnDef, GlobalDef, PoolEntry, Program, Value, Words};

/// Compiler IR: one virtual instruction, jump targets as INSTRUCTION
/// INDICES. This never reaches the VM — [`assemble`] lowers it to the
/// fixed-width LXBC word encoding (word-index jumps) that `Program.words`
/// holds and the interpreter executes in place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Insn {
    Const(Value),
    LoadG(u16),
    StoreG(u16), // pops value, pushes it back (assignment is an expression)
    LoadL(u8),
    StoreL(u8), // ditto
    LoadIdx,    // [arr idx] → [elem]
    StoreIdx,   // [arr idx val] → [val]
    ArrLen,     // [arr] → [len]
    NewArray(u16),
    /// Allocate an arena array sharing const-pool entry N (all-numeric
    /// literal, deduplicated) — copy-on-write on first mutation.
    ConstArr(u16),
    /// Pop the condition; falsy aborts the run with message-pool entry N
    /// (`assert()` — top-level init only).
    Assert(u16),
    Dup,
    Dup2, // [a b] → [a b a b]
    Pop,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    Neg,
    Not,
    BitNot,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    Jmp(u32),
    JmpIfFalse(u32),     // pops
    JmpIfTruePeek(u32),  // ||: jump keeping the lhs value
    JmpIfFalsePeek(u32), // &&
    CallFn { fn_idx: u16, argc: u8 },
    CallBuiltin { b: u16, argc: u8 },
    CallValue { argc: u8 },
    Ret,
    RetNull,

    // ---- superinstructions (Gitea #261) ----
    //
    // Produced ONLY by [`peephole`], after the whole function is emitted.
    // Each is exactly the base sequence in its comment; nothing else in
    // the compiler creates or inspects them.
    StoreLPop(u8),         // StoreL n; Pop
    StoreGPop(u16),        // StoreG n; Pop
    LoadLL(u8, u8),        // LoadL a; LoadL b
    LoadLG(u8, u16),       // LoadL a; LoadG g
    LoadGL(u16, u8),       // LoadG g; LoadL a
    LoadLIdx(u8),          // LoadL a; LoadIdx
    LoadGLIdx(u16, u8),    // LoadG g; LoadL a; LoadIdx
    ConstOp(u8, Fx),       // Const c; <binop>
    LoadLConstOp(u8, u8, Fx),  // LoadL a; Const c; <binop>
    LoadGConstOp(u16, u8, Fx), // LoadG g; Const c; <binop>
    CallBuiltinC { b: u16, argc: u8, c: Fx }, // Const c; CallBuiltin
    CallBuiltinCC { b: u16, argc: u8, c1: Fx, c2: Fx }, // Const; Const; CallBuiltin
    CmpJf(u8, u32),        // <cmp>; JmpIfFalse t   (t = instruction index)
    PopRetNull,            // Pop; RetNull
}

const MAX_GLOBALS: usize = 256;
// 255, not 256: FnDef.locals is a u8 slot *count* (a 256th slot would wrap
// it to 0) and LoadL/StoreL operands are u8 slot indices.
const MAX_LOCALS: usize = 255;

/// Knobs on the lowering pass. The only one so far is a switch for the
/// superinstruction peephole (Gitea #261): every host wants it on, but
/// `luxel bench --no-fuse` and the equivalence tests need a way to get the
/// unfused stream out of the same binary, and a decoder bug in the fused
/// arms is then one flag away from being bisected.
#[derive(Clone, Copy, Debug)]
pub struct CompileOpts {
    pub superinstructions: bool,
    /// Compile-time evaluation of literal arithmetic and of reads of the
    /// predefined globals a pattern never writes (Gitea #312). Unlike the
    /// peephole this changes the *unfused* stream too, so it gets its own
    /// switch: `tests/constfold.rs` renders folded and unfolded and
    /// compares them, exactly as `tests/superinsns.rs` does for fusion.
    pub const_folding: bool,
}

impl Default for CompileOpts {
    fn default() -> Self {
        CompileOpts {
            superinstructions: true,
            const_folding: true,
        }
    }
}

pub fn compile(src: &str) -> Result<Program, Diagnostic> {
    compile_with(src, CompileOpts::default())
}

pub fn compile_with(src: &str, opts: CompileOpts) -> Result<Program, Diagnostic> {
    let ast = parse_program(src)?;
    let mut c = Compiler::new(src);
    c.collect(&ast)?;
    c.emit_program(&ast)?;
    Ok(assemble(
        c.fns,
        c.globals,
        c.exported_fns,
        c.data_arrays,
        c.assert_msgs,
        opts,
    ))
}

/// Const-pool limits (mirrored by the decoder).
const MAX_DATA_ARRAYS: usize = 4096;
const MAX_DATA_ELEMS: usize = 65_536;
const MAX_ASSERT_MSGS: usize = 4096;

/// Lower the compiler IR to the fixed-width word encoding the VM executes
/// in place: two passes per function — measure each instruction's word
/// index (`CONST_NUM` is two words, everything else one), then emit with
/// jump targets mapped from instruction indices to word indices. The
/// const-array pool follows the code in the same word region.
/// Per-instruction positions collapse to word-index-keyed runs.
fn assemble(
    fns: Vec<FnIr>,
    globals: Vec<GlobalDef>,
    exported_fns: Vec<(String, u16)>,
    data_arrays: Vec<Vec<i32>>,
    assert_msgs: Vec<String>,
    opts: CompileOpts,
) -> Program {
    let mut code: Vec<u32> = Vec::new();
    let mut defs: Vec<FnDef> = Vec::with_capacity(fns.len());
    // Which globals can never change over the whole run? A whole-program
    // question, so it is answered once, before any function is lowered
    // (Gitea #312).
    let frozen = frozen_globals(&fns, &globals);
    for mut f in fns {
        // pass 0a: constant folding — literal arithmetic the source wrote
        // out, and reads of the frozen globals wherever a constant fuses
        // better than the load does (Gitea #312).
        if opts.const_folding {
            let (fcode, fpos) =
                const_fold(core::mem::take(&mut f.code), core::mem::take(&mut f.pos), &frozen);
            f.code = fcode;
            f.pos = fpos;
        }
        // pass 0b: fuse the hot sequences into superinstructions
        if opts.superinstructions {
            let (fcode, fpos) =
                peephole(core::mem::take(&mut f.code), core::mem::take(&mut f.pos));
            f.code = fcode;
            f.pos = fpos;
        }
        // pass 1: word index of each instruction (+ end)
        let mut offsets: Vec<u32> = Vec::with_capacity(f.code.len() + 1);
        let mut at = 0u32;
        for insn in &f.code {
            offsets.push(at);
            at += insn_len(insn);
        }
        offsets.push(at);
        // pass 2: emit
        let code_start = code.len() as u32;
        for insn in &f.code {
            emit_insn(&mut code, insn, &offsets);
        }
        let code_len = code.len() as u32 - code_start;
        // positions → word-keyed runs (statement granularity ⇒ few runs)
        let mut pos: Vec<(u32, u32, u32)> = Vec::new();
        for (i, &(line, col)) in f.pos.iter().enumerate() {
            match pos.last() {
                Some(&(_, l, c)) if l == line && c == col => {}
                _ => pos.push((offsets[i], line, col)),
            }
        }
        defs.push(FnDef {
            name: f.name,
            params: f.params,
            locals: f.local_names.len() as u8,
            code_start,
            code_len,
            pos,
            local_names: f.local_names,
        });
    }
    // const pool: raw words appended after the code
    let mut pool: Vec<PoolEntry> = Vec::with_capacity(data_arrays.len());
    for d in &data_arrays {
        pool.push(PoolEntry {
            start: code.len() as u32,
            len: d.len() as u32,
        });
        code.extend(d.iter().map(|&r| r as u32));
    }
    Program {
        words: Words::Owned(code),
        pool,
        fns: defs,
        globals,
        exported_fns,
        assert_msgs,
        pixel_count_g: 0,
    }
}

/// Fuse the hot instruction sequences into superinstructions (Gitea #261).
///
/// A greedy leftmost-longest match over the templates below, run once per
/// function on the finished IR. The candidates and their order come from
/// `luxel bench --profile` counts over all 299 library patterns
/// (tools/profile-library.mjs) — the pairs and triples ranked by how often
/// they actually EXECUTE, not how often they appear.
///
/// Two rules make the fusion invisible to everything but the dispatch count:
///
/// * **Never fuse across a jump target.** An instruction some branch can
///   land on stays addressable, so every jump still targets an instruction
///   boundary and lands where it did.
/// * **Never fuse across a source position.** Positions are set per
///   statement, so a fused run always lies inside one statement and the
///   position runs `assemble` builds are unchanged — the debugger stops on
///   exactly the same source lines, and a runtime error inside a fused op
///   reports the same (line, col) as the base sequence would.
///
/// Semantics are otherwise identical by construction: each arm of the VM
/// does exactly what the sequence it replaces did, in the same order, with
/// the same error messages. The one deliberate difference is FUEL: a fused
/// instruction costs 1, not 2 or 3, so a pattern's execution-limit budget
/// stretches slightly further (`FUEL` is a runaway-loop guard, not a
/// semantic quantity).
/// `is_target[i]` ⇔ some jump in `code` lands on instruction `i`. Both
/// rewrite passes need it: an instruction a jump can land on may never be
/// folded away or fused into its predecessor, or control would arrive in
/// the middle of what used to be several instructions.
fn jump_targets(code: &[Insn]) -> Vec<bool> {
    use Insn::*;
    let n = code.len();
    let mut is_target = alloc::vec![false; n + 1];
    for insn in code {
        match insn {
            Jmp(t) | JmpIfFalse(t) | JmpIfTruePeek(t) | JmpIfFalsePeek(t) | CmpJf(_, t) => {
                if (*t as usize) <= n {
                    is_target[*t as usize] = true;
                }
            }
            _ => {}
        }
    }
    is_target
}

/// The base opcode of an IR instruction that [`crate::vm::binop`] can
/// evaluate — the fusable/foldable two-operand set. `None` for everything
/// else.
fn binop_sub(insn: &Insn) -> Option<u8> {
    use Insn::*;
    Some(match insn {
        Add => op::ADD,
        Sub => op::SUB,
        Mul => op::MUL,
        Div => op::DIV,
        Rem => op::REM,
        Pow => op::POW,
        BitAnd => op::BIT_AND,
        BitOr => op::BIT_OR,
        BitXor => op::BIT_XOR,
        Shl => op::SHL,
        Shr => op::SHR,
        Lt => op::LT,
        Le => op::LE,
        Gt => op::GT,
        Ge => op::GE,
        Eq => op::EQ,
        Ne => op::NE,
        _ => return None,
    })
}

/// Globals whose value is fixed for the whole run, as `Some(value)`.
///
/// A global qualifies when it is **predefined** (so its value is the
/// `GlobalDef.init` the engine installs, not something init computes),
/// **never the target of a `StoreG`** anywhere in the program, and **not
/// exported** — `Engine::set_var` and the `vars` API refuse a
/// non-exported global, so the host cannot write it either. `pixelCount`
/// is excluded by name: the engine writes it straight into `vm.globals`
/// without any `StoreG`, so the `StoreG` scan would not catch it.
///
/// Assigning to a predefined name is legal (`PI = 3` compiles) — it just
/// disqualifies that name here, for that pattern.
fn frozen_globals(fns: &[FnIr], globals: &[GlobalDef]) -> Vec<Option<Fx>> {
    let mut out: Vec<Option<Fx>> = globals
        .iter()
        .map(|g| {
            if g.predefined && !g.export && g.name != "pixelCount" {
                Some(g.init)
            } else {
                None
            }
        })
        .collect();
    for f in fns {
        for insn in &f.code {
            if let Insn::StoreG(i) = insn {
                if let Some(slot) = out.get_mut(*i as usize) {
                    *slot = None;
                }
            }
        }
    }
    out
}

/// Constant folding (Gitea #312), run before the superinstruction
/// peephole. Two rewrites, applied to a fixed point:
///
/// 1. **Literal arithmetic.** `Const a; Const b; <binop>` → `Const (a⊕b)`
///    and `Const a; <unop>` → `Const (⊕a)`. The value comes from
///    [`crate::vm::binop`] and the same `Fx` operators the interpreter's
///    own arms use, so the folded word is bit-identical to the one the VM
///    would have pushed. `2 * PI / 3`, `1 / 2` and `-1` are the everyday
///    shapes — `-1` alone is worth an instruction, since the parser hands
///    the compiler `Neg(Num(1))`, not `Num(-1)`.
///
/// 2. **Frozen-global reads**, `LoadG g` → `Const v`, but ONLY where the
///    constant lowers to fewer or cheaper instructions than the load.
///    That is not automatic: `LoadG` is a one-word instruction that fuses
///    with a following `LoadL` (`LoadGL`), while a `Const` is two words
///    and fuses with nothing on its left — substituting blindly turns
///    `PI * x` (`LoadG; LoadL; Mul` ⇒ `LoadGL; Mul`, 2 ops) into 3 ops.
///    The profitable successors are exactly:
///      * a binop        — `Const; <binop>` fuses to `ConstOp`, and a
///                         preceding `LoadL` to `LoadLConstOp`: 2 ops → 1.
///      * `CallBuiltin`  — `Const; CallBuiltin` fuses to `CallBuiltinC`:
///                         2 ops → 1.
///      * `Const c; <binop>` — rule 1 then collapses all three into one
///                         `Const`: the same op count and word count as
///                         the `LoadGConstOp` it replaces, but a plain
///                         push instead of an arithmetic op, and the fold
///                         can keep travelling up the expression.
///
/// 3. **Operand order**, `Const c; <load>; <commutative op>` →
///    `<load>; Const c; <commutative op>`, so that the constant is where
///    the peephole can fuse it. See [`commute_consts`].
///
/// All three rewrites obey the peephole's two contracts (see
/// `.claude/rules/vm-bytecode.md`): nothing folds across a JUMP TARGET,
/// and nothing folds across a SOURCE POSITION, so the debugger still
/// stops once per source line.
fn const_fold(
    mut code: Vec<Insn>,
    mut pos: Vec<(u32, u32)>,
    frozen: &[Option<Fx>],
) -> (Vec<Insn>, Vec<(u32, u32)>) {
    if pos.len() != code.len() {
        return (code, pos);
    }
    // A substitution can expose a fold and a fold can expose a
    // substitution (`PI * PI2`), so iterate; every round either shrinks
    // the code or turns a `LoadG` into a `Const`, so this terminates.
    for _ in 0..4 {
        let subst = subst_frozen_globals(&mut code, &pos, frozen);
        let swapped = commute_consts(&mut code, &pos);
        let (c, p, folded) = fold_literals(code, pos);
        code = c;
        pos = p;
        if !subst && !swapped && !folded {
            break;
        }
    }
    (code, pos)
}

/// Rewrite rule 2 of [`const_fold`]. In place: no instruction is added or
/// removed, so positions and jump targets are untouched. Returns whether
/// anything changed.
fn subst_frozen_globals(code: &mut [Insn], pos: &[(u32, u32)], frozen: &[Option<Fx>]) -> bool {
    use Insn::*;
    let is_target = jump_targets(code);
    let n = code.len();
    let mut changed = false;
    for i in 0..n {
        let LoadG(g) = code[i] else { continue };
        let Some(Some(v)) = frozen.get(g as usize).copied() else {
            continue;
        };
        // the successor must be this statement's fall-through successor,
        // or the peephole would not have fused with it anyway
        if !(i + 1 < n && !is_target[i + 1] && pos[i + 1] == pos[i]) {
            continue;
        }
        let profitable = match &code[i + 1] {
            insn if binop_sub(insn).is_some() => true,
            CallBuiltin { argc, .. } => *argc >= 1,
            // `PI2 * x`: the constant is on the WRONG side to fuse, but
            // `commute_consts` moves it over as soon as it is a `Const`,
            // so check that pass's guards here rather than its result.
            next if push_kind(next).is_some() => {
                i + 2 < n
                    && !is_target[i + 1]
                    && !is_target[i + 2]
                    && pos[i + 1] == pos[i]
                    && pos[i + 2] == pos[i]
                    && (binop_sub(&code[i + 2]).is_some() && matches!(code[i + 1], Const(Value::Num(_)))
                        || mirror_op(&code[i + 2]).is_some())
            }
            _ => false,
        };
        if profitable {
            code[i] = Const(Value::Num(v));
            changed = true;
        }
    }
    changed
}

/// Rewrite rule 1 of [`const_fold`]: evaluate literal arithmetic.
///
/// Instructions fold into the *output* as it is built, so a chain like
/// `Const 1; Const 2; Add; Const 3; Mul` collapses in one sweep.
/// `origin[k]` is the old index of the first instruction that produced
/// `out[k]`, and it is what the position and jump-target checks are made
/// against: the interior indices of an earlier fold were checked when
/// that fold happened, and all of them carried that same position.
fn fold_literals(code: Vec<Insn>, pos: Vec<(u32, u32)>) -> (Vec<Insn>, Vec<(u32, u32)>, bool) {
    use Insn::*;
    let n = code.len();
    let is_target = jump_targets(&code);
    let mut out: Vec<Insn> = Vec::with_capacity(n);
    let mut outpos: Vec<(u32, u32)> = Vec::with_capacity(n);
    let mut origin: Vec<usize> = Vec::with_capacity(n);
    // old instruction index → new instruction index (jump retargeting)
    let mut map = alloc::vec![0u32; n + 1];
    let mut changed = false;
    for i in 0..n {
        let insn = code[i];
        let mut folded = false;
        // an instruction a jump lands on can never vanish into its
        // predecessor
        if !is_target[i] {
            if matches!(insn, Neg | Not | BitNot) {
                if let Some(&Const(Value::Num(a))) = out.last() {
                    let k = *origin.last().unwrap();
                    if pos[i] == pos[k] {
                        // exactly the VM's own arms
                        let v = match insn {
                            Neg => -a,
                            Not => {
                                if Value::Num(a).truthy() {
                                    Fx::ZERO
                                } else {
                                    Fx::ONE
                                }
                            }
                            _ => !a,
                        };
                        let at = out.len() - 1;
                        out[at] = Const(Value::Num(v));
                        folded = true;
                    }
                }
            } else if let Some(sub) = binop_sub(&insn) {
                if out.len() >= 2 {
                    let hi = out.len() - 1;
                    if let (Const(Value::Num(a)), Const(Value::Num(b))) = (out[hi - 1], out[hi]) {
                        let ka = origin[hi - 1];
                        let kb = origin[hi];
                        if pos[i] == pos[ka] && pos[kb] == pos[ka] && !is_target[kb] {
                            // the interpreter's own evaluator, so the
                            // folded word cannot drift from the executed one
                            let v = crate::vm::binop(sub, Value::Num(a), Value::Num(b));
                            out.pop();
                            outpos.pop();
                            origin.pop();
                            let at = out.len() - 1;
                            out[at] = Const(v);
                            folded = true;
                        }
                    }
                }
            }
        }
        if folded {
            let at = (out.len() - 1) as u32;
            // every old index absorbed into this constant now maps to it;
            // only jump targets are ever read back, and none of the
            // absorbed indices is one
            let from = origin[out.len() - 1];
            for slot in map.iter_mut().take(i + 1).skip(from) {
                *slot = at;
            }
            changed = true;
        } else {
            map[i] = out.len() as u32;
            out.push(insn);
            outpos.push(pos[i]);
            origin.push(i);
        }
    }
    map[n] = out.len() as u32;
    for insn in &mut out {
        match insn {
            Jmp(t) | JmpIfFalse(t) | JmpIfTruePeek(t) | JmpIfFalsePeek(t) | CmpJf(_, t) => {
                *t = map[(*t as usize).min(n)];
            }
            _ => {}
        }
    }
    (out, outpos, changed)
}

/// Rewrite rule 3 of [`const_fold`]: put the constant operand of a
/// commutative binary operation on the RIGHT, where the peephole can fuse
/// it. `2 * x` and `PI2 * x` are written that way all over `library/` and
/// cost three instructions (`Const; LoadL; Mul` — a `Const` fuses with
/// what follows it, never with what follows the load) where `x * 2` costs
/// one (`LoadLConstOp`).
///
/// Only ever swaps two SINGLE PUSHES — `Const`, `LoadL`, `LoadG` — none of
/// which can fail or have an effect, so their evaluation order is
/// unobservable. The operator is either commutative on `Fx` (`+ * & | ^`,
/// all of which the VM evaluates as `a.num() ⊕ b.num()`) or a relational
/// one replaced by its MIRROR (`2 < x` → `x > 2`), which is the same
/// predicate on the total order of a 16.16 word.
fn commute_consts(code: &mut [Insn], pos: &[(u32, u32)]) -> bool {
    let is_target = jump_targets(code);
    let n = code.len();
    let mut changed = false;
    for i in 2..n {
        let Some(mirrored) = mirror_op(&code[i]) else { continue };
        // A jump landing on the operator would skip the swap but still run
        // the mirrored opcode; one landing on the second push would run it
        // against a stack the swap has reordered. Landing on the FIRST push
        // is fine — both pushes then run, in either order, to the same
        // stack. Fusion also needs all three in one statement.
        if is_target[i] || is_target[i - 1] {
            continue;
        }
        if pos[i] != pos[i - 2] || pos[i - 1] != pos[i - 2] {
            continue;
        }
        let (Some(a), Some(b)) = (push_kind(&code[i - 2]), push_kind(&code[i - 1])) else {
            continue;
        };
        if fused_ops(b, a) < fused_ops(a, b) {
            code.swap(i - 2, i - 1);
            code[i] = mirrored;
            changed = true;
        }
    }
    changed
}

/// The instruction to use when this operator's two operands are swapped,
/// or `None` if swapping them is not an identity. `Div`, `Sub`, `Rem`,
/// `Pow` and the shifts have no mirror; `Eq`/`Ne` are left out because
/// `value_eq` is only defined here for numbers and this saves nothing
/// (three sites in the whole library).
fn mirror_op(insn: &Insn) -> Option<Insn> {
    use Insn::*;
    Some(match insn {
        Add => Add,
        Mul => Mul,
        BitAnd => BitAnd,
        BitOr => BitOr,
        BitXor => BitXor,
        Lt => Gt,
        Gt => Lt,
        Le => Ge,
        Ge => Le,
        _ => return None,
    })
}

/// A push with no operand evaluation of its own: `0` a constant, `1` a
/// local read, `2` a global read. Anything else disqualifies the operand
/// from being reordered.
fn push_kind(insn: &Insn) -> Option<u8> {
    match insn {
        Insn::Const(Value::Num(_)) => Some(0),
        Insn::LoadL(_) => Some(1),
        Insn::LoadG(_) => Some(2),
        _ => None,
    }
}

/// How many instructions `[first, second, <binop>]` lowers to once
/// [`fold_literals`] and [`peephole`] have run — the whole point of the
/// swap. `Const` fuses with the operator (`ConstOp`) and with a load in
/// front of it (`LoadLConstOp`/`LoadGConstOp`); two loads fuse with each
/// other (`LoadLL`/`LoadLG`/`LoadGL`) but not with the operator, and two
/// global reads fuse with nothing at all.
fn fused_ops(first: u8, second: u8) -> u8 {
    match (first, second) {
        (_, 0) => 1,          // <push>; Const; op  → *ConstOp (or folded)
        (0, _) => 3,          // Const; <load>; op  → nothing fuses
        (2, 2) => 3,          // LoadG; LoadG; op   → no LoadGG template
        _ => 2,               // LoadLL / LoadLG / LoadGL, then the op
    }
}

fn peephole(code: Vec<Insn>, pos: Vec<(u32, u32)>) -> (Vec<Insn>, Vec<(u32, u32)>) {
    use Insn::*;
    let n = code.len();
    // Positions are parallel to the code; without them the "never fuse
    // across a source position" rule cannot be checked, so fuse nothing.
    if pos.len() != n {
        return (code, pos);
    }
    let is_target = jump_targets(&code);
    // May instructions i..i+len fuse into one? (i itself may be a target.)
    let joinable = |i: usize, len: usize| {
        i + len <= n
            && (1..len).all(|k| !is_target[i + k] && pos[i + k] == pos[i])
    };
    let sub = binop_sub;
    let cmp = |insn: &Insn| -> Option<u8> {
        match sub(insn) {
            Some(o) if crate::bytecode::is_cmp_sub(o) => Some(o),
            _ => None,
        }
    };

    let mut out: Vec<Insn> = Vec::with_capacity(n);
    let mut outpos: Vec<(u32, u32)> = Vec::with_capacity(n);
    // old instruction index → new instruction index. Only indices that are
    // jump targets are ever read, and those are never fused away.
    let mut map = alloc::vec![0u32; n + 1];
    let mut i = 0usize;
    while i < n {
        let at = out.len() as u32;
        let (fused, len) = match &code[i..] {
            // 3-instruction templates first (leftmost-longest).
            [LoadG(g), LoadL(a), LoadIdx, ..] if joinable(i, 3) => (LoadGLIdx(*g, *a), 3),
            [Const(Value::Num(c1)), Const(Value::Num(c2)), CallBuiltin { b, argc }, ..]
                if joinable(i, 3) && *argc >= 2 =>
            {
                (
                    CallBuiltinCC {
                        b: *b,
                        argc: *argc,
                        c1: *c1,
                        c2: *c2,
                    },
                    3,
                )
            }
            [LoadL(a), Const(Value::Num(c)), o, ..] if joinable(i, 3) && sub(o).is_some() => {
                (LoadLConstOp(*a, sub(o).unwrap(), *c), 3)
            }
            [LoadG(g), Const(Value::Num(c)), o, ..] if joinable(i, 3) && sub(o).is_some() => {
                (LoadGConstOp(*g, sub(o).unwrap(), *c), 3)
            }
            // 2-instruction templates.
            [StoreL(a), Pop, ..] if joinable(i, 2) => (StoreLPop(*a), 2),
            [StoreG(g), Pop, ..] if joinable(i, 2) => (StoreGPop(*g), 2),
            [LoadL(a), LoadIdx, ..] if joinable(i, 2) => (LoadLIdx(*a), 2),
            [LoadL(a), LoadL(b), ..] if joinable(i, 2) => (LoadLL(*a, *b), 2),
            [LoadL(a), LoadG(g), ..] if joinable(i, 2) => (LoadLG(*a, *g), 2),
            [LoadG(g), LoadL(a), ..] if joinable(i, 2) => (LoadGL(*g, *a), 2),
            [Const(Value::Num(c)), o, ..] if joinable(i, 2) && sub(o).is_some() => {
                (ConstOp(sub(o).unwrap(), *c), 2)
            }
            [Const(Value::Num(c)), CallBuiltin { b, argc }, ..] if joinable(i, 2) && *argc >= 1 => {
                (
                    CallBuiltinC {
                        b: *b,
                        argc: *argc,
                        c: *c,
                    },
                    2,
                )
            }
            [o, JmpIfFalse(t), ..] if joinable(i, 2) && cmp(o).is_some() => {
                (CmpJf(cmp(o).unwrap(), *t), 2)
            }
            [Pop, RetNull, ..] if joinable(i, 2) => (PopRetNull, 2),
            _ => (code[i], 1),
        };
        for k in 0..len {
            map[i + k] = at;
        }
        out.push(fused);
        outpos.push(pos[i]);
        i += len;
    }
    map[n] = out.len() as u32;
    // Jump operands were instruction indices into the old code.
    for insn in &mut out {
        match insn {
            Jmp(t) | JmpIfFalse(t) | JmpIfTruePeek(t) | JmpIfFalsePeek(t) | CmpJf(_, t) => {
                *t = map[(*t as usize).min(n)];
            }
            _ => {}
        }
    }
    (out, outpos)
}

/// Encoded length of one IR instruction in words.
fn insn_len(insn: &Insn) -> u32 {
    use Insn::*;
    match insn {
        Const(Value::Num(_)) | Const(Value::Arr(_)) => 2,
        ConstOp(..) | LoadLConstOp(..) | LoadGConstOp(..) | CallBuiltinC { .. } | CmpJf(..) => 2,
        CallBuiltinCC { .. } => 3,
        _ => 1,
    }
}

fn emit_insn(out: &mut Vec<u32>, insn: &Insn, offsets: &[u32]) {
    use crate::bytecode::enc;
    use Insn::*;
    let target = |t: u32| offsets.get(t as usize).copied().unwrap_or(*offsets.last().unwrap());
    let w = match insn {
        Const(Value::Num(v)) => {
            out.push(enc::bare(op::CONST_NUM));
            v.raw() as u32
        }
        // Arr constants don't exist in compiler output; encode zero.
        Const(Value::Arr(_)) => {
            out.push(enc::bare(op::CONST_NUM));
            0
        }
        Const(Value::Fun(i)) => enc::with_u16(op::CONST_FUN, *i as u16),
        Const(Value::Builtin(b)) => enc::with_u16(op::CONST_BUILTIN, *b as u16),
        LoadG(i) => enc::with_u16(op::LOAD_G, *i),
        StoreG(i) => enc::with_u16(op::STORE_G, *i),
        LoadL(i) => enc::with_u8(op::LOAD_L, *i),
        StoreL(i) => enc::with_u8(op::STORE_L, *i),
        LoadIdx => enc::bare(op::LOAD_IDX),
        StoreIdx => enc::bare(op::STORE_IDX),
        ArrLen => enc::bare(op::ARR_LEN),
        NewArray(n) => enc::with_u16(op::NEW_ARRAY, *n),
        ConstArr(d) => enc::with_u16(op::CONST_ARR, *d),
        Assert(m) => enc::with_u16(op::ASSERT, *m),
        Dup => enc::bare(op::DUP),
        Dup2 => enc::bare(op::DUP2),
        Pop => enc::bare(op::POP),
        Add => enc::bare(op::ADD),
        Sub => enc::bare(op::SUB),
        Mul => enc::bare(op::MUL),
        Div => enc::bare(op::DIV),
        Rem => enc::bare(op::REM),
        Pow => enc::bare(op::POW),
        Neg => enc::bare(op::NEG),
        Not => enc::bare(op::NOT),
        BitNot => enc::bare(op::BIT_NOT),
        BitAnd => enc::bare(op::BIT_AND),
        BitOr => enc::bare(op::BIT_OR),
        BitXor => enc::bare(op::BIT_XOR),
        Shl => enc::bare(op::SHL),
        Shr => enc::bare(op::SHR),
        Lt => enc::bare(op::LT),
        Le => enc::bare(op::LE),
        Gt => enc::bare(op::GT),
        Ge => enc::bare(op::GE),
        Eq => enc::bare(op::EQ),
        Ne => enc::bare(op::NE),
        Jmp(t) => enc::with_u24(op::JMP, target(*t)),
        JmpIfFalse(t) => enc::with_u24(op::JMP_IF_FALSE, target(*t)),
        JmpIfTruePeek(t) => enc::with_u24(op::JMP_IF_TRUE_PEEK, target(*t)),
        JmpIfFalsePeek(t) => enc::with_u24(op::JMP_IF_FALSE_PEEK, target(*t)),
        CallFn { fn_idx, argc } => enc::call(op::CALL_FN, *fn_idx, *argc),
        CallBuiltin { b, argc } => enc::call(op::CALL_BUILTIN, *b, *argc),
        CallValue { argc } => enc::with_u8(op::CALL_VALUE, *argc),
        Ret => enc::bare(op::RET),
        RetNull => enc::bare(op::RET_NULL),

        // superinstructions — the trailing immediate/target word (where
        // there is one) is pushed after the opcode word, like Const.
        StoreLPop(n) => enc::with_u8(op::STORE_L_POP, *n),
        StoreGPop(n) => enc::with_u16(op::STORE_G_POP, *n),
        LoadLL(a, b) => enc::with_u8_u8(op::LOAD_LL, *a, *b),
        LoadLG(a, g) => enc::with_u8_u16(op::LOAD_LG, *a, *g),
        LoadGL(g, a) => enc::with_u16_u8(op::LOAD_GL, *g, *a),
        LoadLIdx(a) => enc::with_u8(op::LOAD_L_IDX, *a),
        LoadGLIdx(g, a) => enc::with_u16_u8(op::LOAD_G_L_IDX, *g, *a),
        ConstOp(sub, c) => {
            out.push(enc::with_u8(op::CONST_OP, *sub));
            c.raw() as u32
        }
        LoadLConstOp(a, sub, c) => {
            out.push(enc::with_u8_u8(op::LOAD_L_CONST_OP, *a, *sub));
            c.raw() as u32
        }
        LoadGConstOp(g, sub, c) => {
            out.push(enc::with_u16_u8(op::LOAD_G_CONST_OP, *g, *sub));
            c.raw() as u32
        }
        CallBuiltinC { b, argc, c } => {
            out.push(enc::call(op::CALL_BUILTIN_C, *b, *argc));
            c.raw() as u32
        }
        CallBuiltinCC { b, argc, c1, c2 } => {
            out.push(enc::call(op::CALL_BUILTIN_CC, *b, *argc));
            out.push(c1.raw() as u32);
            c2.raw() as u32
        }
        CmpJf(sub, t) => {
            out.push(enc::with_u8(op::CMP_JF, *sub));
            target(*t)
        }
        PopRetNull => enc::bare(op::POP_RET_NULL),
    };
    out.push(w);
}

/// Predefined constants (name, value). `pixelCount` is global 0, written by
/// the engine before init runs. Constants use literal quantization (16.15,
/// truncated) — oracle-confirmed: PI reads back as raw 205886 on hardware.
fn predefined() -> Vec<GlobalDef> {
    use core::f64::consts;
    let g = |name: &str, v: f64| GlobalDef {
        name: name.to_string(),
        export: false,
        init: Fx::from_f64_lit(v),
        predefined: true,
    };
    let gi = |name: &str, v: i32| GlobalDef {
        name: name.to_string(),
        export: false,
        init: Fx::from_int(v),
        predefined: true,
    };
    alloc::vec![
        g("pixelCount", 0.0),
        g("E", consts::E),
        g("PI", consts::PI),
        g("PI2", consts::TAU),
        g("PI3_4", consts::PI * 0.75),
        g("PISQ", consts::PI * consts::PI),
        g("LN2", consts::LN_2),
        g("LN10", consts::LN_10),
        g("LOG2E", consts::LOG2_E),
        g("LOG10E", consts::LOG10_E),
        g("SQRT1_2", consts::FRAC_1_SQRT_2),
        g("SQRT2", consts::SQRT_2),
        // `null` compiles on PB and acts as 0 at runtime (oracle-verified
        // 2026-08-22). `undefined` is REJECTED by the PB compiler
        // ("Undefined symbol") — keeping it as 0 here is a deliberate
        // Luxel leniency, same class as 1-arg `square`.
        gi("null", 0),
        gi("undefined", 0),
        // GPIO constants, oracle-probed from fw 3.67
        gi("LOW", 0),
        gi("HIGH", 1),
        gi("INPUT", 1),
        gi("OUTPUT", 2),
        gi("INPUT_PULLUP", 5),
        gi("INPUT_PULLDOWN", 9),
        gi("OUTPUT_OPEN_DRAIN", 18),
        gi("ANALOG", 192),
    ]
}

/// One function in compiler IR form (see [`Insn`]); [`assemble`] turns the
/// full set into the byte-coded [`Program`].
struct FnIr {
    name: String,
    params: u8,
    code: Vec<Insn>,
    /// (line, col) per instruction — collapsed to runs at assembly.
    pos: Vec<(u32, u32)>,
    /// Local slot names, params first (count = the local slot count).
    local_names: Vec<String>,
}

impl FnIr {
    fn placeholder(name: String) -> FnIr {
        FnIr {
            name,
            params: 0,
            code: Vec::new(),
            pos: Vec::new(),
            local_names: Vec::new(),
        }
    }
}

struct Compiler<'s> {
    src: &'s str,
    globals: Vec<GlobalDef>,
    fns: Vec<FnIr>,
    /// Const-array pool: all-numeric array literals, deduplicated by
    /// content (pixel-art patterns repeat the same rows hundreds of times).
    /// Const-array pool contents (raw 16.16 words), deduplicated via `data_map`.
    data_arrays: Vec<Vec<i32>>,
    data_map: BTreeMap<Vec<i32>, u16>,
    /// `assert()` message pool, deduplicated.
    assert_msgs: Vec<String>,
    /// Top-level named functions: (name, fn index, exported).
    named_fns: Vec<(String, u16, bool)>,
    exported_fns: Vec<(String, u16)>,
    /// Function names the pattern assigns to — demoted to plain global
    /// variables holding a function value (JS-style mutable bindings).
    demoted: Vec<String>,
    /// Names declared `const` at the top level (reassignment is an error).
    const_globals: BTreeSet<String>,
    /// Current emit recursion depth — bounded like the parser's so a deep
    /// AST becomes a compile error instead of a stack overflow on the
    /// firmware's small task stack.
    depth: u32,
}

/// Matches the parser's nesting bound (see parse.rs MAX_DEPTH).
const MAX_EMIT_DEPTH: u32 = 60;

enum Place {
    Local(u8),
    Global(u16),
    Func(u16),
    Builtin(u16),
}

impl<'s> Compiler<'s> {
    fn new(src: &'s str) -> Compiler<'s> {
        Compiler {
            src,
            globals: predefined(),
            fns: alloc::vec![FnIr::placeholder("<init>".to_string())],
            named_fns: Vec::new(),
            exported_fns: Vec::new(),
            demoted: Vec::new(),
            const_globals: BTreeSet::new(),
            data_arrays: Vec::new(),
            data_map: BTreeMap::new(),
            assert_msgs: Vec::new(),
            depth: 0,
        }
    }

    fn global_idx(&self, name: &str) -> Option<u16> {
        self.globals
            .iter()
            .position(|g| g.name == name)
            .map(|i| i as u16)
    }

    fn ensure_global(&mut self, name: &str, export: bool, span: Span) -> Result<u16, Diagnostic> {
        if let Some(i) = self.global_idx(name) {
            if export {
                self.globals[i as usize].export = true;
            }
            return Ok(i);
        }
        if self.globals.len() >= MAX_GLOBALS {
            return Err(Diagnostic::new(
                span,
                format!("too many globals (max {MAX_GLOBALS})"),
            ));
        }
        self.globals.push(GlobalDef {
            name: name.to_string(),
            export,
            init: Fx::ZERO,
            predefined: false,
        });
        Ok((self.globals.len() - 1) as u16)
    }

    // ---- pass 1: collect functions and globals ----

    fn collect(&mut self, top: &[Stmt]) -> Result<(), Diagnostic> {
        // Register named functions first so calls resolve in any order.
        // PB flattens ALL function declarations to global scope regardless of
        // nesting (corpus patterns call functions declared inside other
        // functions), and duplicates are allowed — the last definition wins.
        register_fns(self, top);
        // reserve placeholder defs so indices are stable during emission
        for (name, _, _) in self.named_fns.clone() {
            self.fns.push(FnIr::placeholder(name));
        }
        // top level: every var / assignment is a global (even inside blocks)
        let empty: Vec<String> = Vec::new();
        for s in top {
            match &s.kind {
                StmtKind::Func { params, body, .. } => {
                    let locals = function_scope(params, body);
                    self.scan_stmts(body, &locals, false)?;
                }
                _ => self.scan_stmt(s, &empty, true)?,
            }
        }
        Ok(())
    }

    fn scan_stmts(
        &mut self,
        stmts: &[Stmt],
        locals: &[String],
        top: bool,
    ) -> Result<(), Diagnostic> {
        for s in stmts {
            self.scan_stmt(s, locals, top)?;
        }
        Ok(())
    }

    fn scan_stmt(&mut self, s: &Stmt, locals: &[String], top: bool) -> Result<(), Diagnostic> {
        match &s.kind {
            // nested named function: its own scope, like a lambda (the name
            // becomes a hoisted local of the enclosing function)
            StmtKind::Func { params, body, .. } => {
                let scope = function_scope(params, body);
                self.scan_stmts(body, &scope, false)
            }
            StmtKind::Var {
                export,
                kind,
                decls,
            } => {
                for d in decls {
                    if top {
                        self.ensure_global(&d.name, *export, d.span)?;
                        if *kind == DeclKind::Const {
                            self.const_globals.insert(d.name.clone());
                        }
                    }
                    if let Some(init) = &d.init {
                        self.scan_expr(init, locals)?;
                    }
                }
                Ok(())
            }
            StmtKind::Expr(e) => self.scan_expr(e, locals),
            StmtKind::If { cond, then, els } => {
                self.scan_expr(cond, locals)?;
                self.scan_stmt(then, locals, top)?;
                if let Some(e) = els {
                    self.scan_stmt(e, locals, top)?;
                }
                Ok(())
            }
            StmtKind::While { cond, body } => {
                self.scan_expr(cond, locals)?;
                self.scan_stmt(body, locals, top)
            }
            StmtKind::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    self.scan_stmt(i, locals, top)?;
                }
                if let Some(c) = cond {
                    self.scan_expr(c, locals)?;
                }
                if let Some(u) = update {
                    self.scan_expr(u, locals)?;
                }
                self.scan_stmt(body, locals, top)
            }
            StmtKind::Switch { disc, cases } => {
                self.scan_expr(disc, locals)?;
                for c in cases {
                    if let Some(t) = &c.test {
                        self.scan_expr(t, locals)?;
                    }
                    self.scan_stmts(&c.body, locals, top)?;
                }
                Ok(())
            }
            StmtKind::Block(b) => self.scan_stmts(b, locals, top),
            StmtKind::Assert { cond, .. } => self.scan_expr(cond, locals),
            StmtKind::Return(Some(e)) => self.scan_expr(e, locals),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Empty => {
                Ok(())
            }
        }
    }

    fn scan_expr(&mut self, e: &Expr, locals: &[String]) -> Result<(), Diagnostic> {
        match &e.kind {
            ExprKind::Num(_) => Ok(()),
            ExprKind::Ident(_) => Ok(()), // reads checked during emission
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    self.scan_expr(el, locals)?;
                }
                Ok(())
            }
            ExprKind::Unary { expr, .. } => self.scan_expr(expr, locals),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.scan_expr(lhs, locals)?;
                self.scan_expr(rhs, locals)
            }
            ExprKind::Assign { target, value, .. } => {
                self.scan_assign_target(target, locals)?;
                self.scan_expr(value, locals)
            }
            ExprKind::IncDec { target, .. } => self.scan_assign_target(target, locals),
            ExprKind::Ternary { cond, then, els } => {
                self.scan_expr(cond, locals)?;
                self.scan_expr(then, locals)?;
                self.scan_expr(els, locals)
            }
            ExprKind::Call { callee, args } => {
                self.scan_expr(callee, locals)?;
                for a in args {
                    self.scan_expr(a, locals)?;
                }
                Ok(())
            }
            ExprKind::Index { obj, index } => {
                self.scan_expr(obj, locals)?;
                self.scan_expr(index, locals)
            }
            ExprKind::Member { obj, .. } => self.scan_expr(obj, locals),
            ExprKind::Lambda { params, body } => match body {
                LambdaBody::Expr(e) => {
                    let scope = function_scope(params, &[]);
                    self.scan_expr(e, &scope)
                }
                LambdaBody::Block(stmts) => {
                    let scope = function_scope(params, stmts);
                    self.scan_stmts(stmts, &scope, false)
                }
            },
        }
    }

    fn scan_assign_target(&mut self, target: &Expr, locals: &[String]) -> Result<(), Diagnostic> {
        match &target.kind {
            ExprKind::Ident(name) => {
                let is_local = locals.iter().any(|l| l == name);
                if is_local {
                    return Ok(());
                }
                // assigning to a function name demotes it to a plain global
                // variable initialized with the function value (JS-style)
                if self.named_fns.iter().any(|(n, _, _)| n == name)
                    && !self.demoted.iter().any(|n| n == name)
                {
                    self.demoted.push(name.clone());
                }
                // implicit assignment creates a global
                self.ensure_global(name, false, target.span)?;
                Ok(())
            }
            ExprKind::Index { obj, index } => {
                self.scan_expr(obj, locals)?;
                self.scan_expr(index, locals)
            }
            _ => Ok(()),
        }
    }

    // ---- pass 2: emission ----

    fn emit_program(&mut self, top: &[Stmt]) -> Result<(), Diagnostic> {
        // All function declarations (any nesting depth) compile into their
        // registered global slots; duplicates emit in walk order, last wins.
        let mut decls: Vec<&Stmt> = Vec::new();
        walk_fns(top, &mut |s| decls.push(s));
        for s in decls {
            let StmtKind::Func {
                export,
                name,
                params,
                body,
            } = &s.kind
            else {
                unreachable!()
            };
            let idx = self
                .named_fns
                .iter()
                .find(|(n, _, _)| n == name)
                .map(|&(_, i, _)| i)
                .expect("registered in collect");
            let def = self.emit_function(name.clone(), params, body, s.span)?;
            self.fns[idx as usize] = def;
            if *export && !self.exported_fns.iter().any(|(n, _)| n == name) {
                self.exported_fns.push((name.clone(), idx));
            }
        }

        // top-level init (fn 0). Demoted functions (ones the pattern assigns
        // to, making them plain variables) get their global slots initialized
        // first.
        let mut ctx = FnCtx::new(Vec::new(), true);
        for name in self.demoted.clone() {
            let idx = self
                .named_fns
                .iter()
                .find(|(n, _, _)| *n == name)
                .map(|&(_, i, _)| i)
                .expect("demoted implies registered");
            let g = self.ensure_global(&name, false, Span::default())?;
            ctx.push(Insn::Const(Value::Fun(idx as u32)));
            ctx.push(Insn::StoreG(g));
            ctx.push(Insn::Pop);
        }
        for s in top {
            if matches!(s.kind, StmtKind::Func { .. }) {
                continue;
            }
            self.emit_stmt(&mut ctx, s)?;
        }
        ctx.push(Insn::RetNull);
        self.fns[0].pos = ctx.pos; // keep line info for init-time vmerrs
        self.fns[0].code = ctx.code;
        Ok(())
    }

    fn emit_function(
        &mut self,
        name: String,
        params: &[String],
        body: &[Stmt],
        span: Span,
    ) -> Result<FnIr, Diagnostic> {
        let locals = function_scope(params, body);
        if locals.len() > MAX_LOCALS {
            return Err(Diagnostic::new(
                span,
                format!("too many locals in `{name}` (max {MAX_LOCALS})"),
            ));
        }
        let mut ctx = FnCtx::new(locals, false);
        for s in body {
            self.emit_stmt(&mut ctx, s)?;
        }
        ctx.push(Insn::RetNull);
        Ok(ctx.finish(name, params.len() as u8))
    }

    fn emit_lambda(
        &mut self,
        params: &[String],
        body: &LambdaBody,
        span: Span,
    ) -> Result<u16, Diagnostic> {
        let idx = self.fns.len() as u16;
        let name = format!("<lambda#{idx}>");
        // reserve slot first (nested lambdas may allocate more)
        self.fns.push(FnIr::placeholder(name.clone()));
        let def = match body {
            LambdaBody::Expr(e) => {
                let locals = function_scope(params, &[]);
                let mut ctx = FnCtx::new(locals, false);
                ctx.set_pos(line_col(self.src, e.span.start));
                self.emit_expr(&mut ctx, e)?;
                ctx.push(Insn::Ret);
                ctx.finish(name.clone(), params.len() as u8)
            }
            LambdaBody::Block(stmts) => {
                let locals = function_scope(params, stmts);
                if locals.len() > MAX_LOCALS {
                    return Err(Diagnostic::new(
                        span,
                        "too many locals in lambda".to_string(),
                    ));
                }
                let mut ctx = FnCtx::new(locals, false);
                for s in stmts {
                    self.emit_stmt(&mut ctx, s)?;
                }
                ctx.push(Insn::RetNull);
                ctx.finish(name, params.len() as u8)
            }
        };
        self.fns[idx as usize] = def;
        Ok(idx)
    }

    fn resolve(&self, ctx: &FnCtx, name: &str, span: Span) -> Result<Place, Diagnostic> {
        if let Some(i) = ctx.locals.iter().position(|l| l == name) {
            return Ok(Place::Local(i as u8));
        }
        if !self.demoted.iter().any(|n| n == name) {
            if let Some(&(_, idx, _)) = self.named_fns.iter().find(|(n, _, _)| n == name) {
                return Ok(Place::Func(idx));
            }
        }
        if let Some(i) = self.global_idx(name) {
            return Ok(Place::Global(i));
        }
        if let Some(b) = lookup_builtin(name) {
            return Ok(Place::Builtin(b));
        }
        Err(Diagnostic::new(
            span,
            format!("unknown identifier `{name}`"),
        ))
    }

    fn emit_stmt(&mut self, ctx: &mut FnCtx, s: &Stmt) -> Result<(), Diagnostic> {
        self.depth += 1;
        if self.depth > MAX_EMIT_DEPTH {
            self.depth -= 1;
            return Err(Diagnostic::new(s.span, "statement nesting too deep"));
        }
        let r = self.emit_stmt_inner(ctx, s);
        self.depth -= 1;
        r
    }

    fn emit_stmt_inner(&mut self, ctx: &mut FnCtx, s: &Stmt) -> Result<(), Diagnostic> {
        ctx.set_pos(line_col(self.src, s.span.start));
        match &s.kind {
            StmtKind::Empty => Ok(()),
            // nested named functions were already bound at function entry
            StmtKind::Func { .. } => Ok(()),
            StmtKind::Var { decls, kind, .. } => {
                for d in decls {
                    if let Some(init) = &d.init {
                        self.emit_expr(ctx, init)?;
                        // a declaration's own store is always allowed; it also
                        // registers const-ness so later assignments error
                        self.emit_store_decl(ctx, &d.name, d.span, *kind == DeclKind::Const)?;
                        ctx.push(Insn::Pop);
                    }
                }
                Ok(())
            }
            StmtKind::Expr(e) => {
                self.emit_expr_discard(ctx, e)?;
                ctx.push(Insn::Pop);
                Ok(())
            }
            StmtKind::If { cond, then, els } => {
                self.emit_expr(ctx, cond)?;
                let jf = ctx.emit_placeholder();
                self.emit_stmt(ctx, then)?;
                if let Some(els) = els {
                    let jend = ctx.emit_placeholder();
                    ctx.patch(jf, Insn::JmpIfFalse(ctx.here()));
                    self.emit_stmt(ctx, els)?;
                    ctx.patch(jend, Insn::Jmp(ctx.here()));
                } else {
                    ctx.patch(jf, Insn::JmpIfFalse(ctx.here()));
                }
                Ok(())
            }
            StmtKind::While { cond, body } => {
                let start = ctx.here();
                self.emit_expr(ctx, cond)?;
                let jf = ctx.emit_placeholder();
                ctx.loops.push(BreakFrame::loop_frame());
                self.emit_stmt(ctx, body)?;
                ctx.push(Insn::Jmp(start));
                let frame = ctx.loops.pop().unwrap();
                let end = ctx.here();
                ctx.patch(jf, Insn::JmpIfFalse(end));
                for b in frame.breaks {
                    ctx.patch(b, Insn::Jmp(end));
                }
                for c in frame.continues {
                    ctx.patch(c, Insn::Jmp(start));
                }
                Ok(())
            }
            StmtKind::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    self.emit_stmt(ctx, i)?;
                }
                let start = ctx.here();
                let jf = if let Some(c) = cond {
                    self.emit_expr(ctx, c)?;
                    Some(ctx.emit_placeholder())
                } else {
                    None
                };
                ctx.loops.push(BreakFrame::loop_frame());
                self.emit_stmt(ctx, body)?;
                let frame = ctx.loops.pop().unwrap();
                let cont = ctx.here();
                if let Some(u) = update {
                    self.emit_expr_discard(ctx, u)?;
                    ctx.push(Insn::Pop);
                }
                ctx.push(Insn::Jmp(start));
                let end = ctx.here();
                if let Some(jf) = jf {
                    ctx.patch(jf, Insn::JmpIfFalse(end));
                }
                for b in frame.breaks {
                    ctx.patch(b, Insn::Jmp(end));
                }
                for c in frame.continues {
                    ctx.patch(c, Insn::Jmp(cont));
                }
                Ok(())
            }
            // `switch` needs no opcodes of its own — it lowers to the same
            // Dup/Ne/JmpIfFalse/Jmp shapes `if` already uses:
            //
            //   <disc>                       discriminant, kept on the stack
            //   Dup; <test_i>; Ne; JmpIfFalse T_i    (one per `case`, in order)
            //   Pop; Jmp default|end                 (no label matched)
            //   T_i: Pop; Jmp body_i                 (trampolines drop the disc)
            //   body_0 … body_n                      (source order ⇒ fall-through)
            //   end:
            //
            // Every path pops the discriminant exactly once *before* entering
            // a body, so the bodies — and any `break`/`return` out of them —
            // run on a stack that looks like plain statement context.
            StmtKind::Switch { disc, cases } => {
                self.emit_expr(ctx, disc)?;
                // test chain: one comparison per `case`, `default` skipped
                let mut tests: Vec<Option<usize>> = Vec::with_capacity(cases.len());
                for c in cases {
                    match &c.test {
                        Some(t) => {
                            ctx.push(Insn::Dup);
                            self.emit_expr(ctx, t)?;
                            ctx.push(Insn::Ne);
                            tests.push(Some(ctx.emit_placeholder()));
                        }
                        None => tests.push(None),
                    }
                }
                // fell off the end of the tests: nothing matched
                ctx.push(Insn::Pop);
                let j_nomatch = ctx.emit_placeholder();
                // trampolines
                let mut tramps: Vec<(usize, usize)> = Vec::new();
                for (i, t) in tests.iter().enumerate() {
                    if let Some(p) = *t {
                        let here = ctx.here();
                        ctx.patch(p, Insn::JmpIfFalse(here));
                        ctx.push(Insn::Pop);
                        tramps.push((i, ctx.emit_placeholder()));
                    }
                }
                // bodies, in source order so fall-through is just "no jump"
                ctx.loops.push(BreakFrame::switch_frame());
                let mut body_start: Vec<u32> = Vec::with_capacity(cases.len());
                for c in cases {
                    body_start.push(ctx.here());
                    for s in &c.body {
                        self.emit_stmt(ctx, s)?;
                    }
                }
                let frame = ctx.loops.pop().unwrap();
                let end = ctx.here();
                for (i, p) in tramps {
                    ctx.patch(p, Insn::Jmp(body_start[i]));
                }
                let no_match = cases
                    .iter()
                    .position(|c| c.test.is_none())
                    .map(|i| body_start[i])
                    .unwrap_or(end);
                ctx.patch(j_nomatch, Insn::Jmp(no_match));
                for b in frame.breaks {
                    ctx.patch(b, Insn::Jmp(end));
                }
                debug_assert!(
                    frame.continues.is_empty(),
                    "switch frames take no continues"
                );
                Ok(())
            }
            StmtKind::Block(body) => {
                for s in body {
                    self.emit_stmt(ctx, s)?;
                }
                Ok(())
            }
            StmtKind::Return(v) => {
                match v {
                    Some(e) => {
                        self.emit_expr(ctx, e)?;
                        ctx.push(Insn::Ret);
                    }
                    None => ctx.push(Insn::RetNull),
                }
                Ok(())
            }
            StmtKind::Break => {
                let p = ctx.emit_placeholder();
                // innermost breakable construct — a loop OR a switch
                match ctx.loops.last_mut() {
                    Some(f) => {
                        f.breaks.push(p);
                        Ok(())
                    }
                    None => Err(Diagnostic::new(
                        s.span,
                        "`break` outside a loop or `switch`".to_string(),
                    )),
                }
            }
            StmtKind::Continue => {
                let p = ctx.emit_placeholder();
                // a `switch` is breakable but not continuable: skip past it
                // to the enclosing loop, like JS
                match ctx.loops.iter_mut().rev().find(|f| f.is_loop) {
                    Some(f) => {
                        f.continues.push(p);
                        Ok(())
                    }
                    None => Err(Diagnostic::new(
                        s.span,
                        "`continue` outside a loop".to_string(),
                    )),
                }
            }
            StmtKind::Assert { cond, message } => {
                // Top level only: asserts run once, inline in init. Inside a
                // function they'd fire per frame/pixel; nested in a block
                // they'd be conditional — neither is a declared invariant.
                // depth == 1 ⇔ this statement came straight off the top
                // level (emit_stmt is the only way in, and it counts).
                if !ctx.is_top || self.depth != 1 {
                    return Err(Diagnostic::new(
                        s.span,
                        "assert() is only allowed at the top level of a pattern \
                         (it runs once, as part of initialization)"
                            .to_string(),
                    ));
                }
                self.emit_expr(ctx, cond)?;
                let text = match message {
                    Some(m) => m.clone(),
                    None => self.src[cond.span.start as usize..cond.span.end as usize]
                        .trim()
                        .to_string(),
                };
                let m = self.intern_msg(text, s.span)?;
                ctx.push(Insn::Assert(m));
                Ok(())
            }
        }
    }

    /// Emit a store to a named variable (value on stack; leaves it there).
    /// Rejects reassignment of a `const`.
    fn emit_store(&mut self, ctx: &mut FnCtx, name: &str, span: Span) -> Result<(), Diagnostic> {
        match self.resolve(ctx, name, span)? {
            Place::Local(i) => {
                if ctx.const_locals.contains(&i) {
                    return Err(Diagnostic::new(
                        span,
                        format!("cannot assign to `{name}` — it is declared const"),
                    ));
                }
                ctx.push(Insn::StoreL(i));
            }
            Place::Global(i) => {
                if self.const_globals.contains(name) {
                    return Err(Diagnostic::new(
                        span,
                        format!("cannot assign to `{name}` — it is declared const"),
                    ));
                }
                ctx.push(Insn::StoreG(i));
            }
            Place::Func(_) | Place::Builtin(_) => {
                return Err(Diagnostic::new(span, format!("cannot assign to `{name}`")))
            }
        }
        Ok(())
    }

    /// Emit the store for a declaration's initializer. Bypasses the const
    /// check (a const's own init is legal) and, for const, records the
    /// binding so subsequent assignments are rejected.
    fn emit_store_decl(
        &mut self,
        ctx: &mut FnCtx,
        name: &str,
        span: Span,
        is_const: bool,
    ) -> Result<(), Diagnostic> {
        match self.resolve(ctx, name, span)? {
            Place::Local(i) => {
                ctx.push(Insn::StoreL(i));
                if is_const {
                    ctx.const_locals.insert(i);
                }
            }
            Place::Global(i) => {
                ctx.push(Insn::StoreG(i));
                // const globals already recorded during scanning
            }
            Place::Func(_) | Place::Builtin(_) => {
                return Err(Diagnostic::new(span, format!("cannot assign to `{name}`")))
            }
        }
        Ok(())
    }

    /// Emit an expression whose value the caller pops immediately.
    ///
    /// The only shape that differs from [`emit_expr`] is a POSTFIX
    /// `x++` / `x--` (Gitea #312): its value is defined to be the *old*
    /// one, which the compiler recovers by applying the inverse operation
    /// to the new value (`Const 1; Sub` after an `x++`). When the caller
    /// is about to `Pop` that value, nothing can observe the difference,
    /// so the recovery is not emitted and the statement is two
    /// instructions shorter — three fewer per `for` iteration once the
    /// peephole folds `StoreL; Pop` into `StoreLPop`, which the recovery
    /// was preventing.
    ///
    /// The value is still LEFT ON THE STACK; the caller pops it, exactly
    /// as with `emit_expr`.
    fn emit_expr_discard(&mut self, ctx: &mut FnCtx, e: &Expr) -> Result<(), Diagnostic> {
        let ExprKind::IncDec { inc, target, .. } = &e.kind else {
            return self.emit_expr(ctx, e);
        };
        self.depth += 1;
        if self.depth > MAX_EMIT_DEPTH {
            self.depth -= 1;
            return Err(Diagnostic::new(e.span, "expression nesting too deep"));
        }
        let r = self.emit_incdec(ctx, *inc, target, false);
        self.depth -= 1;
        r
    }

    /// `x++` / `++x` / `x--` / `--x`. `keep_old` asks for the postfix
    /// value (the value BEFORE the update); prefix — and any context that
    /// discards the result — passes `false` and gets the new value.
    fn emit_incdec(
        &mut self,
        ctx: &mut FnCtx,
        inc: bool,
        target: &Expr,
        keep_old: bool,
    ) -> Result<(), Diagnostic> {
        let one = Insn::Const(Value::Num(Fx::ONE));
        let (fwd, inv) = if inc {
            (Insn::Add, Insn::Sub)
        } else {
            (Insn::Sub, Insn::Add)
        };
        match &target.kind {
            ExprKind::Ident(name) => {
                self.emit_expr(ctx, target)?;
                ctx.push(one);
                ctx.push(fwd);
                self.emit_store(ctx, name, target.span)?;
            }
            ExprKind::Index { obj, index } => {
                self.emit_expr(ctx, obj)?;
                self.emit_expr(ctx, index)?;
                ctx.push(Insn::Dup2);
                ctx.push(Insn::LoadIdx);
                ctx.push(one);
                ctx.push(fwd);
                ctx.push(Insn::StoreIdx);
            }
            _ => {
                return Err(Diagnostic::new(
                    target.span,
                    "invalid increment target".to_string(),
                ))
            }
        }
        if keep_old {
            // stack holds the NEW value; recover the old one (exact
            // inverse under wrapping arithmetic)
            ctx.push(one);
            ctx.push(inv);
        }
        Ok(())
    }

    fn emit_expr(&mut self, ctx: &mut FnCtx, e: &Expr) -> Result<(), Diagnostic> {
        self.depth += 1;
        if self.depth > MAX_EMIT_DEPTH {
            self.depth -= 1;
            return Err(Diagnostic::new(e.span, "expression nesting too deep"));
        }
        let r = self.emit_expr_inner(ctx, e);
        self.depth -= 1;
        r
    }

    fn emit_expr_inner(&mut self, ctx: &mut FnCtx, e: &Expr) -> Result<(), Diagnostic> {
        match &e.kind {
            ExprKind::Num(v) => {
                ctx.push(Insn::Const(Value::Num(*v)));
                Ok(())
            }
            ExprKind::Ident(name) => {
                match self.resolve(ctx, name, e.span)? {
                    Place::Local(i) => ctx.push(Insn::LoadL(i)),
                    Place::Global(i) => ctx.push(Insn::LoadG(i)),
                    Place::Func(i) => ctx.push(Insn::Const(Value::Fun(i as u32))),
                    Place::Builtin(b) => ctx.push(Insn::Const(Value::Builtin(b as u32))),
                }
                Ok(())
            }
            ExprKind::ArrayLit(elems) => {
                if elems.len() > u16::MAX as usize {
                    return Err(Diagnostic::new(
                        e.span,
                        "array literal too large".to_string(),
                    ));
                }
                // All-numeric literals intern into the const-array pool
                // (deduplicated — the .rodata of a pattern): the VM shares
                // the data copy-on-write instead of materializing it.
                if let Some(consts) = const_elements(elems) {
                    if let Some(d) = self.intern_data(consts) {
                        ctx.push(Insn::ConstArr(d));
                        return Ok(());
                    }
                    // pool full — fall through to the runtime-built path
                }
                for el in elems {
                    self.emit_expr(ctx, el)?;
                }
                ctx.push(Insn::NewArray(elems.len() as u16));
                Ok(())
            }
            ExprKind::Unary { op, expr } => {
                self.emit_expr(ctx, expr)?;
                match op {
                    UnOp::Neg => ctx.push(Insn::Neg),
                    UnOp::Pos => {} // numeric identity
                    UnOp::Not => ctx.push(Insn::Not),
                    UnOp::BitNot => ctx.push(Insn::BitNot),
                }
                Ok(())
            }
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::And => {
                    self.emit_expr(ctx, lhs)?;
                    let j = ctx.emit_placeholder();
                    ctx.push(Insn::Pop);
                    self.emit_expr(ctx, rhs)?;
                    ctx.patch(j, Insn::JmpIfFalsePeek(ctx.here()));
                    Ok(())
                }
                BinOp::Or => {
                    self.emit_expr(ctx, lhs)?;
                    let j = ctx.emit_placeholder();
                    ctx.push(Insn::Pop);
                    self.emit_expr(ctx, rhs)?;
                    ctx.patch(j, Insn::JmpIfTruePeek(ctx.here()));
                    Ok(())
                }
                _ => {
                    self.emit_expr(ctx, lhs)?;
                    self.emit_expr(ctx, rhs)?;
                    ctx.push(bin_insn(*op));
                    Ok(())
                }
            },
            ExprKind::Assign { op, target, value } => match &target.kind {
                ExprKind::Ident(name) => {
                    if let Some(op) = op {
                        self.emit_expr(ctx, target)?;
                        self.emit_expr(ctx, value)?;
                        ctx.push(bin_insn(*op));
                    } else {
                        self.emit_expr(ctx, value)?;
                    }
                    self.emit_store(ctx, name, target.span)
                }
                ExprKind::Index { obj, index } => {
                    self.emit_expr(ctx, obj)?;
                    self.emit_expr(ctx, index)?;
                    if let Some(op) = op {
                        ctx.push(Insn::Dup2);
                        ctx.push(Insn::LoadIdx);
                        self.emit_expr(ctx, value)?;
                        ctx.push(bin_insn(*op));
                    } else {
                        self.emit_expr(ctx, value)?;
                    }
                    ctx.push(Insn::StoreIdx);
                    Ok(())
                }
                _ => Err(Diagnostic::new(
                    target.span,
                    "invalid assignment target".to_string(),
                )),
            },
            ExprKind::IncDec {
                inc,
                prefix,
                target,
            } => self.emit_incdec(ctx, *inc, target, !*prefix),
            ExprKind::Ternary { cond, then, els } => {
                self.emit_expr(ctx, cond)?;
                let jf = ctx.emit_placeholder();
                self.emit_expr(ctx, then)?;
                let jend = ctx.emit_placeholder();
                ctx.patch(jf, Insn::JmpIfFalse(ctx.here()));
                self.emit_expr(ctx, els)?;
                ctx.patch(jend, Insn::Jmp(ctx.here()));
                Ok(())
            }
            ExprKind::Call { callee, args } => {
                if args.len() > 15 {
                    return Err(Diagnostic::new(e.span, "too many arguments".to_string()));
                }
                let argc = args.len() as u8;
                // method form: a.mutate(f) → arrayMutate(a, f)
                if let ExprKind::Member { obj, name } = &callee.kind {
                    let Some(b) = lookup_method(name) else {
                        return Err(Diagnostic::new(
                            callee.span,
                            format!("unknown method `.{name}()`"),
                        ));
                    };
                    self.emit_expr(ctx, obj)?;
                    for a in args {
                        self.emit_expr(ctx, a)?;
                    }
                    ctx.push(Insn::CallBuiltin { b, argc: argc + 1 });
                    return Ok(());
                }
                // direct call of a named function or builtin
                if let ExprKind::Ident(name) = &callee.kind {
                    match self.resolve(ctx, name, callee.span)? {
                        Place::Func(fn_idx) => {
                            for a in args {
                                self.emit_expr(ctx, a)?;
                            }
                            ctx.push(Insn::CallFn { fn_idx, argc });
                            return Ok(());
                        }
                        Place::Builtin(b) => {
                            for a in args {
                                self.emit_expr(ctx, a)?;
                            }
                            ctx.push(Insn::CallBuiltin { b, argc });
                            return Ok(());
                        }
                        _ => {} // fall through to value call
                    }
                }
                self.emit_expr(ctx, callee)?;
                for a in args {
                    self.emit_expr(ctx, a)?;
                }
                ctx.push(Insn::CallValue { argc });
                Ok(())
            }
            ExprKind::Index { obj, index } => {
                self.emit_expr(ctx, obj)?;
                self.emit_expr(ctx, index)?;
                ctx.push(Insn::LoadIdx);
                Ok(())
            }
            ExprKind::Member { obj, name } => {
                if name == "length" {
                    self.emit_expr(ctx, obj)?;
                    ctx.push(Insn::ArrLen);
                    Ok(())
                } else {
                    Err(Diagnostic::new(
                        e.span,
                        format!("unknown property `.{name}`"),
                    ))
                }
            }
            ExprKind::Lambda { params, body } => {
                let idx = self.emit_lambda(params, body, e.span)?;
                ctx.push(Insn::Const(Value::Fun(idx as u32)));
                Ok(())
            }
        }
    }
}

/// The literal's elements as compile-time constants, if EVERY element is a
/// numeric literal (optionally under unary +/-). Nested arrays, idents,
/// and expressions disqualify it — those need runtime construction.
fn const_elements(elems: &[Expr]) -> Option<Vec<i32>> {
    let mut out = Vec::with_capacity(elems.len());
    for e in elems {
        let raw = match &e.kind {
            ExprKind::Num(v) => v.raw(),
            ExprKind::Unary { op, expr } => match (&op, &expr.kind) {
                (UnOp::Neg, ExprKind::Num(v)) => (-*v).raw(),
                (UnOp::Pos, ExprKind::Num(v)) => v.raw(),
                _ => return None,
            },
            _ => return None,
        };
        out.push(raw);
    }
    Some(out)
}

impl<'s> Compiler<'s> {
    /// Intern an `assert()` message into the program's message pool
    /// (deduplicated), truncated to the str8 wire limit on a char boundary.
    fn intern_msg(&mut self, mut text: String, span: Span) -> Result<u16, Diagnostic> {
        if text.len() > 255 {
            let mut cut = 252;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
            text.push_str("...");
        }
        if let Some(i) = self.assert_msgs.iter().position(|m| *m == text) {
            return Ok(i as u16);
        }
        if self.assert_msgs.len() >= MAX_ASSERT_MSGS {
            return Err(Diagnostic::new(
                span,
                format!("too many distinct assert messages (max {MAX_ASSERT_MSGS})"),
            ));
        }
        self.assert_msgs.push(text);
        Ok((self.assert_msgs.len() - 1) as u16)
    }

    /// Intern a const array's contents; None if the pool is at capacity
    /// (the caller falls back to runtime construction).
    fn intern_data(&mut self, raws: Vec<i32>) -> Option<u16> {
        if let Some(&d) = self.data_map.get(&raws) {
            return Some(d);
        }
        let total: usize = self.data_arrays.iter().map(|a| a.len()).sum();
        if self.data_arrays.len() >= MAX_DATA_ARRAYS || total + raws.len() > MAX_DATA_ELEMS {
            return None;
        }
        let d = self.data_arrays.len() as u16;
        self.data_arrays.push(raws.clone());
        self.data_map.insert(raws, d);
        Some(d)
    }
}

fn bin_insn(op: BinOp) -> Insn {
    match op {
        BinOp::Add => Insn::Add,
        BinOp::Sub => Insn::Sub,
        BinOp::Mul => Insn::Mul,
        BinOp::Div => Insn::Div,
        BinOp::Rem => Insn::Rem,
        BinOp::Pow => Insn::Pow,
        BinOp::Shl => Insn::Shl,
        BinOp::Shr => Insn::Shr,
        BinOp::BitAnd => Insn::BitAnd,
        BinOp::BitOr => Insn::BitOr,
        BinOp::BitXor => Insn::BitXor,
        BinOp::Lt => Insn::Lt,
        BinOp::Le => Insn::Le,
        BinOp::Gt => Insn::Gt,
        BinOp::Ge => Insn::Ge,
        BinOp::Eq => Insn::Eq,
        BinOp::Ne => Insn::Ne,
        BinOp::And | BinOp::Or => unreachable!("short-circuit ops emitted separately"),
    }
}

struct FnCtx {
    locals: Vec<String>,
    code: Vec<Insn>,
    /// (line, col) per emitted instruction — statement granularity.
    pos: Vec<(u32, u32)>,
    cur_pos: (u32, u32),
    loops: Vec<BreakFrame>,
    /// Local slots declared `const` in this function.
    const_locals: BTreeSet<u8>,
    /// This is fn 0 (top-level init) — where `assert()` is legal.
    is_top: bool,
}

/// A `break`/`continue` target. Loops answer both; a `switch` answers only
/// `break`, so `continue` inside one still belongs to the enclosing loop.
struct BreakFrame {
    breaks: Vec<usize>,
    continues: Vec<usize>,
    is_loop: bool,
}

impl BreakFrame {
    fn loop_frame() -> BreakFrame {
        BreakFrame {
            breaks: Vec::new(),
            continues: Vec::new(),
            is_loop: true,
        }
    }

    fn switch_frame() -> BreakFrame {
        BreakFrame {
            breaks: Vec::new(),
            continues: Vec::new(),
            is_loop: false,
        }
    }
}

impl FnCtx {
    fn new(locals: Vec<String>, is_top: bool) -> FnCtx {
        FnCtx {
            locals,
            code: Vec::new(),
            pos: Vec::new(),
            cur_pos: (0, 0),
            loops: Vec::new(),
            const_locals: BTreeSet::new(),
            is_top,
        }
    }

    fn set_pos(&mut self, pos: (u32, u32)) {
        self.cur_pos = pos;
    }

    fn push(&mut self, insn: Insn) {
        self.code.push(insn);
        self.pos.push(self.cur_pos);
    }

    fn here(&self) -> u32 {
        self.code.len() as u32
    }

    fn emit_placeholder(&mut self) -> usize {
        self.push(Insn::Jmp(u32::MAX));
        self.code.len() - 1
    }

    fn patch(&mut self, at: usize, insn: Insn) {
        self.code[at] = insn;
    }

    fn finish(self, name: String, params: u8) -> FnIr {
        FnIr {
            name,
            params,
            code: self.code,
            pos: self.pos,
            local_names: self.locals,
        }
    }
}

/// Walk every statement (including inside function bodies — PB flattens all
/// declarations to global scope) and visit each `function` declaration.
fn walk_fns<'a>(stmts: &'a [Stmt], f: &mut impl FnMut(&'a Stmt)) {
    for s in stmts {
        match &s.kind {
            StmtKind::Func { body, .. } => {
                f(s);
                walk_fns(body, f);
            }
            StmtKind::If { then, els, .. } => {
                walk_fns(core::slice::from_ref(then), f);
                if let Some(e) = els {
                    walk_fns(core::slice::from_ref(e), f);
                }
            }
            StmtKind::While { body, .. } => walk_fns(core::slice::from_ref(body), f),
            StmtKind::For { init, body, .. } => {
                if let Some(i) = init {
                    walk_fns(core::slice::from_ref(i), f);
                }
                walk_fns(core::slice::from_ref(body), f);
            }
            StmtKind::Switch { cases, .. } => {
                for c in cases {
                    walk_fns(&c.body, f);
                }
            }
            StmtKind::Block(b) => walk_fns(b, f),
            _ => {}
        }
    }
}

fn register_fns(c: &mut Compiler, top: &[Stmt]) {
    let mut found: Vec<(String, bool)> = Vec::new();
    walk_fns(top, &mut |s| {
        if let StmtKind::Func { export, name, .. } = &s.kind {
            if let Some(e) = found.iter_mut().find(|(n, _)| n == name) {
                e.1 |= *export;
            } else {
                found.push((name.clone(), *export));
            }
        }
    });
    for (name, export) in found {
        let idx = (c.fns.len() + c.named_fns.len()) as u16;
        c.named_fns.push((name, idx, export));
    }
}

/// A function's local slots: params first, then every hoisted `var` in the
/// body (JS var hoisting — blocks and loops don't scope, lambdas do).
fn function_scope(params: &[String], body: &[Stmt]) -> Vec<String> {
    let mut locals: Vec<String> = params.to_vec();
    hoist(body, &mut locals);
    locals
}

fn hoist(stmts: &[Stmt], out: &mut Vec<String>) {
    for s in stmts {
        match &s.kind {
            StmtKind::Var { decls, .. } => {
                for d in decls {
                    if !out.iter().any(|n| n == &d.name) {
                        out.push(d.name.clone());
                    }
                }
            }
            // note: nested function declarations do NOT hoist as locals —
            // PB flattens them to global scope (register_fns handles them)
            StmtKind::If { then, els, .. } => {
                hoist(core::slice::from_ref(then), out);
                if let Some(e) = els {
                    hoist(core::slice::from_ref(e), out);
                }
            }
            StmtKind::While { body, .. } => hoist(core::slice::from_ref(body), out),
            StmtKind::For { init, body, .. } => {
                if let Some(i) = init {
                    hoist(core::slice::from_ref(i), out);
                }
                hoist(core::slice::from_ref(body), out);
            }
            StmtKind::Switch { cases, .. } => {
                for c in cases {
                    hoist(&c.body, out);
                }
            }
            StmtKind::Block(b) => hoist(b, out),
            _ => {}
        }
    }
}

#[cfg(test)]
mod const_fold_tests {
    //! Per-rule codegen pins for the Gitea #312 passes: one test per
    //! rewrite, each stating the source it fires on and the exact
    //! instruction sequence the function must lower to. A rule that
    //! silently stops matching fails here rather than quietly costing
    //! instructions on the device.
    use super::*;
    use alloc::string::String;
    use alloc::vec::Vec;

    /// The IR of one function after both #312 passes and the #261
    /// peephole — i.e. exactly the instruction stream [`assemble`]
    /// encodes.
    fn ir(src: &str, fname: &str) -> Vec<Insn> {
        let ast = parse_program(src).expect("parses");
        let mut c = Compiler::new(src);
        c.collect(&ast).expect("collects");
        c.emit_program(&ast).expect("compiles");
        let frozen = frozen_globals(&c.fns, &c.globals);
        let f = c
            .fns
            .iter()
            .find(|f| f.name == fname)
            .unwrap_or_else(|| panic!("no function `{fname}`"));
        let (code, pos) = const_fold(f.code.clone(), f.pos.clone(), &frozen);
        peephole(code, pos).0
    }

    /// Variant names only — the operand values are asserted separately
    /// where they carry the point of the test.
    fn ops(src: &str, fname: &str) -> Vec<String> {
        ir(src, fname)
            .iter()
            .map(|i| {
                let s = format!("{i:?}");
                s.split(['(', ' ']).next().unwrap_or("?").into()
            })
            .collect()
    }

    fn same_ir(src: &str, fname: &str) -> Vec<String> {
        ir(src, fname).iter().map(|i| format!("{i:?}")).collect()
    }

    // ---- rule: postfix inc/dec whose value is discarded ----

    #[test]
    fn a_for_headers_increment_does_not_recover_the_old_value() {
        // `i++` as a for-update: the four instructions of the update are
        // LoadL/Const/Add/StoreL, and the discarded old value used to cost
        // a `Const 1; Sub` on top — which also blocked `StoreL; Pop` from
        // fusing. Nine instructions per iteration, not eleven.
        let src = "export function render(index) {
  var x = 0
  for (var i = 0; i < 16; i++) {
    x += i * 0.5
  }
  hsv(x, 1, 1)
}";
        assert_eq!(
            ops(src, "render"),
            [
                "Const",           // x = 0
                "StoreLPop",
                "Const",           // i = 0
                "StoreLPop",
                "LoadLConstOp",    // i < 16
                "JmpIfFalse",
                "LoadLL",          // x += i * 0.5
                "ConstOp",
                "Add",
                "StoreLPop",
                "LoadLConstOp",    // i++  (no old-value recovery)
                "StoreLPop",
                "Jmp",
                "LoadL",           // hsv(x, 1, 1)
                "CallBuiltinCC",
                "PopRetNull",
            ]
        );
    }

    #[test]
    fn a_bare_increment_statement_loses_the_recovery_too() {
        let src = "export function render(index) {
  var i = index
  i++
  hsv(i, 1, 1)
}";
        assert_eq!(
            ops(src, "render"),
            [
                "LoadL",
                "StoreLPop",
                "LoadLConstOp",
                "StoreLPop",
                "LoadL",
                "CallBuiltinCC",
                "PopRetNull"
            ]
        );
    }

    #[test]
    fn a_postfix_increment_whose_value_is_used_still_recovers_it() {
        // The rewrite is only legal where the value is popped. Used as a
        // value, `i++` must still yield the OLD one.
        let src = "export function render(index) {
  var i = 0
  var a = i++
  hsv(a, 1, 1)
}";
        let o = ops(src, "render");
        assert!(
            o.windows(2).any(|w| w[0] == "StoreL" && w[1] == "ConstOp"),
            "the old-value recovery is gone from a value context: {o:?}"
        );
    }

    // ---- rule: literal constant folding ----

    #[test]
    fn a_negated_literal_is_one_instruction() {
        // The parser hands the compiler `Neg(Num(1))`, never `Num(-1)`.
        let src = "export function render(index) { hsv(index + -1, 1, 1) }";
        let o = ops(src, "render");
        assert!(!o.contains(&String::from("Neg")), "{o:?}");
        assert_eq!(o, ["LoadLConstOp", "CallBuiltinCC", "PopRetNull"]);
        // …and it is the right constant
        assert!(
            same_ir(src, "render")[0].contains("-1"),
            "{:?}",
            same_ir(src, "render")
        );
    }

    #[test]
    fn literal_arithmetic_collapses_to_one_constant() {
        let src = "export function render(index) { hsv(index * (1 / 3), 1, 1) }";
        assert_eq!(
            ops(src, "render"),
            ["LoadLConstOp", "CallBuiltinCC", "PopRetNull"]
        );
        // 1/3 in 16.16 is 21845 raw — the same word `Fx::div` computes
        assert_eq!(
            ir(src, "render")[0],
            Insn::LoadLConstOp(0, op::MUL, Fx::from_raw(21845))
        );
    }

    #[test]
    fn a_folded_chain_matches_the_interpreters_own_arithmetic() {
        // Every foldable operator, over the edge words of the format, must
        // give exactly what `vm::binop` gives — that is the whole
        // bit-exactness claim, so it is checked directly rather than
        // inferred from a render.
        let edges = [
            Fx::from_raw(i32::MIN),
            Fx::from_raw(i32::MIN + 1),
            Fx::from_raw(-65536),
            Fx::from_raw(-32768),
            Fx::from_raw(-1),
            Fx::ZERO,
            Fx::from_raw(1),
            Fx::from_raw(32768),
            Fx::ONE,
            Fx::from_raw(i32::MAX),
        ];
        let subs = [
            op::ADD, op::SUB, op::MUL, op::DIV, op::REM, op::POW,
            op::BIT_AND, op::BIT_OR, op::BIT_XOR, op::SHL, op::SHR,
            op::LT, op::LE, op::GT, op::GE, op::EQ, op::NE,
        ];
        for &sub in &subs {
            for &a in &edges {
                for &b in &edges {
                    let code = alloc::vec![
                        Insn::Const(Value::Num(a)),
                        Insn::Const(Value::Num(b)),
                        insn_for_sub(sub),
                    ];
                    let pos = alloc::vec![(1u32, 1u32); 3];
                    let (out, _) = const_fold(code, pos, &[]);
                    assert_eq!(out.len(), 1, "{sub} {a:?} {b:?} did not fold");
                    let want = crate::vm::binop(sub, Value::Num(a), Value::Num(b));
                    assert_eq!(
                        out[0],
                        Insn::Const(want),
                        "fold of {sub} on {a:?}, {b:?} drifted from vm::binop"
                    );
                }
            }
        }
        // and the unary set
        for &a in &edges {
            for (insn, want) in [
                (Insn::Neg, Value::Num(-a)),
                (Insn::BitNot, Value::Num(!a)),
                (
                    Insn::Not,
                    Value::Num(if Value::Num(a).truthy() { Fx::ZERO } else { Fx::ONE }),
                ),
            ] {
                let code = alloc::vec![Insn::Const(Value::Num(a)), insn];
                let (out, _) = const_fold(code, alloc::vec![(1, 1); 2], &[]);
                assert_eq!(out, alloc::vec![Insn::Const(want)], "unary {insn:?} on {a:?}");
            }
        }
    }

    fn insn_for_sub(sub: u8) -> Insn {
        match sub {
            op::ADD => Insn::Add,
            op::SUB => Insn::Sub,
            op::MUL => Insn::Mul,
            op::DIV => Insn::Div,
            op::REM => Insn::Rem,
            op::POW => Insn::Pow,
            op::BIT_AND => Insn::BitAnd,
            op::BIT_OR => Insn::BitOr,
            op::BIT_XOR => Insn::BitXor,
            op::SHL => Insn::Shl,
            op::SHR => Insn::Shr,
            op::LT => Insn::Lt,
            op::LE => Insn::Le,
            op::GT => Insn::Gt,
            op::GE => Insn::Ge,
            op::EQ => Insn::Eq,
            _ => Insn::Ne,
        }
    }

    // ---- rule: frozen-global substitution ----

    #[test]
    fn pi2_times_a_local_becomes_one_instruction() {
        let src = "export function render(index) { hsv(index * PI2, 1, 1) }";
        assert_eq!(
            ops(src, "render"),
            ["LoadLConstOp", "CallBuiltinCC", "PopRetNull"]
        );
    }

    #[test]
    fn a_frozen_global_is_not_substituted_where_it_would_cost_a_word() {
        // `PI * arr[0]` — the load's successor is neither an operator the
        // constant can fuse with nor a reorderable push, so `LoadG` stays.
        let src = "var arr = array(4)
export function render(index) { hsv(PI * arr[index], 1, 1) }";
        let o = ops(src, "render");
        assert!(o.contains(&String::from("LoadG")), "{o:?}");
    }

    #[test]
    fn assigning_to_a_predefined_name_unfreezes_it() {
        // `PI = 3` is legal; the pattern below must then read the global,
        // not the compile-time constant.
        let src = "PI = 3
export function render(index) { hsv(index * PI, 1, 1) }";
        let o = ops(src, "render");
        assert!(
            o.contains(&String::from("LoadLG")),
            "a written global was folded into a constant: {o:?}"
        );
    }

    #[test]
    fn an_exported_predefined_name_is_not_frozen() {
        // Only exported globals are settable from the host, so exporting
        // one has to take it out of the frozen set.
        let src = "export var PI = 3.5
export function render(index) { hsv(index * PI, 1, 1) }";
        let o = ops(src, "render");
        assert!(o.contains(&String::from("LoadLG")), "{o:?}");
    }

    #[test]
    fn pixel_count_is_never_frozen() {
        // The engine writes it straight into `vm.globals`; no `StoreG`
        // ever appears, so the `StoreG` scan cannot catch it.
        let src = "export function render(index) { hsv(index / pixelCount, 1, 1) }";
        let o = ops(src, "render");
        assert!(o.contains(&String::from("LoadLG")), "{o:?}");
    }

    // ---- rule: operand commute ----

    #[test]
    fn a_constant_on_the_left_of_a_commutative_op_moves_right() {
        let src = "export function render(index) { hsv(2 * index, 1, 1) }";
        assert_eq!(
            ops(src, "render"),
            ["LoadLConstOp", "CallBuiltinCC", "PopRetNull"]
        );
        assert_eq!(
            ir(src, "render")[0],
            Insn::LoadLConstOp(0, op::MUL, Fx::from_int(2))
        );
    }

    #[test]
    fn a_constant_on_the_left_of_a_comparison_mirrors_the_operator() {
        // `2 < index` is `index > 2`, the same predicate on the 16.16
        // total order — and one instruction instead of three.
        let src = "export function render(index) { hsv(2 < index, 1, 1) }";
        assert_eq!(
            ir(src, "render")[0],
            Insn::LoadLConstOp(0, op::GT, Fx::from_int(2))
        );
    }

    #[test]
    fn a_non_commutative_operator_is_left_alone() {
        // `2 / index` is not `index / 2`.
        let src = "export function render(index) { hsv(2 / index, 1, 1) }";
        let o = ops(src, "render");
        assert_eq!(o[0], "Const");
        assert_eq!(o[1], "LoadL");
        assert_eq!(o[2], "Div");
    }

    #[test]
    fn a_jump_target_between_the_operands_blocks_the_swap() {
        // The `&&` short-circuit lands its jump on the second operand, so
        // reordering it would change what the jumped-to path sees.
        let src = "export function render(index) {
  var v = 2 * (index > 1 && index)
  hsv(v, 1, 1)
}";
        // still renders; the point is that it compiles without the swap
        // corrupting the jump target — the equivalence tests in
        // tests/constfold.rs check the pixels.
        let o = ops(src, "render");
        assert!(o.iter().any(|s| s == "JmpIfTruePeek" || s == "JmpIfFalsePeek"), "{o:?}");
    }
}
