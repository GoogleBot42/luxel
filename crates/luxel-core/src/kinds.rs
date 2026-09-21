//! Kinds: the static representation proof LXBC v6 carries (Gitea #607,
//! docs/jit-design.md §2).
//!
//! A *kind* is a proof about a slot — a global, a local, a parameter, a
//! function's return — that every value which can ever reach it has one
//! representation. The interpreter does not care: it boxes every [`Value`]
//! either way and [`crate::bytecode::op::BOX`] is a no-op arm. The JIT does:
//! a `Num` slot is a raw 16.16 word in a register, a `Dyn` slot is the
//! boxed pair. Phase 0 (this module) only *computes* and *checks* the
//! proof.
//!
//! Three entry points:
//!
//! - [`infer`] — a flow-insensitive whole-program fixpoint over the
//!   COMPILED word stream (not the AST: the folds and superinstructions
//!   must see the same kinds the device verifies, and one implementation
//!   then serves the compiler, the CLI and the decoder).
//! - [`verify`] — the linear stack-map walk of §2.4. It is the decoder's
//!   check on an untrusted blob AND the compiler's own self-check; a
//!   failure is a compiler bug and is meant to be loud.
//! - [`explain`] — why each `Dyn` slot is `Dyn`, for the playground lint.
//!
//! **Soundness rule**: a slot may be `Num` only if EVERY value that can
//! reach it is a number. That includes values the *engine* writes:
//! `Engine::set_var` pokes a `Value::Num` into any exported global, and
//! `Engine::from_program*` seeds `frequencyData`/`accelerometer`/
//! `analogInputs` with arrays. Both are modelled below.

use alloc::vec::Vec;

#[cfg(feature = "kinds")]
use alloc::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "kinds")]
use alloc::format;
#[cfg(feature = "kinds")]
use alloc::string::{String, ToString};

use crate::vm::Program;

// ---------------------------------------------------------------- lattice

/// The kind lattice (docs/jit-design.md §2.1):
///
/// ```text
///             Dyn                 (boxed 8-byte Value; anything)
///       ┌──────┼──────┬─────┐
///      Num    Arr    Fun  Builtin (unboxed 4-byte word)
///              │
///            ArrNum                (array whose elements are all Num)
/// ```
///
/// The discriminants ARE the wire encoding of a kind byte; other byte
/// values are a format error.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Kind {
    #[default]
    Dyn = 0,
    Num = 1,
    Arr = 2,
    ArrNum = 3,
    Fun = 4,
    Builtin = 5,
}

impl Kind {
    pub fn from_byte(b: u8) -> Option<Kind> {
        Some(match b {
            0 => Kind::Dyn,
            1 => Kind::Num,
            2 => Kind::Arr,
            3 => Kind::ArrNum,
            4 => Kind::Fun,
            5 => Kind::Builtin,
            _ => return None,
        })
    }

    #[inline]
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    /// `self ⊑ other` — "a value of kind `self` may be stored where `other`
    /// is expected". Everything is below `Dyn`; `ArrNum` is below `Arr`.
    #[inline]
    pub fn le(self, other: Kind) -> bool {
        self == other || other == Kind::Dyn || (self == Kind::ArrNum && other == Kind::Arr)
    }

    /// Least upper bound.
    #[inline]
    pub fn join(self, other: Kind) -> Kind {
        if self == other {
            self
        } else if self.le(other) {
            other
        } else if other.le(self) {
            self
        } else {
            Kind::Dyn
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Dyn => "Dyn",
            Kind::Num => "Num",
            Kind::Arr => "Arr",
            Kind::ArrNum => "ArrNum",
            Kind::Fun => "Fun",
            Kind::Builtin => "Builtin",
        }
    }
}

/// One function's annotation: the return kind, then one kind per local
/// SLOT (`FnDef::locals`, which counts params first — so slots `0..params`
/// are the parameters).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FnKinds {
    pub ret: Kind,
    pub slots: Vec<Kind>,
}

/// A whole program's annotation — the `kinds` section of an LXBC v6 blob.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kinds {
    pub globals: Vec<Kind>,
    pub fns: Vec<FnKinds>,
}

impl Kinds {
    /// The trivial (always sound, never useful) annotation: everything
    /// `Dyn`. What a v6 blob without the `TYPED` flag means.
    pub fn all_dyn(prog: &Program) -> Kinds {
        Kinds {
            globals: alloc::vec![Kind::Dyn; prog.globals.len()],
            fns: prog
                .fns
                .iter()
                .map(|f| FnKinds {
                    ret: Kind::Dyn,
                    slots: alloc::vec![Kind::Dyn; f.locals as usize],
                })
                .collect(),
        }
    }

    #[inline]
    pub fn global(&self, g: usize) -> Kind {
        self.globals.get(g).copied().unwrap_or(Kind::Dyn)
    }
    #[inline]
    pub fn slot(&self, f: usize, i: usize) -> Kind {
        self.fns
            .get(f)
            .and_then(|k| k.slots.get(i))
            .copied()
            .unwrap_or(Kind::Dyn)
    }
    #[inline]
    pub fn ret(&self, f: usize) -> Kind {
        self.fns.get(f).map(|k| k.ret).unwrap_or(Kind::Dyn)
    }

    /// Encoded length of the `kinds` section in bytes, INCLUDING the zero
    /// padding to a multiple of 4. Derivable from the header counts alone,
    /// which is what lets a decoder without the `kinds` feature skip the
    /// section without parsing it.
    pub fn section_len(n_globals: usize, locals_per_fn: impl Iterator<Item = usize>) -> usize {
        let mut n = n_globals;
        for l in locals_per_fn {
            n += 1 + l;
        }
        n.next_multiple_of(4)
    }
}

// ------------------------------------------------------- verifier plumbing

/// The shape [`verify_words`] needs of one function — the subset of
/// [`crate::vm::FnDef`] the stack-map walk reads. The decoder can fill it
/// in without materializing a `Program`.
#[derive(Clone, Copy, Debug)]
pub struct FnView {
    pub params: u8,
    pub locals: u8,
    pub code_start: u32,
    pub code_len: u32,
}

/// A kind-verification failure: which function, which fn-relative WORD
/// index, and what did not match. A `KindError` out of the compiler is a
/// compiler bug; out of the decoder it is a bad blob (`bc-kinds`).
#[cfg(feature = "kinds")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindError {
    pub fn_idx: u16,
    pub word: u32,
    pub what: String,
}

#[cfg(feature = "kinds")]
impl core::fmt::Display for KindError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "fn {} word {}: {}", self.fn_idx, self.word, self.what)
    }
}

/// One `Box` the compiler must insert to make a conditional join agree
/// (§2.3 "Joins"). `at` is the fn-relative WORD index of the instruction
/// the `Box` goes BEFORE — the branch instruction for a taken edge, the
/// join target itself for the fall-through edge.
#[cfg(feature = "kinds")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxFix {
    pub fn_idx: u16,
    pub at: u32,
    /// Stack depth (0 = bottom) of the slot whose kinds disagree. Always
    /// the top of the stack — a deeper disagreement is a hard error,
    /// because one `Box` cannot reach past the top.
    pub depth: u32,
}

// ------------------------------------------------------------- opcode view

#[cfg(feature = "kinds")]
use crate::bytecode::{enc, is_binop_sub, op};

/// Words one instruction occupies. Mirrors `bytecode::walk_word`'s `len`
/// without its validation — every caller here runs on words a decoder has
/// already accepted, or on the compiler's own freshly laid-out stream.
#[cfg(feature = "kinds")]
pub fn ilen(o: u8) -> usize {
    match o {
        op::CONST_NUM
        | op::CONST_OP
        | op::LOAD_L_CONST_OP
        | op::LOAD_G_CONST_OP
        | op::CALL_BUILTIN_C
        | op::CMP_JF => 2,
        op::CALL_BUILTIN_CC => 3,
        _ => 1,
    }
}

/// Is this opcode a two-operand value op (pops two, pushes `Num`)?
#[cfg(feature = "kinds")]
fn is_binop(o: u8) -> bool {
    is_binop_sub(o)
}

// ------------------------------------------------------ builtin signatures

#[cfg(feature = "kinds")]
pub use crate::vm::{builtin_sig, BuiltinSig, SigRet, SigWrite};

// ================================================================ VERIFIER

/// The linear stack-map walk of docs/jit-design.md §2.4, over a program
/// already in `Program` form.
#[cfg(feature = "kinds")]
pub fn verify(prog: &Program, kinds: &Kinds) -> Result<(), KindError> {
    let views: Vec<FnView> = prog
        .fns
        .iter()
        .map(|f| FnView {
            params: f.params,
            locals: f.locals,
            code_start: f.code_start,
            code_len: f.code_len,
        })
        .collect();
    let words: &[u32] = &prog.words;
    verify_words(&views, prog.globals.len(), kinds, &|i| {
        words.get(i).copied().unwrap_or(0)
    })
}

/// Every `Box` the compiler still owes this program, or the first HARD
/// verification failure (one a `Box` cannot fix). Host/compiler side only.
#[cfg(feature = "kinds")]
pub fn plan_boxes(prog: &Program, kinds: &Kinds) -> Result<Vec<BoxFix>, KindError> {
    let views: Vec<FnView> = prog
        .fns
        .iter()
        .map(|f| FnView {
            params: f.params,
            locals: f.locals,
            code_start: f.code_start,
            code_len: f.code_len,
        })
        .collect();
    let words: &[u32] = &prog.words;
    let mut fixes = Vec::new();
    walk_all(
        &views,
        prog.globals.len(),
        kinds,
        &|i| words.get(i).copied().unwrap_or(0),
        Some(&mut fixes),
    )?;
    Ok(fixes)
}

/// The abstract operand stack on ARRIVAL at each fn-relative word index,
/// or `None` at a word nothing reaches (a dead epilogue after an explicit
/// `return` — 0.2 % of the library's instructions).
///
/// Indexed by word, so continuation words of a two- or three-word
/// instruction are `None` too.
#[cfg(feature = "kinds")]
pub type StackMap = Vec<Option<Vec<Kind>>>;

/// One [`StackMap`] per function, from the SAME walk [`verify`] runs
/// (Gitea #651). The JIT's emitter needs the kind and depth of every
/// operand at every instruction, and this is how it gets them without
/// forking the semantics: a second implementation of §2.4's rules that
/// disagreed by one push would be a silent miscompile.
///
/// Verifies as it goes, so a `Ok` result is also a successful [`verify`].
#[cfg(feature = "kinds")]
pub fn stack_maps(prog: &Program, kinds: &Kinds) -> Result<Vec<StackMap>, KindError> {
    let views: Vec<FnView> = prog
        .fns
        .iter()
        .map(|f| FnView {
            params: f.params,
            locals: f.locals,
            code_start: f.code_start,
            code_len: f.code_len,
        })
        .collect();
    let words: &[u32] = &prog.words;
    let word = |i: usize| words.get(i).copied().unwrap_or(0);
    if kinds.fns.len() != views.len() || kinds.globals.len() != prog.globals.len() {
        return Err(KindError {
            fn_idx: 0,
            word: 0,
            what: "kinds section does not match the program's shape".to_string(),
        });
    }
    let mut out = Vec::with_capacity(views.len());
    for (fi, f) in views.iter().enumerate() {
        if kinds.fns[fi].slots.len() != f.locals as usize {
            return Err(KindError {
                fn_idx: fi as u16,
                word: 0,
                what: format!(
                    "kinds section has {} slots, the function has {}",
                    kinds.fns[fi].slots.len(),
                    f.locals
                ),
            });
        }
        let mut m: StackMap = alloc::vec![None; f.code_len as usize];
        walk_fn(
            fi,
            &views,
            prog.globals.len(),
            kinds,
            &word,
            None,
            Some(&mut m),
        )?;
        out.push(m);
    }
    Ok(out)
}

/// [`verify`] over a decoder's view of a blob: the function table it has
/// already validated plus a word accessor, with no `Program` in hand.
#[cfg(feature = "kinds")]
pub fn verify_words(
    fns: &[FnView],
    n_globals: usize,
    kinds: &Kinds,
    word: &dyn Fn(usize) -> u32,
) -> Result<(), KindError> {
    walk_all(fns, n_globals, kinds, word, None)
}

/// Which edge recorded an abstract stack at a join target.
#[cfg(feature = "kinds")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Origin {
    /// Fell through into the target from the preceding instruction.
    FallThrough,
    /// A branch at this fn-relative word index jumped here.
    Jump(u32),
}

#[cfg(feature = "kinds")]
fn walk_all(
    fns: &[FnView],
    n_globals: usize,
    kinds: &Kinds,
    word: &dyn Fn(usize) -> u32,
    mut fixes: Option<&mut Vec<BoxFix>>,
) -> Result<(), KindError> {
    if kinds.fns.len() != fns.len() || kinds.globals.len() != n_globals {
        return Err(KindError {
            fn_idx: 0,
            word: 0,
            what: "kinds section does not match the program's shape".to_string(),
        });
    }
    for (fi, f) in fns.iter().enumerate() {
        if kinds.fns[fi].slots.len() != f.locals as usize {
            return Err(KindError {
                fn_idx: fi as u16,
                word: 0,
                what: format!(
                    "kinds section has {} slots, the function has {}",
                    kinds.fns[fi].slots.len(),
                    f.locals
                ),
            });
        }
        walk_fn(fi, fns, n_globals, kinds, word, fixes.as_deref_mut(), None)?;
    }
    Ok(())
}
/// One entry of the abstract operand stack: a kind, plus whether the
/// value is reachable through an ANNOTATED slot.
///
/// The flag is what makes the `ArrNum` rules checkable. `ArrNum` is a
/// property of an array OBJECT, and a linear walk cannot follow an object
/// through the heap — but it does not have to. A value fresh out of
/// `array(n)` / `NewArray` / `ConstArr` genuinely holds only numbers at
/// that instant, and nothing has promised anything about its future; the
/// moment it is stored into a slot, that slot's annotation becomes a
/// promise about it, so `StoreL`/`StoreG` re-tag the value with the SLOT's
/// kind and mark it annotated. A non-`Num` write through an annotated
/// `ArrNum` value then contradicts the annotation and is a hard error,
/// while `arr = array(8); arr.mutate(f)` — where store forwarding hands
/// the fresh array straight to the builtin and the inference has already
/// demoted `arr` to `Arr` — passes, correctly.
#[cfg(feature = "kinds")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ab {
    k: Kind,
    ann: bool,
}

#[cfg(feature = "kinds")]
#[inline]
fn fresh(k: Kind) -> Ab {
    Ab { k, ann: false }
}
#[cfg(feature = "kinds")]
#[inline]
fn slotted(k: Kind) -> Ab {
    Ab { k, ann: true }
}

#[cfg(feature = "kinds")]
#[allow(clippy::too_many_lines)]
fn walk_fn(
    fi: usize,
    fns: &[FnView],
    n_globals: usize,
    kinds: &Kinds,
    word: &dyn Fn(usize) -> u32,
    mut fixes: Option<&mut Vec<BoxFix>>,
    mut map: Option<&mut StackMap>,
) -> Result<(), KindError> {
    let f = fns[fi];
    let base = f.code_start as usize;
    let n = f.code_len as usize;
    let fk = &kinds.fns[fi];
    let err = |at: usize, what: String| KindError {
        fn_idx: fi as u16,
        word: at as u32,
        what,
    };

    // States recorded by an edge at a (not yet walked, or already walked)
    // word. A first visit records; a later visit compares (§2.4).
    let mut states: BTreeMap<u32, (Vec<Ab>, Origin)> = BTreeMap::new();
    let mut cur: Option<Vec<Ab>> = Some(Vec::new());
    let mut at = 0usize;

    while at < n {
        let w = word(base + at);
        let o = enc::opcode(w);
        let len = ilen(o);

        // arrival: adopt or merge the recorded stack
        if let Some((rec, orig)) = states.get(&(at as u32)) {
            let rec = rec.clone();
            let orig = *orig;
            match cur.take() {
                None => cur = Some(rec),
                Some(c) => {
                    match merge_edges(
                        fi,
                        at,
                        &rec,
                        orig,
                        &c,
                        Origin::FallThrough,
                        at as u32,
                        fixes.as_deref_mut(),
                    )? {
                        Some(m) => cur = Some(m),
                        // a Box fix was recorded: this function will be
                        // re-walked after the compiler inserts it
                        None => return Ok(()),
                    }
                }
            }
        }
        let Some(st) = cur.as_mut() else {
            // unreachable code (a dead epilogue after an explicit return —
            // 0.2 % of the library's instructions). Nothing executes it, so
            // there is nothing to prove.
            at += len;
            continue;
        };
        // The JIT's emitter needs the abstract stack at every word, and it
        // has to be THIS walk's answer rather than a second implementation
        // of the same rules (Gitea #651). Recording it here is the only
        // hook that cannot drift: `st` is the state on ARRIVAL, before the
        // instruction's own effect, which is exactly what the emitter
        // reads to know each operand's kind and home.
        if let Some(m) = map.as_deref_mut() {
            m[at] = Some(st.iter().map(|a| a.k).collect());
        }

        macro_rules! pop {
            () => {
                match st.pop() {
                    Some(k) => k,
                    None => return Err(err(at, "operand stack underflow".to_string())),
                }
            };
        }
        macro_rules! need {
            ($v:expr, $want:expr, $what:expr) => {{
                let v: Kind = $v;
                let want: Kind = $want;
                if !v.le(want) {
                    return Err(err(
                        at,
                        format!("{} is {}, needs {}", $what, v.name(), want.name()),
                    ));
                }
            }};
        }
        let gk = |g: usize, at: usize| -> Result<Kind, KindError> {
            if g >= n_globals {
                return Err(err(at, "global index out of range".to_string()));
            }
            Ok(kinds.global(g))
        };
        let lk = |i: usize, at: usize| -> Result<Kind, KindError> {
            if i >= fk.slots.len() {
                return Err(err(at, "local slot out of range".to_string()));
            }
            Ok(fk.slots[i])
        };

        let mut edges: Vec<(u32, Vec<Ab>)> = Vec::new();
        let mut terminal = false;

        match o {
            op::CONST_NUM => st.push(fresh(Kind::Num)),
            op::CONST_FUN => st.push(fresh(Kind::Fun)),
            op::CONST_BUILTIN => st.push(fresh(Kind::Builtin)),
            op::LOAD_G => {
                let k = gk(enc::imm16(w) as usize, at)?;
                st.push(slotted(k));
            }
            // The peek forms leave the value on the stack. It is now
            // reachable through the slot, so when the slot's annotation is
            // EXACTLY this kind, the slot's promise is a promise about
            // this value: mark it annotated. (A wider slot — `Arr` or
            // `Dyn` holding an `ArrNum` value — promises nothing extra, so
            // the value stays unannotated and keeps its own, more precise,
            // kind.) This is what makes an `ArrNum` global's annotation
            // checkable even when store forwarding hands the fresh array
            // straight on without a reload.
            op::STORE_G => {
                let want = gk(enc::imm16(w) as usize, at)?;
                if st.is_empty() {
                    return Err(err(at, "operand stack underflow".to_string()));
                }
                let v = *st.last().unwrap();
                need!(v.k, want, "StoreG value");
                st.last_mut().unwrap().ann |= want == v.k;
            }
            op::STORE_G_POP => {
                let want = gk(enc::imm16(w) as usize, at)?;
                let v = pop!();
                need!(v.k, want, "StoreG value");
            }
            op::LOAD_L => {
                let k = lk(enc::imm8(w) as usize, at)?;
                st.push(slotted(k));
            }
            op::STORE_L => {
                let want = lk(enc::imm8(w) as usize, at)?;
                if st.is_empty() {
                    return Err(err(at, "operand stack underflow".to_string()));
                }
                let v = *st.last().unwrap();
                need!(v.k, want, "StoreL value");
                st.last_mut().unwrap().ann |= want == v.k;
            }
            op::STORE_L_POP => {
                let want = lk(enc::imm8(w) as usize, at)?;
                let v = pop!();
                need!(v.k, want, "StoreL value");
            }
            op::LOAD_IDX => {
                let _idx = pop!();
                let arr = pop!();
                st.push(fresh(elem_kind(arr.k)));
            }
            op::LOAD_L_IDX => {
                lk(enc::imm8(w) as usize, at)?;
                let arr = pop!();
                st.push(fresh(elem_kind(arr.k)));
            }
            op::LOAD_G_L_IDX => {
                lk(enc::argc(w) as usize, at)?;
                let arr = gk(enc::imm16(w) as usize, at)?;
                st.push(fresh(elem_kind(arr)));
            }
            op::STORE_IDX => {
                let v = pop!();
                let _idx = pop!();
                let arr = pop!();
                if arr.k == Kind::ArrNum && arr.ann {
                    need!(v.k, Kind::Num, "StoreIdx value into an ArrNum array");
                }
                st.push(v);
            }
            op::ARR_LEN => {
                let _ = pop!();
                st.push(fresh(Kind::Num));
            }
            op::NEW_ARRAY => {
                let cnt = enc::imm16(w) as usize;
                let mut all_num = true;
                for _ in 0..cnt {
                    if pop!().k != Kind::Num {
                        all_num = false;
                    }
                }
                st.push(fresh(if all_num { Kind::ArrNum } else { Kind::Arr }));
            }
            // const-pool arrays are all-numeric by construction
            op::CONST_ARR => st.push(fresh(Kind::ArrNum)),
            op::ASSERT => {
                let _ = pop!();
            }
            op::DUP => {
                // §2.4: `Dup` on an empty stack pushes Num 0 in the
                // interpreter; the verifier forbids it, so the JIT never
                // has to materialize that.
                let v = *st
                    .last()
                    .ok_or_else(|| err(at, "Dup on an empty stack".to_string()))?;
                st.push(v);
            }
            op::DUP2 => {
                if st.len() < 2 {
                    return Err(err(at, "Dup2 with fewer than two operands".to_string()));
                }
                let a = st[st.len() - 2];
                let b = st[st.len() - 1];
                st.push(a);
                st.push(b);
            }
            op::POP => {
                let _ = pop!();
            }
            op::NEG | op::NOT | op::BIT_NOT => {
                let _ = pop!();
                st.push(fresh(Kind::Num));
            }
            op::BOX => {
                let v = pop!();
                if v.k == Kind::Dyn {
                    return Err(err(at, "Box of an already-Dyn operand".to_string()));
                }
                st.push(fresh(Kind::Dyn));
            }
            op::CONST_OP => {
                let _ = pop!();
                st.push(fresh(Kind::Num));
            }
            op::LOAD_L_CONST_OP => {
                lk(enc::imm8(w) as usize, at)?;
                st.push(fresh(Kind::Num));
            }
            op::LOAD_G_CONST_OP => {
                gk(enc::imm16(w) as usize, at)?;
                st.push(fresh(Kind::Num));
            }
            op::LOAD_LL => {
                let a = lk(enc::imm8(w) as usize, at)?;
                let b = lk(enc::imm8b(w) as usize, at)?;
                st.push(slotted(a));
                st.push(slotted(b));
            }
            op::LOAD_LG => {
                let a = lk(enc::imm8(w) as usize, at)?;
                let g = gk(enc::imm16hi(w) as usize, at)?;
                st.push(slotted(a));
                st.push(slotted(g));
            }
            op::LOAD_GL => {
                let g = gk(enc::imm16(w) as usize, at)?;
                let a = lk(enc::argc(w) as usize, at)?;
                st.push(slotted(g));
                st.push(slotted(a));
            }
            op::JMP => {
                edges.push((enc::imm24(w), st.clone()));
                terminal = true;
            }
            op::JMP_IF_FALSE => {
                let _ = pop!();
                edges.push((enc::imm24(w), st.clone()));
            }
            // `||` / `&&`: the condition IS the value and is left on the
            // stack by both edges (vm.rs peeks).
            op::JMP_IF_TRUE_PEEK | op::JMP_IF_FALSE_PEEK => {
                if st.is_empty() {
                    return Err(err(at, "operand stack underflow".to_string()));
                }
                edges.push((enc::imm24(w), st.clone()));
            }
            op::CMP_JF => {
                let _ = pop!();
                let _ = pop!();
                edges.push((word(base + at + 1), st.clone()));
            }
            op::CALL_FN => {
                let callee = enc::imm16(w) as usize;
                let argc = enc::argc(w) as usize;
                if callee >= fns.len() {
                    return Err(err(at, "function index out of range".to_string()));
                }
                let np = fns[callee].params as usize;
                let mut args = alloc::vec![fresh(Kind::Num); argc];
                for j in (0..argc).rev() {
                    args[j] = pop!();
                }
                for (j, a) in args.iter().enumerate().take(np) {
                    let want = kinds.fns[callee].slots.get(j).copied().unwrap_or(Kind::Dyn);
                    need!(a.k, want, "call argument");
                }
                st.push(slotted(kinds.ret(callee)));
            }
            op::CALL_BUILTIN | op::CALL_BUILTIN_C | op::CALL_BUILTIN_CC => {
                let b = enc::imm16(w);
                let argc = enc::argc(w) as usize;
                let nconst = match o {
                    op::CALL_BUILTIN_C => 1,
                    op::CALL_BUILTIN_CC => 2,
                    _ => 0,
                };
                let from_stack = argc.saturating_sub(nconst);
                // trailing immediates are always Num
                let mut args = alloc::vec![fresh(Kind::Num); argc];
                for j in (0..from_stack).rev() {
                    args[j] = pop!();
                }
                let sig = builtin_sig(b);
                let a = |i: u8| args.get(i as usize).copied().unwrap_or(fresh(Kind::Num));
                // an array a builtin writes non-Num into cannot be ArrNum
                if let Some((ai, wk)) = sig.writes {
                    let arr = a(ai);
                    if arr.k == Kind::ArrNum && arr.ann {
                        let stored = match wk {
                            SigWrite::Num => Kind::Num,
                            SigWrite::ArgsFrom(i) => {
                                let mut k = Kind::Num;
                                for j in i as usize..argc {
                                    k = k.join(args[j].k);
                                }
                                k
                            }
                        };
                        need!(stored, Kind::Num, "value written into an ArrNum array");
                    }
                }
                st.push(match sig.ret {
                    SigRet::Num => fresh(Kind::Num),
                    SigRet::Dyn => fresh(Kind::Dyn),
                    SigRet::NewArrNum => fresh(Kind::ArrNum),
                    // returned verbatim: it keeps whatever promise it had
                    SigRet::Arg(i) => a(i),
                });
            }
            op::CALL_VALUE => {
                let argc = enc::imm8(w) as usize;
                for _ in 0..argc + 1 {
                    let _ = pop!();
                }
                st.push(fresh(Kind::Dyn));
            }
            op::RET => {
                let v = pop!();
                need!(v.k, fk.ret, "return value");
                terminal = true;
            }
            op::RET_NULL => {
                need!(Kind::Num, fk.ret, "implicit return value");
                terminal = true;
            }
            op::POP_RET_NULL => {
                let _ = pop!();
                need!(Kind::Num, fk.ret, "implicit return value");
                terminal = true;
            }
            o if is_binop(o) => {
                let _ = pop!();
                let _ = pop!();
                st.push(fresh(Kind::Num));
            }
            _ => return Err(err(at, format!("unknown opcode {o:#04x}"))),
        }

        for (t, s) in edges {
            if t as usize > n {
                return Err(err(at, "jump target out of range".to_string()));
            }
            if t as usize == n {
                // "fall off the end" — the implicit RetNull
                continue;
            }
            match states.get(&t) {
                None => {
                    states.insert(t, (s, Origin::Jump(at as u32)));
                }
                Some((rec, orig)) => {
                    let rec = rec.clone();
                    let orig = *orig;
                    match merge_edges(
                        fi,
                        at,
                        &rec,
                        orig,
                        &s,
                        Origin::Jump(at as u32),
                        t,
                        fixes.as_deref_mut(),
                    )? {
                        Some(m) => {
                            states.insert(t, (m, orig));
                        }
                        None => return Ok(()),
                    }
                }
            }
        }
        if terminal {
            cur = None;
        }
        at += len;
    }
    Ok(())
}

/// What a `LoadIdx` off an array of kind `arr` pushes.
#[cfg(feature = "kinds")]
#[inline]
pub fn elem_kind(arr: Kind) -> Kind {
    match arr {
        Kind::ArrNum => Kind::Num,
        Kind::Arr | Kind::Dyn => Kind::Dyn,
        // Indexing a non-array ALWAYS traps (`Vm::index_read`: "indexing a
        // non-array value"), so the value this pushes is unreachable.
        // `Infer::elem_of` has always said `Num` here; this arm is what
        // keeps the verifier from disagreeing with it, which it did until
        // the prelude produced the first program that indexes a slot proven
        // `Num` (`arrayForEach(5, f)` — the helper's parameter is a plain
        // number because that is the only thing any call site passes it).
        _ => Kind::Num,
    }
}

/// Compare two abstract stacks arriving at the same word.
///
/// Returns the joined stack, or `None` when a `Box` fix was recorded (the
/// caller abandons this function; the compiler re-lays it out and the walk
/// starts over). In verify-only mode (`fixes` is `None`) a kind
/// disagreement is a hard error — the compiler is not allowed to ship a
/// join the verifier would have to guess at.
#[cfg(feature = "kinds")]
#[allow(clippy::too_many_arguments)]
fn merge_edges(
    fi: usize,
    at: usize,
    a: &[Ab],
    a_from: Origin,
    b: &[Ab],
    b_from: Origin,
    target: u32,
    fixes: Option<&mut Vec<BoxFix>>,
) -> Result<Option<Vec<Ab>>, KindError> {
    if a.len() != b.len() {
        return Err(KindError {
            fn_idx: fi as u16,
            word: at as u32,
            what: format!(
                "jump target {target} is reached with stack depth {} and {}",
                a.len(),
                b.len()
            ),
        });
    }
    let Some(d) = (0..a.len()).find(|&i| a[i].k != b[i].k) else {
        // kinds agree; a promise on either edge is a promise here
        return Ok(Some(
            (0..a.len())
                .map(|i| Ab {
                    k: a[i].k,
                    ann: a[i].ann || b[i].ann,
                })
                .collect(),
        ));
    };
    if d + 1 != a.len() {
        return Err(KindError {
            fn_idx: fi as u16,
            word: at as u32,
            what: format!(
                "jump target {target} joins {} and {} at stack depth {d}, below the top \
                 (no single Box can fix this)",
                a[d].k.name(),
                b[d].k.name()
            ),
        });
    }
    let Some(fixes) = fixes else {
        return Err(KindError {
            fn_idx: fi as u16,
            word: at as u32,
            what: format!(
                "jump target {target} joins {} and {} — the compiler owes a Box here",
                a[d].k.name(),
                b[d].k.name()
            ),
        });
    };
    // Box the narrower edge(s); with incomparable kinds (Num vs Arr) both
    // edges need one.
    let mut push = |o: Origin| {
        let w = match o {
            Origin::Jump(w) => w,
            Origin::FallThrough => target,
        };
        let fix = BoxFix {
            fn_idx: fi as u16,
            at: w,
            depth: d as u32,
        };
        if !fixes.contains(&fix) {
            fixes.push(fix);
        }
    };
    if a[d].k.le(b[d].k) {
        push(a_from);
    } else if b[d].k.le(a[d].k) {
        push(b_from);
    } else {
        push(a_from);
        push(b_from);
    }
    Ok(None)
}
// =============================================================== INFERENCE

/// Why a slot ended up `Dyn`. The fixed set the library census found
/// (docs/jit-design.md §9); the playground's lint (#627) renders these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DynCause {
    /// Param of a function reached only via a callback or `CallValue`.
    CallbackParam,
    /// Load from an array whose elements are not all `Num`.
    ArrayElem,
    /// Result of calling a function value.
    CallValueResult,
    /// Load from an array of unknown provenance.
    UnknownArray,
    /// A store through an array of unknown provenance poisoned every site.
    Poison,
    /// The slot is assigned two different kinds.
    AssignMerge,
    /// A ternary / short-circuit join of differing kinds.
    JoinMerge,
    /// A builtin whose return kind we cannot pin (`arrayReduce`).
    BuiltinDyn,
    /// Call-argument merge: two call sites pass different kinds.
    CallArgMerge,
    /// An array holds both `Num` and non-`Num` elements.
    ElemMerge,
    /// The host may poke a number into any EXPORTED global
    /// (`Engine::set_var`), and seeds the sensor globals with arrays.
    HostWrite,
}

impl DynCause {
    /// The census's wording, kept verbatim so `jitcensus`'s report format
    /// (docs/tools.md) does not change.
    pub fn text(self) -> &'static str {
        match self {
            DynCause::CallbackParam => "param of a function reached only via callback/CallValue",
            DynCause::ArrayElem => {
                "load from an array whose elements are not all Num (array of arrays / of functions / mixed)"
            }
            DynCause::CallValueResult => "CallValue result (calling a function value)",
            DynCause::UnknownArray => "ArrLoad from an array of unknown provenance",
            DynCause::Poison => "arrays poisoned by a store of unknown provenance",
            DynCause::AssignMerge => "slot holds two different kinds (assignment merge)",
            DynCause::JoinMerge => "ternary / short-circuit join of differing kinds",
            DynCause::BuiltinDyn => "builtin whose return we classify Unknown (arrayReduce)",
            DynCause::CallArgMerge => "call argument merge",
            DynCause::ElemMerge => "array holds both Num and non-Num elements",
            DynCause::HostWrite => {
                "exported global: the host can write a number into it (Engine::set_var)"
            }
        }
    }
}

/// Which slot a [`DynCause`] is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DynSlot {
    Global(u16),
    Local { fn_idx: u16, slot: u8 },
    Ret(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DynReason {
    pub slot: DynSlot,
    pub cause: DynCause,
}

/// Whole-program kind inference (docs/jit-design.md §2.3).
#[cfg(feature = "kinds")]
pub fn infer(prog: &Program) -> Kinds {
    analyze(prog).0
}

/// Every `Dyn` slot of `kinds`, with the reason the inference recorded.
#[cfg(feature = "kinds")]
pub fn explain(prog: &Program, kinds: &Kinds) -> Vec<DynReason> {
    let (_, reasons) = analyze(prog);
    reasons
        .into_iter()
        .filter(|r| match r.slot {
            DynSlot::Global(g) => kinds.global(g as usize) == Kind::Dyn,
            DynSlot::Local { fn_idx, slot } => {
                kinds.slot(fn_idx as usize, slot as usize) == Kind::Dyn
            }
            DynSlot::Ret(f) => kinds.ret(f as usize) == Kind::Dyn,
        })
        .collect()
}

// ---- internal lattice: kinds plus the provenance sets ArrNum needs ----

/// A set of allocation sites, with `top` = "could be anything".
#[cfg(feature = "kinds")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Set {
    top: bool,
    bits: u128,
}

#[cfg(feature = "kinds")]
impl Set {
    fn one(i: usize) -> Set {
        if i < 128 {
            Set {
                top: false,
                bits: 1u128 << i,
            }
        } else {
            Set { top: true, bits: 0 }
        }
    }
    fn union(a: Set, b: Set) -> Set {
        Set {
            top: a.top || b.top,
            bits: a.bits | b.bits,
        }
    }
    fn ids(&self) -> impl Iterator<Item = usize> + '_ {
        (0..128).filter(|i| (self.bits >> i) & 1 == 1)
    }
    fn empty(&self) -> bool {
        !self.top && self.bits == 0
    }
}

/// The inference's internal lattice. `Bot` is "no value reaches here yet";
/// a slot left `Bot` holds `Value::default()` = `Num(0)` at run time, so it
/// lowers to `Num`.
#[cfg(feature = "kinds")]
#[derive(Clone, Copy, Debug)]
enum K {
    Bot,
    Num,
    Arr(Set),
    Fun(Set),
    Bi,
    Top(DynCause),
}

#[cfg(feature = "kinds")]
impl PartialEq for K {
    fn eq(&self, o: &K) -> bool {
        match (self, o) {
            (K::Bot, K::Bot) | (K::Num, K::Num) | (K::Bi, K::Bi) | (K::Top(_), K::Top(_)) => true,
            (K::Arr(a), K::Arr(b)) => a == b,
            (K::Fun(a), K::Fun(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(feature = "kinds")]
fn kjoin(a: K, b: K, why: DynCause) -> K {
    match (a, b) {
        (K::Bot, x) => x,
        (x, K::Bot) => x,
        (K::Top(r), _) => K::Top(r),
        (_, K::Top(r)) => K::Top(r),
        (K::Num, K::Num) => K::Num,
        (K::Bi, K::Bi) => K::Bi,
        (K::Arr(x), K::Arr(y)) => K::Arr(Set::union(x, y)),
        (K::Fun(x), K::Fun(y)) => K::Fun(Set::union(x, y)),
        _ => K::Top(why),
    }
}

#[cfg(feature = "kinds")]
struct An<'p> {
    prog: &'p Program,
    globals: Vec<Option<K>>,
    locals: Vec<Vec<Option<K>>>,
    rets: Vec<Option<K>>,
    /// Element kind per allocation site.
    site_elem: Vec<Option<K>>,
    /// (fn, word) → site id, for the allocating instructions.
    sites: BTreeMap<(usize, usize), usize>,
    poison: bool,
    /// Functions whose params must be `Dyn`: reachable as a value.
    escaped: Set,
    const_fun_fns: Set,
    changed: bool,
    reasons: BTreeSet<DynReason>,
}

#[cfg(feature = "kinds")]
impl<'p> An<'p> {
    fn new(prog: &'p Program) -> Self {
        let nf = prog.fns.len();
        let mut sites = BTreeMap::new();
        let mut nsites = 0usize;
        let mut const_fun_fns = Set::default();
        let mut called = Set::default();
        for (fi, f) in prog.fns.iter().enumerate() {
            let code = fn_code(prog, fi);
            let mut at = 0usize;
            while at < code.len() {
                let w = code[at];
                let o = enc::opcode(w);
                let alloc = match o {
                    op::NEW_ARRAY | op::CONST_ARR => true,
                    op::CALL_BUILTIN | op::CALL_BUILTIN_C | op::CALL_BUILTIN_CC => {
                        matches!(builtin_sig(enc::imm16(w)).ret, SigRet::NewArrNum)
                    }
                    _ => false,
                };
                if alloc {
                    sites.insert((fi, at), nsites);
                    nsites += 1;
                }
                if o == op::CONST_FUN {
                    const_fun_fns = Set::union(const_fun_fns, Set::one(enc::imm16(w) as usize));
                }
                if o == op::CALL_FN {
                    called = Set::union(called, Set::one(enc::imm16(w) as usize));
                }
                at += ilen(o);
            }
            let _ = f;
        }
        // A function with NO call site at all is dead code — the library's
        // sequencer frameworks define API entry points their own script
        // never uses. Its params would otherwise stay Bot (= `Num`, the
        // "never written" default) while its body hands them to callees
        // that expect something else, and the verifier would reject a body
        // that can never run. `Dyn` is the honest kind for "we do not know
        // how this is called", and it propagates into those callees.
        let mut dead = Set::default();
        for f in 1..nf {
            let exported = prog.exported_fns.iter().any(|&(_, i)| i as usize == f);
            let is_called = called.top || (f < 128 && (called.bits >> f) & 1 == 1);
            let is_value = const_fun_fns.top || (f < 128 && (const_fun_fns.bits >> f) & 1 == 1);
            if !exported && !is_called && !is_value {
                dead = Set::union(dead, Set::one(f));
            }
        }
        // Three synthetic sites for the arrays `Engine::from_program*`
        // seeds into the sensor globals before init runs.
        let sensor_site0 = nsites;
        nsites += 3;
        An {
            prog,
            globals: alloc::vec![None; prog.globals.len()],
            locals: prog
                .fns
                .iter()
                .map(|f| alloc::vec![None; f.locals as usize])
                .collect(),
            rets: alloc::vec![None; nf],
            site_elem: alloc::vec![None; nsites],
            sites,
            poison: false,
            escaped: dead,
            const_fun_fns,
            changed: false,
            reasons: BTreeSet::new(),
        }
        .seed(sensor_site0)
    }

    /// Everything the ENGINE writes into a global before or between the
    /// pattern's own stores. These are stores too, and leaving them out
    /// would let the JIT unbox a slot the host can overwrite.
    fn seed(mut self, sensor_site0: usize) -> Self {
        for (k, name) in ["frequencyData", "accelerometer", "analogInputs"]
            .into_iter()
            .enumerate()
        {
            if let Some(g) = self.prog.global_index(name) {
                if self.prog.globals[g as usize].export {
                    let s = sensor_site0 + k;
                    self.bump_site(s, K::Num);
                    self.bump_g(g as usize, K::Arr(Set::one(s)));
                }
            }
        }
        // `Engine::set_var` writes `Value::Num` into ANY exported global.
        for g in 0..self.prog.globals.len() {
            if self.prog.globals[g].export {
                self.bump_g_why(g, K::Num, DynCause::HostWrite);
            }
        }
        // Every exported function is callable by the engine (render
        // entries, beforeRender, controls) with `Value::Num` arguments.
        for &(_, fidx) in &self.prog.exported_fns {
            let f = fidx as usize;
            if f < self.prog.fns.len() {
                for j in 0..self.prog.fns[f].params as usize {
                    self.bump_l(f, j, K::Num);
                }
            }
        }
        self
    }

    fn note(&mut self, slot: DynSlot, k: K) {
        if let K::Top(cause) = k {
            self.reasons.insert(DynReason { slot, cause });
        }
    }

    fn bump_g(&mut self, g: usize, k: K) {
        self.bump_g_why(g, k, DynCause::AssignMerge)
    }
    fn bump_g_why(&mut self, g: usize, k: K, why: DynCause) {
        let n = match self.globals[g] {
            None => k,
            Some(o) => kjoin(o, k, why),
        };
        if self.globals[g] != Some(n) {
            self.globals[g] = Some(n);
            self.changed = true;
        }
        self.note(DynSlot::Global(g as u16), n);
    }
    fn bump_l(&mut self, f: usize, i: usize, k: K) {
        if i >= self.locals[f].len() {
            return;
        }
        let n = match self.locals[f][i] {
            None => k,
            Some(o) => kjoin(o, k, DynCause::AssignMerge),
        };
        if self.locals[f][i] != Some(n) {
            self.locals[f][i] = Some(n);
            self.changed = true;
        }
        self.note(
            DynSlot::Local {
                fn_idx: f as u16,
                slot: i as u8,
            },
            n,
        );
    }
    fn gk(&self, g: usize) -> K {
        self.globals.get(g).copied().flatten().unwrap_or(K::Bot)
    }
    fn lk(&self, f: usize, i: usize) -> K {
        self.locals[f].get(i).copied().flatten().unwrap_or(K::Bot)
    }
    fn bump_ret(&mut self, f: usize, k: K) {
        let n = match self.rets[f] {
            None => k,
            Some(o) => kjoin(o, k, DynCause::AssignMerge),
        };
        if self.rets[f] != Some(n) {
            self.rets[f] = Some(n);
            self.changed = true;
        }
        self.note(DynSlot::Ret(f as u16), n);
    }
    fn bump_site(&mut self, s: usize, k: K) {
        if matches!(k, K::Bot) {
            return;
        }
        let n = match self.site_elem[s] {
            None => k,
            Some(o) => kjoin(o, k, DynCause::ElemMerge),
        };
        if self.site_elem[s] != Some(n) {
            self.site_elem[s] = Some(n);
            self.changed = true;
        }
    }
    fn se(&self, s: usize) -> K {
        self.site_elem[s].unwrap_or(K::Bot)
    }
    fn set_poison(&mut self) {
        if !self.poison {
            self.poison = true;
            self.changed = true;
        }
    }
    fn escape_all(&mut self) {
        let n = Set::union(self.escaped, self.const_fun_fns);
        if n != self.escaped {
            self.escaped = n;
            self.changed = true;
        }
    }
    fn ret_of(&self, f: usize) -> K {
        self.rets[f].unwrap_or(K::Bot)
    }

    /// Element kind read out of an array-typed value.
    fn elem_of(&mut self, a: K) -> K {
        match a {
            K::Arr(s) => {
                if self.poison {
                    return K::Top(DynCause::Poison);
                }
                if s.top || s.empty() {
                    return K::Top(DynCause::UnknownArray);
                }
                let mut k: Option<K> = None;
                for i in s.ids() {
                    let e = self.se(i);
                    k = Some(match k {
                        None => e,
                        Some(p) => kjoin(p, e, DynCause::ArrayElem),
                    });
                }
                match k.unwrap_or(K::Bot) {
                    K::Num | K::Bot => K::Num,
                    K::Top(_) => K::Top(DynCause::ArrayElem),
                    _ => K::Top(DynCause::ArrayElem),
                }
            }
            K::Top(_) => K::Top(DynCause::UnknownArray),
            K::Bot => K::Bot,
            // indexing a non-array always traps; the value is unreachable
            _ => K::Num,
        }
    }

    fn store_into(&mut self, arr: K, val: K) {
        if matches!(val, K::Num | K::Bot) {
            return;
        }
        match arr {
            K::Arr(s) if !s.top && !s.empty() => {
                for i in s.ids().collect::<Vec<_>>() {
                    self.bump_site(i, val);
                }
            }
            K::Bot => {}
            // a store through an array of unknown provenance demotes every
            // site: the poison rule (§2.3)
            _ => self.set_poison(),
        }
    }

    /// A function value CALLED AS A VALUE is callable from anywhere with
    /// anything: every `ConstFun`-referenced function's params go `Dyn`.
    fn callback(&mut self, cb: K) {
        if matches!(cb, K::Bi) {
            return;
        }
        self.escape_all();
    }

    fn classify_builtin(&mut self, fi: usize, at: usize, id: u16, args: &[K]) -> K {
        let sig = builtin_sig(id);
        let a = |i: u8| args.get(i as usize).copied().unwrap_or(K::Bot);
        // What the builtin writes into its array argument.
        if let Some((ai, wk)) = sig.writes {
            let v = match wk {
                SigWrite::Num => K::Num,
                SigWrite::ArgsFrom(i) => {
                    let mut k = K::Bot;
                    for x in args.iter().skip(i as usize) {
                        k = kjoin(k, *x, DynCause::CallArgMerge);
                    }
                    k
                }
            };
            let arr = a(ai);
            self.store_into(arr, v);
        }
        match sig.ret {
            SigRet::NewArrNum => {
                let s = self.sites[&(fi, at)];
                // `array(n)` zero-fills, so its elements start out Num.
                self.bump_site(s, K::Num);
                K::Arr(Set::one(s))
            }
            SigRet::Dyn => K::Top(DynCause::BuiltinDyn),
            SigRet::Arg(i) => a(i),
            SigRet::Num => K::Num,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn analyze_fn(&mut self, fi: usize) {
        let code = fn_code(self.prog, fi).to_vec();
        let nparams = self.prog.fns[fi].params as usize;
        if self.escaped.top || (fi < 128 && (self.escaped.bits >> fi) & 1 == 1) {
            for j in 0..nparams {
                self.bump_l(fi, j, K::Top(DynCause::CallbackParam));
            }
        }
        let mut states: BTreeMap<usize, Vec<K>> = BTreeMap::new();
        states.insert(0, Vec::new());
        let mut work = alloc::vec![0usize];
        let mut guard = 0usize;
        while let Some(at) = work.pop() {
            guard += 1;
            if guard > 400_000 || at >= code.len() {
                continue;
            }
            let mut st = states[&at].clone();
            let w = code[at];
            let o = enc::opcode(w);
            let next = at + ilen(o);
            let mut succ: Vec<(usize, Vec<K>)> = Vec::new();
            macro_rules! pop {
                () => {
                    st.pop().unwrap_or(K::Bot)
                };
            }
            match o {
                op::CONST_NUM => st.push(K::Num),
                op::CONST_FUN => st.push(K::Fun(Set::one(enc::imm16(w) as usize))),
                op::CONST_BUILTIN => st.push(K::Bi),
                op::LOAD_G => {
                    let k = self.gk(enc::imm16(w) as usize);
                    st.push(k);
                }
                op::STORE_G => {
                    let v = *st.last().unwrap_or(&K::Bot);
                    self.bump_g(enc::imm16(w) as usize, v);
                }
                op::STORE_G_POP => {
                    let v = pop!();
                    self.bump_g(enc::imm16(w) as usize, v);
                }
                op::LOAD_L => {
                    let k = self.lk(fi, enc::imm8(w) as usize);
                    st.push(k);
                }
                op::STORE_L => {
                    let v = *st.last().unwrap_or(&K::Bot);
                    self.bump_l(fi, enc::imm8(w) as usize, v);
                }
                op::STORE_L_POP => {
                    let v = pop!();
                    self.bump_l(fi, enc::imm8(w) as usize, v);
                }
                op::LOAD_IDX => {
                    let _ = pop!();
                    let a = pop!();
                    let e = self.elem_of(a);
                    st.push(e);
                }
                op::LOAD_L_IDX => {
                    let a = pop!();
                    let e = self.elem_of(a);
                    st.push(e);
                }
                op::LOAD_G_L_IDX => {
                    let gv = self.gk(enc::imm16(w) as usize);
                    let e = self.elem_of(gv);
                    st.push(e);
                }
                op::STORE_IDX => {
                    let v = pop!();
                    let _ = pop!();
                    let a = pop!();
                    self.store_into(a, v);
                    st.push(v);
                }
                op::ARR_LEN => {
                    let _ = pop!();
                    st.push(K::Num);
                }
                op::NEW_ARRAY => {
                    let cnt = enc::imm16(w) as usize;
                    let mut e = if cnt == 0 { K::Num } else { K::Bot };
                    for _ in 0..cnt {
                        e = kjoin(e, pop!(), DynCause::ElemMerge);
                    }
                    let s = self.sites[&(fi, at)];
                    self.bump_site(s, e);
                    st.push(K::Arr(Set::one(s)));
                }
                op::CONST_ARR => {
                    let s = self.sites[&(fi, at)];
                    self.bump_site(s, K::Num);
                    st.push(K::Arr(Set::one(s)));
                }
                op::DUP => {
                    let v = *st.last().unwrap_or(&K::Bot);
                    st.push(v);
                }
                op::DUP2 => {
                    let n = st.len();
                    let a = if n >= 2 { st[n - 2] } else { K::Bot };
                    let b = if n >= 1 { st[n - 1] } else { K::Bot };
                    st.push(a);
                    st.push(b);
                }
                op::POP | op::ASSERT => {
                    let _ = pop!();
                }
                op::NEG | op::NOT | op::BIT_NOT => {
                    let _ = pop!();
                    st.push(K::Num);
                }
                op::BOX => {
                    let _ = pop!();
                    st.push(K::Top(DynCause::JoinMerge));
                }
                op::CONST_OP => {
                    let _ = pop!();
                    st.push(K::Num);
                }
                op::LOAD_L_CONST_OP | op::LOAD_G_CONST_OP => st.push(K::Num),
                op::LOAD_LL => {
                    let a = self.lk(fi, enc::imm8(w) as usize);
                    let b = self.lk(fi, enc::imm8b(w) as usize);
                    st.push(a);
                    st.push(b);
                }
                op::LOAD_LG => {
                    let a = self.lk(fi, enc::imm8(w) as usize);
                    let g = self.gk(enc::imm16hi(w) as usize);
                    st.push(a);
                    st.push(g);
                }
                op::LOAD_GL => {
                    let g = self.gk(enc::imm16(w) as usize);
                    let a = self.lk(fi, enc::argc(w) as usize);
                    st.push(g);
                    st.push(a);
                }
                op::JMP => succ.push((enc::imm24(w) as usize, st.clone())),
                op::JMP_IF_FALSE => {
                    let _ = pop!();
                    succ.push((enc::imm24(w) as usize, st.clone()));
                    succ.push((next, st.clone()));
                }
                op::JMP_IF_TRUE_PEEK | op::JMP_IF_FALSE_PEEK => {
                    succ.push((enc::imm24(w) as usize, st.clone()));
                    succ.push((next, st.clone()));
                }
                op::CMP_JF => {
                    let _ = pop!();
                    let _ = pop!();
                    let t = code.get(at + 1).copied().unwrap_or(0) as usize;
                    succ.push((t, st.clone()));
                    succ.push((next, st.clone()));
                }
                op::CALL_FN => {
                    let f2 = enc::imm16(w) as usize;
                    let argc = enc::argc(w) as usize;
                    let mut args = alloc::vec![K::Num; argc];
                    for j in (0..argc).rev() {
                        args[j] = pop!();
                    }
                    let np = self.prog.fns[f2].params as usize;
                    for (j, a) in args.iter().enumerate().take(np) {
                        self.bump_l(f2, j, *a);
                    }
                    let r = self.ret_of(f2);
                    st.push(r);
                }
                op::CALL_BUILTIN | op::CALL_BUILTIN_C | op::CALL_BUILTIN_CC => {
                    let b = enc::imm16(w);
                    let argc = enc::argc(w) as usize;
                    let nconst = match o {
                        op::CALL_BUILTIN_C => 1,
                        op::CALL_BUILTIN_CC => 2,
                        _ => 0,
                    };
                    let from_stack = argc.saturating_sub(nconst);
                    let mut args = alloc::vec![K::Num; argc];
                    for j in (0..from_stack).rev() {
                        args[j] = pop!();
                    }
                    let r = self.classify_builtin(fi, at, b, &args);
                    st.push(r);
                }
                op::CALL_VALUE => {
                    let argc = enc::imm8(w) as usize;
                    for _ in 0..argc {
                        let _ = pop!();
                    }
                    let callee = pop!();
                    self.callback(callee);
                    st.push(K::Top(DynCause::CallValueResult));
                }
                op::RET => {
                    let v = pop!();
                    self.bump_ret(fi, v);
                }
                op::RET_NULL => self.bump_ret(fi, K::Num),
                op::POP_RET_NULL => {
                    let _ = pop!();
                    self.bump_ret(fi, K::Num);
                }
                o if is_binop(o) => {
                    let _ = pop!();
                    let _ = pop!();
                    st.push(K::Num);
                }
                _ => {}
            }
            let terminal = matches!(
                o,
                op::JMP | op::RET | op::RET_NULL | op::POP_RET_NULL
            );
            if succ.is_empty() && !terminal {
                succ.push((next, st.clone()));
            }
            for (t, s) in succ {
                if t >= code.len() {
                    continue;
                }
                match states.get(&t) {
                    None => {
                        states.insert(t, s);
                        work.push(t);
                    }
                    Some(old) => {
                        if old.len() != s.len() {
                            continue;
                        }
                        let mut merged = Vec::with_capacity(s.len());
                        for (i, k) in s.iter().enumerate() {
                            merged.push(kjoin(old[i], *k, DynCause::JoinMerge));
                        }
                        if merged != *old {
                            states.insert(t, merged);
                            work.push(t);
                        }
                    }
                }
            }
        }
    }

    fn run(&mut self) {
        // Conservative mode (the one §9's numbers were measured in): every
        // function ever named by a `ConstFun` is reachable as a callback.
        self.escape_all();
        for _ in 0..80 {
            self.changed = false;
            for fi in 0..self.prog.fns.len() {
                self.analyze_fn(fi);
            }
            if !self.changed {
                break;
            }
        }
    }

    fn lower(&self, k: K) -> Kind {
        match k {
            // never written ⇒ Value::default() = Num(0)
            K::Bot | K::Num => Kind::Num,
            K::Bi => Kind::Builtin,
            K::Fun(_) => Kind::Fun,
            K::Top(_) => Kind::Dyn,
            K::Arr(s) => {
                let all_num = !self.poison
                    && !s.top
                    && !s.empty()
                    && s.ids().all(|i| matches!(self.se(i), K::Num | K::Bot));
                if all_num {
                    Kind::ArrNum
                } else {
                    Kind::Arr
                }
            }
        }
    }
}

#[cfg(feature = "kinds")]
fn fn_code(prog: &Program, fi: usize) -> &[u32] {
    let f = &prog.fns[fi];
    let s = f.code_start as usize;
    &prog.words[s..s + f.code_len as usize]
}

#[cfg(feature = "kinds")]
fn analyze(prog: &Program) -> (Kinds, Vec<DynReason>) {
    let mut an = An::new(prog);
    // §2.3: the declared initial value is a store too — every global's
    // `init` is an `Fx`, so it contributes `Num` — EXCEPT where the init
    // function definitely assigns the global before anything can read it.
    let exempt = init_definitely_assigns(prog);
    for g in 0..prog.globals.len() {
        if !exempt.contains(&g) {
            an.bump_g_why(g, K::Num, DynCause::AssignMerge);
        }
    }
    an.run();

    let kinds = Kinds {
        globals: (0..prog.globals.len())
            .map(|g| an.lower(an.gk(g)))
            .collect(),
        fns: (0..prog.fns.len())
            .map(|f| FnKinds {
                ret: an.lower(an.ret_of(f)),
                slots: (0..prog.fns[f].locals as usize)
                    .map(|i| an.lower(an.lk(f, i)))
                    .collect(),
            })
            .collect(),
    };
    (kinds, an.reasons.into_iter().collect())
}

/// Per function: which globals it can read, transitively through its
/// calls, and whether it can read ANY of them (a `CallValue` goes somewhere
/// this analysis cannot follow; no BUILTIN can, since #626).
#[cfg(feature = "kinds")]
fn global_reads(prog: &Program) -> (Vec<BTreeSet<usize>>, Vec<bool>) {
    let nf = prog.fns.len();
    let mut reads: Vec<BTreeSet<usize>> = alloc::vec![BTreeSet::new(); nf];
    let mut all = alloc::vec![false; nf];
    let mut callees: Vec<BTreeSet<usize>> = alloc::vec![BTreeSet::new(); nf];
    for fi in 0..nf {
        let code = fn_code(prog, fi);
        let mut at = 0usize;
        while at < code.len() {
            let w = code[at];
            let o = enc::opcode(w);
            match o {
                op::LOAD_G | op::LOAD_G_L_IDX | op::LOAD_G_CONST_OP | op::LOAD_GL => {
                    reads[fi].insert(enc::imm16(w) as usize);
                }
                op::LOAD_LG => {
                    reads[fi].insert(enc::imm16hi(w) as usize);
                }
                op::CALL_FN => {
                    callees[fi].insert(enc::imm16(w) as usize);
                }
                op::CALL_VALUE => all[fi] = true,
                _ => {}
            }
            at += ilen(o);
        }
    }
    // transitive closure over the call graph
    for _ in 0..nf + 1 {
        let mut changed = false;
        for fi in 0..nf {
            let cs: Vec<usize> = callees[fi].iter().copied().collect();
            for c in cs {
                if c >= nf {
                    continue;
                }
                if all[c] && !all[fi] {
                    all[fi] = true;
                    changed = true;
                }
                let add: Vec<usize> = reads[c].difference(&reads[fi]).copied().collect();
                if !add.is_empty() {
                    changed = true;
                    reads[fi].extend(add);
                }
            }
        }
        if !changed {
            break;
        }
    }
    (reads, all)
}

/// Globals the init function (fn 0) provably assigns before any read —
/// the §2.3 exemption without which `var hues = array(n)` — an array
/// global whose DECLARED init is the `Fx` zero — would be `Dyn`.
///
/// The rule: some `StoreG g` in init DOMINATES every `LoadG g` in init,
/// and no call that could READ `g` can execute before that store (§2.3
/// says "no call to a pattern function precedes that store"; refining
/// "any call" to "a call that can reach a `LoadG g`" is strictly sound and
/// is worth 22 of 307 library patterns, because a top-level init that builds
/// several arrays calls helpers between them). Dominators are computed on
/// the init function's CFG only.
///
/// **This rule assumes init RUNS TO COMPLETION.** A runtime error in init
/// (an out-of-bounds read, an array-budget refusal) aborts it and the
/// engine renders anyway, so the store may never have happened. A consumer
/// that unboxes on the strength of these kinds — the JIT — must fall back
/// to the interpreter when init errored. Dropping the exemption instead
/// would cost the whole `ArrNum` result, since `array(n)` is itself
/// fallible (docs/jit-design.md §2.3, docs/spec/bytecode.md).
///
/// Note that an EXPLICIT `var hues = 0` emits a real `StoreG` of `Num`,
/// which is a second kind the slot genuinely holds and which the verifier
/// sees; only the declared init is dropped.
#[cfg(feature = "kinds")]
fn init_definitely_assigns(prog: &Program) -> BTreeSet<usize> {
    let (callee_reads, reads_all) = global_reads(prog);
    let mut out = BTreeSet::new();

    if prog.fns.is_empty() {
        return out;
    }
    let code = fn_code(prog, 0);
    let n = code.len();
    if n == 0 {
        return out;
    }
    // instruction starts, successors, and the per-word reads/writes
    let mut starts: Vec<usize> = Vec::new();
    let mut at = 0usize;
    while at < n {
        starts.push(at);
        at += ilen(enc::opcode(code[at]));
    }
    let mut succ: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    // call word -> callee fn index (None = could be anything)
    let mut calls: BTreeMap<usize, Option<usize>> = BTreeMap::new();
    let mut reads: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut writes: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (si, &at) in starts.iter().enumerate() {
        let w = code[at];
        let o = enc::opcode(w);
        let next = starts.get(si + 1).copied();
        let mut s: Vec<usize> = Vec::new();
        let tgt = |t: u32| -> Option<usize> {
            let t = t as usize;
            if t < n {
                Some(t)
            } else {
                None
            }
        };
        match o {
            op::JMP => {
                if let Some(t) = tgt(enc::imm24(w)) {
                    s.push(t);
                }
            }
            op::JMP_IF_FALSE | op::JMP_IF_TRUE_PEEK | op::JMP_IF_FALSE_PEEK => {
                if let Some(t) = tgt(enc::imm24(w)) {
                    s.push(t);
                }
                s.extend(next);
            }
            op::CMP_JF => {
                if let Some(t) = tgt(code.get(at + 1).copied().unwrap_or(0)) {
                    s.push(t);
                }
                s.extend(next);
            }
            op::RET | op::RET_NULL | op::POP_RET_NULL => {}
            _ => s.extend(next),
        }
        match o {
            op::LOAD_G | op::LOAD_G_L_IDX | op::LOAD_G_CONST_OP => {
                reads.entry(at).or_default().push(enc::imm16(w) as usize)
            }
            op::LOAD_GL => reads.entry(at).or_default().push(enc::imm16(w) as usize),
            op::LOAD_LG => reads
                .entry(at)
                .or_default()
                .push(enc::imm16hi(w) as usize),
            op::STORE_G | op::STORE_G_POP => {
                writes.entry(at).or_default().push(enc::imm16(w) as usize)
            }
            _ => {}
        }
        // A call before the store matters only if the callee can READ the
        // global — that is the whole reason the init value has to stay in
        // the join. A `CallValue` could go anywhere, so it reads everything;
        // a builtin cannot reach pattern code at all since #626.
        match o {
            op::CALL_FN => {
                calls.insert(at, Some(enc::imm16(w) as usize));
            }
            op::CALL_VALUE => {
                calls.insert(at, None);
            }
            _ => {}
        }
        succ.insert(at, s);
    }

    // Dominators over the init CFG, Cooper/Harvey/Kennedy: reverse
    // postorder plus an idom array, which is linear in the function rather
    // than the quadratic bitset a top-level init (thousands of
    // instructions) would need.
    let idx: BTreeMap<usize, usize> = starts.iter().enumerate().map(|(i, &a)| (a, i)).collect();
    let m = starts.len();
    let mut preds: Vec<Vec<usize>> = alloc::vec![Vec::new(); m];
    let mut succ_i: Vec<Vec<usize>> = alloc::vec![Vec::new(); m];
    for (&a, ss) in &succ {
        for &t in ss {
            if let (Some(&i), Some(&j)) = (idx.get(&a), idx.get(&t)) {
                preds[j].push(i);
                succ_i[i].push(j);
            }
        }
    }
    // postorder (iterative DFS)
    let mut post: Vec<usize> = Vec::with_capacity(m);
    let mut visited = alloc::vec![false; m];
    let mut stack: Vec<(usize, usize)> = alloc::vec![(0, 0)];
    visited[0] = true;
    while let Some(&mut (nd, ref mut k)) = stack.last_mut() {
        if *k < succ_i[nd].len() {
            let t = succ_i[nd][*k];
            *k += 1;
            if !visited[t] {
                visited[t] = true;
                stack.push((t, 0));
            }
        } else {
            post.push(nd);
            stack.pop();
        }
    }
    // rpo_num[n] = position in reverse postorder (entry = 0)
    let mut rpo_num = alloc::vec![usize::MAX; m];
    for (i, &nd) in post.iter().rev().enumerate() {
        rpo_num[nd] = i;
    }
    let rpo: Vec<usize> = post.iter().rev().copied().collect();
    const NONE: usize = usize::MAX;
    let mut idom = alloc::vec![NONE; m];
    idom[0] = 0;
    loop {
        let mut changed = false;
        for &b in rpo.iter().skip(1) {
            let mut new: usize = NONE;
            for &p in &preds[b] {
                if idom[p] == NONE {
                    continue;
                }
                new = if new == NONE {
                    p
                } else {
                    // intersect(p, new) over the idom tree
                    let (mut x, mut y) = (p, new);
                    while x != y {
                        while rpo_num[x] > rpo_num[y] {
                            x = idom[x];
                        }
                        while rpo_num[y] > rpo_num[x] {
                            y = idom[y];
                        }
                    }
                    x
                };
            }
            if new != NONE && idom[b] != new {
                idom[b] = new;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // `a` dominates `b` iff `a` is on `b`'s idom chain. Unreachable code
    // (no idom) can never read anything, so it dominates vacuously.
    let dominates = |a: usize, b: usize| -> bool {
        if rpo_num[b] == usize::MAX {
            return true;
        }
        let mut x = b;
        loop {
            if x == a {
                return true;
            }
            if x == 0 || idom[x] == NONE || idom[x] == x {
                return false;
            }
            x = idom[x];
        }
    };

    // words reachable from entry WITHOUT passing through `s`
    let before = |s: usize| -> BTreeSet<usize> {
        let mut seen = BTreeSet::new();
        let mut stack = alloc::vec![starts[0]];
        while let Some(a) = stack.pop() {
            if a == starts[s] || !seen.insert(a) {
                continue;
            }
            for &t in succ.get(&a).map(|v| v.as_slice()).unwrap_or(&[]) {
                stack.push(t);
            }
        }
        seen
    };

    for g in 0..prog.globals.len() {
        let stores: Vec<usize> = starts
            .iter()
            .enumerate()
            .filter(|(_, &a)| writes.get(&a).is_some_and(|v| v.contains(&g)))
            .map(|(i, _)| i)
            .collect();
        if stores.is_empty() {
            continue;
        }
        let loads: Vec<usize> = starts
            .iter()
            .enumerate()
            .filter(|(_, &a)| reads.get(&a).is_some_and(|v| v.contains(&g)))
            .map(|(i, _)| i)
            .collect();
        let Some(&s) = stores
            .iter()
            .find(|&&s| loads.iter().all(|&l| dominates(s, l)))
        else {
            continue;
        };
        // No call that could READ `g` may execute before the store.
        let pre = before(s);
        if calls.iter().any(|(c, callee)| {
            pre.contains(c)
                && match callee {
                    None => true,
                    Some(f) => reads_all[*f] || callee_reads[*f].contains(&g),
                }
        }) {
            continue;
        }
        out.insert(g);
    }
    out
}
