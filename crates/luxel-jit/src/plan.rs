//! Per-function planning: the frame layout of docs/jit-design.md §3.3 and
//! the register plan of §3.4, decided once before a byte is emitted.
//!
//! The frame does NOT match §3.3's sketch, and the difference is the one
//! that mattered: the window save areas are 32 bytes at the TOP, not 16 at
//! `a1+0` (see [`WINDOW_SAVE`] — Gitea #658).
//!
//! Everything here is a pure function of the bytecode and its `kinds`
//! section, so the emitter never has to reconsider a decision mid-stream —
//! which is what lets it be a single pass with no allocator.

use alloc::vec::Vec;

use luxel_core::bytecode::{enc, op};
use luxel_core::jit::dev32;
use luxel_core::kinds::{ilen, Kind, Kinds, StackMap};
use luxel_core::vm::Program;

use crate::Refusal;

/// Bytes at the **TOP** of every generated frame — `[a1 + frame - 32,
/// a1 + frame)` — that the window mechanism may spill into. Never touched
/// by generated code.
///
/// **§3.3 had this wrong in both the place and the size, and it was a real
/// crash** (Gitea #658). The design put 16 bytes at `a1+0`, the BOTTOM.
/// The Xtensa windowed ABI puts the save areas just below the CALLER's
/// stack pointer, and `entry a1, N` sets `a1 = caller_sp - N`, so they
/// live at the top of the callee's frame:
///
/// ```text
///   a1 + frame      ── caller's sp
///   a1 + frame - 16 ── base save area:  caller's a0…a3
///   a1 + frame - 32 ── call8 extra:     caller's a4…a7
///   …                  locals, operand-stack homes, boxed-args scratch
///   a1 + 0
/// ```
///
/// Those are not hypothetical: `xtensa-lx-rt`'s `_WindowOverflow8` writes
/// `a0…a3` with `s32e aX, a9, -16…-4` and `a4…a7` with
/// `s32e aX, a0, -32…-20`, both relative to the saved sp. With the old
/// layout, any generated function deep enough to take a window-overflow
/// exception had its TOP 32 bytes of data silently overwritten by the
/// handler — and on the way back the underflow handler reloaded a
/// corrupted `a1`, which is the wild store `tools/qemu/jit-test.py` caught
/// on `aurora-2d.js` and `bulk-canvas-ripples-2d.js`.
///
/// 32 and not 48: 48 is what a `call12` caller needs, and nothing calls
/// generated code with one. The emitter only ever emits `callx8`, and the
/// engine enters through a Rust `extern "C"` pointer, which is `call8` on
/// these targets.
pub const WINDOW_SAVE: u32 = 32;

/// Uniform stride of a frame home: eight bytes, laid out as a
/// [`luxel_core::vm::ValueRaw`] (tag at +0, payload at +4) whether or not
/// the slot is `Dyn`.
///
/// **Deviation from §3.3**, which sizes a home by kind (4 B for
/// `Num`/handle, 8 B for `Dyn`). One stride means one offset formula for
/// locals, stack homes and boxed arguments alike, and a `Dyn` home is then
/// byte-identical to the `Value` a builtin wrapper wants. A function's
/// median frame is a few dozen bytes either way.
pub const HOME: u32 = dev32::VALUE;

/// Where the payload word of a home lives, by kind. A non-`Dyn` slot is a
/// raw word in a frame home private to generated code, so it sits at +0; a
/// `Dyn` home IS a `Value`, so its payload is at +4 behind the tag.
#[inline]
pub fn payload_at(home: u32, k: Kind) -> u32 {
    if k == Kind::Dyn {
        home + dev32::VALUE_PAYLOAD
    } else {
        home
    }
}

/// The tag word of a `Dyn` home.
#[inline]
pub fn tag_at(home: u32) -> u32 {
    home + dev32::VALUE_TAG
}

/// Where one value lives while it is live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotHome {
    /// An address register that holds it for the whole function (a
    /// register-convention parameter) or for the current basic block (a
    /// register-homed operand-stack depth).
    Reg(u8),
    /// A byte offset from `a1`.
    Frame(u32),
}

/// How a function takes its arguments.
///
/// **Simplification of §3.2's "params 0–4 in `a3…a7`, the rest and every
/// `Dyn` one through `ctx.args`".** Mixing the two per parameter would make
/// the register assignment depend on which parameters happen to be `Dyn`;
/// v1 picks one convention for the whole function instead, so caller and
/// callee agree by looking at the same one bit. The common case — all
/// parameters `Num`, at most five of them — still lands in registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamConv {
    /// Parameters 0..n in `a3…a7` (callee view); `n ≤ 5` and none is `Dyn`.
    Regs,
    /// Every parameter through `JitCtx::args`, one word for a non-`Dyn`
    /// one and two (tag, payload) for a `Dyn` one, in parameter order. The
    /// prologue copies them into frame homes before anything else runs, so
    /// the handoff area is live only across the call instruction (§3.2).
    CtxArgs,
}

/// Stack depths `0..REG_DEPTHS` live in `a8 + d` while their basic block
/// runs, provided they are not `Dyn`.
///
/// **Deviation from §3.4**, which homes depths 0–5 in `a8…a13` and keeps
/// only `a14`/`a15` as scratch. Two scratch registers are nowhere near
/// enough — the `value_eq` sequence alone wants three live at once, a
/// `Dyn` operand costs one more each, and a wide frame offset borrows
/// another — and the first cut of this emitter with four scratch registers
/// still ran out on real library patterns. v1 therefore keeps only depths
/// 0 and 1 in registers and gives the other six to the scratch pool.
///
/// What that costs is small: the census (§9b) measured the median
/// function's peak operand depth at 2, so the two register homes cover the
/// shape that matters — `LoadL x; LoadL y; Add` is still `add a8, a3, a4`
/// — and everything deeper was going to spill under any v1 plan. A real
/// allocator is the phase-4 lever.
pub const REG_DEPTHS: usize = 2;
/// First register-homed depth.
pub const REG_DEPTH_BASE: u8 = 8;
/// Scratch registers, clobbered freely within one bytecode instruction and
/// never live across a call.
///
/// These ARE the outgoing argument registers (`a10…a15`), which is safe
/// only because `spill_all` runs before every call: scratch is never live
/// when arguments are being marshalled, and the marshalling itself borrows
/// `a8`/`a9`, which the spill has just emptied.
pub const SCRATCH: [u8; 6] = [10, 11, 12, 13, 14, 15];
/// First register-convention parameter register (callee view).
pub const PARAM_BASE: u8 = 3;
/// How many parameters [`ParamConv::Regs`] can carry.
pub const MAX_REG_PARAMS: usize = 5;

/// Everything the emitter decided about one function before emitting it.
#[derive(Clone, Debug)]
pub struct FnPlan {
    pub fn_idx: u16,
    pub params: usize,
    pub locals: usize,
    /// Annotated kind per local slot (parameters first).
    pub slot_kind: Vec<Kind>,
    /// This function's return kind.
    pub ret: Kind,
    pub conv: ParamConv,
    /// Frame home of each local, or `None` for a register-convention
    /// parameter (which lives in `a3 + i` for the whole function).
    pub local_home: Vec<Option<u32>>,
    /// `JitCtx::args` BYTE offset of each parameter under
    /// [`ParamConv::CtxArgs`].
    pub arg_off: Vec<u32>,
    /// Frame home of operand-stack depth `d`.
    pub stack_home: Vec<u32>,
    /// Frame offset of the boxed-argument scratch, `[Value; scratch_n]`.
    pub scratch_off: u32,
    pub scratch_n: usize,
    /// `entry`'s frame size.
    pub frame: u32,
    /// The abstract stack on arrival at every word (`kinds::stack_maps`).
    pub map: StackMap,
    /// Words that some branch targets — the basic-block starts at which
    /// every register-homed depth is considered spilled.
    pub block_start: Vec<bool>,
    /// Words that a BACKWARD branch targets: the fuel check goes there
    /// (§3.6).
    pub back_target: Vec<bool>,
    /// This function is named by a `ConstFun` somewhere, so it can be
    /// reached through `CallValue` with a callee the call site cannot
    /// name. It therefore returns its value BOXED — `(tag, payload)` in
    /// `a2:a3` — whatever its declared return kind is, because that is the
    /// only shape a dynamic caller can read. Direct `CallFn` callers know
    /// the index statically and read the same shape.
    ///
    /// **This is not in §3.2.** The design assumed a function reachable as
    /// a value has a `Dyn` return; the inference does not guarantee it —
    /// `function twice(v) { return v * 2 }` used as a value keeps a `Num`
    /// return, and a `CallValue` on it would have read a garbage tag.
    pub ret_boxed: bool,
}

impl FnPlan {
    /// Home of operand-stack depth `d` given its kind.
    #[inline]
    pub fn depth_home(&self, d: usize) -> u32 {
        self.stack_home[d]
    }

    /// The register holding depth `d`, if the plan homes it in one. `Dyn`
    /// depths never are — one register cannot hold a tag and a payload.
    #[inline]
    pub fn depth_reg(d: usize, k: Kind) -> Option<u8> {
        if d < REG_DEPTHS && k != Kind::Dyn {
            Some(REG_DEPTH_BASE + d as u8)
        } else {
            None
        }
    }

    /// Does this function return `(tag, payload)` in `a2:a3` rather than a
    /// single word in `a2`? True for a `Dyn` return — which has no other
    /// shape — and for every function reachable as a VALUE, see
    /// [`FnPlan::ret_boxed`].
    #[inline]
    pub fn boxed_return(&self) -> bool {
        self.ret == Kind::Dyn || self.ret_boxed
    }

    /// Where local slot `i` lives.
    #[inline]
    pub fn local(&self, i: usize) -> SlotHome {
        match self.local_home[i] {
            Some(off) => SlotHome::Frame(off),
            None => SlotHome::Reg(PARAM_BASE + i as u8),
        }
    }
}

/// Plan every function, or say why one cannot be compiled.
pub fn plan_all(prog: &Program, kinds: &Kinds) -> Result<Vec<FnPlan>, Refusal> {
    let maps = luxel_core::kinds::stack_maps(prog, kinds).map_err(|e| Refusal::Verifier {
        detail: alloc::format!("{e}"),
    })?;
    // A function named by `ConstFun` anywhere is reachable through
    // `CallValue` with arbitrary arguments, so §2.3 makes all its
    // parameters `Dyn` and it must take them through `ctx.args`. If the
    // inference ever says otherwise the two sides would disagree about
    // where the arguments are, so check rather than assume.
    let mut as_value = alloc::vec![false; prog.fns.len()];
    for f in &prog.fns {
        let s = f.code_start as usize;
        let n = f.code_len as usize;
        let mut at = 0;
        while at < n {
            let w = prog.words[s + at];
            let o = enc::opcode(w);
            if o == op::CONST_FUN {
                let i = enc::imm16(w) as usize;
                if i < as_value.len() {
                    as_value[i] = true;
                }
            }
            at += ilen(o);
        }
    }

    let mut out = Vec::with_capacity(prog.fns.len());
    for (fi, f) in prog.fns.iter().enumerate() {
        out.push(plan_fn(prog, kinds, fi, &maps[fi], as_value[fi])?);
        let _ = f;
    }
    Ok(out)
}

fn plan_fn(
    prog: &Program,
    kinds: &Kinds,
    fi: usize,
    map: &StackMap,
    as_value: bool,
) -> Result<FnPlan, Refusal> {
    let f = &prog.fns[fi];
    let fk = &kinds.fns[fi];
    let params = f.params as usize;
    let locals = f.locals as usize;
    let slot_kind: Vec<Kind> = fk.slots.clone();

    let all_simple = slot_kind.iter().take(params).all(|k| *k != Kind::Dyn);
    let mut conv = if params <= MAX_REG_PARAMS && all_simple {
        ParamConv::Regs
    } else {
        ParamConv::CtxArgs
    };
    if as_value && params > 0 {
        // Reachable as a value: `CallValue` hands arguments over through
        // `ctx.args`, so this function must read them from there.
        if conv == ParamConv::Regs {
            // §2.3 says this cannot happen; refuse rather than miscompile.
            return Err(Refusal::ParamOverflow {
                fn_idx: fi as u16,
                words: params,
            });
        }
        conv = ParamConv::CtxArgs;
    }

    // `ctx.args` offsets, one word per non-`Dyn` parameter and two per
    // `Dyn` one, in order.
    let mut arg_off = Vec::with_capacity(params);
    let mut aw = 0u32;
    for k in slot_kind.iter().take(params) {
        arg_off.push(dev32::ARGS + aw * 4);
        aw += if *k == Kind::Dyn { 2 } else { 1 };
    }
    if conv == ParamConv::CtxArgs && aw as usize > luxel_core::jit::CTX_ARGS {
        return Err(Refusal::ParamOverflow {
            fn_idx: fi as u16,
            words: aw as usize,
        });
    }

    // Frame, from the BOTTOM up: locals, then one home per operand-stack
    // depth, then the boxed-argument scratch — and the window save areas
    // reserved at the TOP, above all of it (see `WINDOW_SAVE`).
    let mut off = 0u32;
    let mut local_home = Vec::with_capacity(locals);
    for i in 0..locals {
        if conv == ParamConv::Regs && i < params {
            local_home.push(None);
        } else {
            local_home.push(Some(off));
            off += HOME;
        }
    }

    // Peak operand-stack depth. The map records the stack on ARRIVAL, and
    // the deepest an instruction can leave it is two above that (`Dup2`),
    // so two homes of slack cover every opcode without a second walk.
    let peak = map.max_depth() + 2;
    let mut stack_home = Vec::with_capacity(peak);
    for _ in 0..peak {
        stack_home.push(off);
        off += HOME;
    }

    // Boxed arguments: the widest `[Value; n]` any call or `NewArray` in
    // this function builds.
    let scratch_n = max_boxed(prog, f.code_start as usize, f.code_len as usize);
    let scratch_off = off;
    off += scratch_n as u32 * dev32::VALUE;

    // `WINDOW_SAVE` is added HERE, once, at the top: `off` is the size of
    // the data the function owns, and the frame is that plus the spill
    // area the window mechanism will use, rounded to `entry`'s 16-byte
    // granularity. A leaf with no data still reserves the full save area,
    // which is why the minimum frame is 32 and not 0.
    let frame = (off + WINDOW_SAVE).next_multiple_of(16);
    if frame > 32_760 {
        return Err(Refusal::FrameTooLarge {
            fn_idx: fi as u16,
            bytes: frame as usize,
        });
    }

    // Branch targets: basic-block starts, and which of them a BACKWARD
    // branch reaches (that is where the fuel check goes, §3.6).
    let n = f.code_len as usize;
    let mut block_start = alloc::vec![false; n + 1];
    let mut back_target = alloc::vec![false; n + 1];
    let s = f.code_start as usize;
    let mut at = 0usize;
    while at < n {
        let w = prog.words[s + at];
        let o = enc::opcode(w);
        let t = match o {
            op::JMP | op::JMP_IF_FALSE | op::JMP_IF_TRUE_PEEK | op::JMP_IF_FALSE_PEEK => {
                Some(enc::imm24(w) as usize)
            }
            op::CMP_JF => Some(prog.words[s + at + 1] as usize),
            _ => None,
        };
        if let Some(t) = t {
            if t <= n {
                block_start[t] = true;
                if t <= at {
                    back_target[t] = true;
                }
            }
        }
        // the instruction after a conditional branch also starts a block
        if t.is_some() && o != op::JMP {
            let nx = at + ilen(o);
            if nx <= n {
                block_start[nx] = true;
            }
        }
        at += ilen(o);
    }

    Ok(FnPlan {
        fn_idx: fi as u16,
        params,
        locals,
        slot_kind,
        ret: fk.ret,
        conv,
        local_home,
        arg_off,
        stack_home,
        scratch_off,
        scratch_n,
        frame,
        map: map.clone(),
        block_start,
        back_target,
        ret_boxed: as_value,
    })
}

/// The widest boxed-argument array this function builds: the argument count
/// of any `CallBuiltin*`/`CallValue`, and the element count of any
/// `NewArray`.
fn max_boxed(prog: &Program, start: usize, len: usize) -> usize {
    let mut m = 0usize;
    let mut at = 0usize;
    while at < len {
        let w = prog.words[start + at];
        let o = enc::opcode(w);
        let n = match o {
            op::CALL_BUILTIN | op::CALL_BUILTIN_C | op::CALL_BUILTIN_CC => enc::argc(w) as usize,
            op::CALL_VALUE => enc::imm8(w) as usize,
            op::NEW_ARRAY => enc::imm16(w) as usize,
            _ => 0,
        };
        if n > m {
            m = n;
        }
        at += ilen(o);
    }
    m
}
