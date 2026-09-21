//! The compiler: LXBC v6 words in, Xtensa LX7 bytes out
//! (docs/jit-design.md §3.5–§3.7).
//!
//! One pass per function over the word stream, with the abstract operand
//! stack taken from `luxel_core::kinds::stack_maps` — the VERIFIER'S OWN
//! walk, not a second implementation of §2.4's rules. At every word the
//! emitter therefore knows the static depth and the kind of every operand,
//! which is what makes the unboxed representation legal.
//!
//! # Buffer layout
//!
//! `[literal pool][fn 0][fn 1]…`, 4-aligned, exactly §3.7. The pool goes
//! first because `l32r` reaches BACKWARDS ONLY. Its size is not known until
//! the last function is emitted, so code is emitted into its own buffer at
//! code-local offsets and the pool is prepended afterwards: branches and
//! `j` are pc-relative and survive a uniform shift untouched, and every
//! `l32r` is a fixup anyway.
//!
//! # Register discipline
//!
//! `a2` is the context for the whole function. `a3…a7` hold the parameters
//! under [`ParamConv::Regs`]. `a8…a11` hold operand-stack depths 0–3 WITHIN
//! ONE BASIC BLOCK; every register-homed depth is spilled to its frame home
//! before any branch, at every branch target and before every `callx8`, so
//! no two edges into a block can disagree about what is where. `a12…a15`
//! are scratch inside one bytecode instruction, and `a8`/`a9` double as
//! scratch while a call's arguments are being marshalled into `a10…a15`.
//!
//! # What is NOT here
//!
//! Nothing executes. [`compile`] is a pure function, which is what lets
//! `tests/library_diff.rs` run its output through an ISA model on x86 and
//! compare against the interpreter, bit for bit.

use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec::Vec;

use luxel_core::bytecode::{enc, is_binop_sub, op};
use luxel_core::jit::{dev32, DirectSig, BUILTIN_ENTRIES, RET_NUM, STATUS_ERR};
use luxel_core::kinds::{builtin_sig, elem_kind, ilen, Kind, Kinds, SigRet};
use luxel_core::vm::{Program, TAG_ARR, TAG_BUILTIN, TAG_FUN, TAG_NUM};

use crate::plan::{
    payload_at, plan_all, tag_at, FnPlan, ParamConv, SlotHome, MAX_REG_PARAMS,
    REG_DEPTHS, REG_DEPTH_BASE, SCRATCH,
};
use crate::xtensa::{Asm, Cond, OutOfReach, Reg, ZCond, A1, A10, A2, A8, A9};
use crate::{Env, FnAbi, NativeImage, Refusal};



/// `Fx::ONE` as a raw word — what a comparison pushes for "true", and the
/// divisor that turns a 16.16 word into its truncated integer part.
const ONE: i32 = 1 << 16;

/// Compile a whole program, or refuse it (docs/jit-design.md §4a).
///
/// A refusal is whole-program and never a panic: the caller runs the
/// interpreter, at the interpreter's speed, with the same pixels.
pub fn compile(prog: &Program, kinds: &Kinds, env: &Env) -> Result<NativeImage, Refusal> {
    if env.code_base % 4 != 0 {
        return Err(Refusal::Verifier {
            detail: "code_base must be 4-aligned".to_string(),
        });
    }
    // **`retw` restores only 30 bits of return address** — `PC ←
    // PC[31:30] || a0[29:0]` — so a `callx8` to a target in a different
    // gigabyte returns into the wrong one. Found by the ISA model, and it
    // binds every helper address phase 3 supplies as much as it binds this
    // test harness. On the S3 the JIT's IBUS window, the flash mapping and
    // IRAM are all in `0x4…`, so this only ever fires on a wiring mistake.
    let region = env.code_base >> 30;
    let h = &env.helpers;
    for (name, addr) in [
        ("fx_div", h.fx_div),
        ("fx_pow", h.fx_pow),
        ("arr_load_num", h.arr_load_num),
        ("arr_load_dyn", h.arr_load_dyn),
        ("arr_store", h.arr_store),
        ("arr_len", h.arr_len),
        ("new_array", h.new_array),
        ("const_arr", h.const_arr),
        ("call_value", h.call_value),
        ("assert_fail", h.assert_fail),
        ("bail_fuel", h.bail_fuel),
        ("bail_depth", h.bail_depth),
    ] {
        if addr >> 30 != region {
            return Err(Refusal::AddressRegion { name, addr });
        }
    }
    let plans = plan_all(prog, kinds)?;
    let mut e = Emitter {
        prog,
        kinds,
        env,
        code: Asm::new(),
        pool: Pool::default(),
        l32r_fix: Vec::new(),
        entries: Vec::with_capacity(prog.fns.len()),
        abi: Vec::with_capacity(prog.fns.len()),
        plans: &plans,
        f: Frame::empty(),
    };

    for fi in 0..prog.fns.len() {
        e.function(fi, &plans[fi])?;
    }
    e.finish()
}

// ------------------------------------------------------------ literal pool

/// A key in the literal pool. Function addresses are only known once every
/// function has been laid out, so they are interned by index and filled in
/// at the end (§3.1's "forward references are patched from a fixup list").
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Lit {
    Word(u32),
    FnAddr(u16),
}

#[derive(Default)]
struct Pool {
    idx: BTreeMap<Lit, usize>,
    keys: Vec<Lit>,
}

impl Pool {
    fn intern(&mut self, l: Lit) -> usize {
        if let Some(&i) = self.idx.get(&l) {
            return i;
        }
        let i = self.keys.len();
        self.keys.push(l);
        self.idx.insert(l, i);
        i
    }
}

// ------------------------------------------------------------ per-function

struct Frame {
    fn_idx: u16,
    /// Code-local offset of each word's first instruction.
    word_off: Vec<Option<usize>>,
    /// Forward `j` sites waiting for a word target. Every forward
    /// conditional is `b<inverted> +6; j target` (§3.7), so a `j` is the
    /// only thing ever patched against a word index.
    fix: Vec<(usize, u32)>,
    /// `j` sites aimed at this function's bail epilogue.
    bail_fix: Vec<usize>,
    /// The emitter's running copy of the abstract stack. Re-seeded from the
    /// verifier's map at every word so it cannot drift; the copy exists so
    /// a fused superinstruction can push and pop its parts.
    stack: Vec<Kind>,
    /// Which register-homed depths hold a live value right now.
    in_reg: [bool; REG_DEPTHS],
    /// Scratch cursor, reset per bytecode instruction.
    sc: usize,
    /// One instruction asked for more scratch than the pool holds.
    sc_overflow: bool,
}

impl Frame {
    fn empty() -> Frame {
        Frame {
            fn_idx: 0,
            word_off: Vec::new(),
            fix: Vec::new(),
            bail_fix: Vec::new(),
            stack: Vec::new(),
            in_reg: [false; REG_DEPTHS],
            sc: 0,
            sc_overflow: false,
        }
    }
}

struct Emitter<'a> {
    prog: &'a Program,
    kinds: &'a Kinds,
    env: &'a Env,
    code: Asm,
    pool: Pool,
    /// `(code-local site, literal index)`.
    l32r_fix: Vec<(usize, usize)>,
    entries: Vec<u32>,
    abi: Vec<FnAbi>,
    /// Every function's plan: a `CallFn` has to know the CALLEE's
    /// convention and return shape, not just its own.
    plans: &'a [FnPlan],
    f: Frame,
}

/// The two-register branch that tests a bytecode comparison, and whether
/// its operands go to the branch in the other order — there is no
/// `ble`/`bgt` on Xtensa, so `a <= b` is `bge b, a`.
fn cmp_branch(sub: u8) -> (Cond, bool) {
    match sub {
        op::LT => (Cond::Lt, false),
        op::GE => (Cond::Ge, false),
        op::LE => (Cond::Ge, true),
        op::GT => (Cond::Lt, true),
        op::EQ => (Cond::Eq, false),
        _ => (Cond::Ne, false),
    }
}

/// The `Value` tag a non-`Dyn` kind boxes to (§3.5's `Box` row).
fn tag_of(k: Kind) -> u32 {
    match k {
        Kind::Arr | Kind::ArrNum => TAG_ARR,
        Kind::Fun => TAG_FUN,
        Kind::Builtin => TAG_BUILTIN,
        _ => TAG_NUM,
    }
}

impl<'a> Emitter<'a> {
    // -------------------------------------------------------- primitives

    /// Next scratch register. Scratch is live only inside one bytecode
    /// instruction; the counter resets per word.
    /// Next scratch register — or, if this instruction has somehow asked
    /// for more than the pool holds, a wrapped one plus a flag.
    ///
    /// The flag is checked after every bytecode instruction and turns into
    /// [`Refusal::ScratchExhausted`]. A panic here would be an emitter bug
    /// crashing the render task on a board with no serial port; a refusal
    /// is the same information delivered as "this pattern runs in the
    /// interpreter", which is always a safe answer.
    fn sc(&mut self) -> Reg {
        if self.f.sc >= SCRATCH.len() {
            self.f.sc_overflow = true;
        }
        let r = SCRATCH[self.f.sc % SCRATCH.len()];
        self.f.sc += 1;
        r
    }

    /// Begin a new atomic step, releasing every scratch register.
    ///
    /// A superinstruction is the CONCATENATION of its parts (§3.5 "Fusion
    /// is transparent"), and each part commits its result to a register
    /// home or a frame home before the next one starts — so the scratch
    /// pool belongs to one part, not to the whole fused word. Without this
    /// a three-part fusion like `LoadG g; Const c; <op>` runs the pool dry
    /// on its own.
    fn step(&mut self) {
        self.f.sc = 0;
    }

    /// Release the `n` most recently taken scratch registers. Used where a
    /// temporary dies immediately (an unbox's tag test, a wide store's
    /// address), so a long sequence stays inside the four.
    fn sc_free(&mut self, n: usize) {
        self.f.sc -= n;
    }

    /// `l32r r, <literal>`, recorded for patching once the pool's position
    /// is known.
    fn load_lit(&mut self, r: Reg, l: Lit) {
        let i = self.pool.intern(l);
        let site = self.code.l32r_placeholder(r);
        self.l32r_fix.push((site, i));
    }

    /// Materialise a 32-bit constant: `movi`/`movi.n` when it fits, a pool
    /// literal otherwise (§3.5's `ConstNum` row).
    fn imm(&mut self, r: Reg, v: i32) {
        if (-2048..=2047).contains(&v) {
            let ok = self.code.movi_best(r, v);
            debug_assert!(ok);
        } else {
            self.load_lit(r, Lit::Word(v as u32));
        }
    }

    /// `l32i dst, base, off` for any reachable `off`. Over `l32i`'s 1020 —
    /// a big frame, or a program with many globals — one `addmi` gets
    /// within reach, and `dst` doubles as the address register because it
    /// is never `a1` or `a2`.
    fn load_at(&mut self, dst: Reg, base: Reg, off: u32) -> Result<(), Refusal> {
        if self.code.load(dst, base, off) {
            return Ok(());
        }
        if off >= 32_768 {
            return Err(self.offset_reach(off));
        }
        debug_assert!(dst != base);
        let ok = self.code.addmi(dst, base, (off & !0xff) as i32);
        debug_assert!(ok);
        let ok = self.code.load(dst, dst, off & 0xff);
        debug_assert!(ok);
        Ok(())
    }

    /// `s32i src, base, off` for any reachable `off`. A store has no
    /// destination register to borrow, so the wide form takes a scratch and
    /// gives it straight back.
    fn store_at(&mut self, src: Reg, base: Reg, off: u32) -> Result<(), Refusal> {
        if self.code.store(src, base, off) {
            return Ok(());
        }
        if off >= 32_768 {
            return Err(self.offset_reach(off));
        }
        let t = self.sc();
        let ok = self.code.addmi(t, base, (off & !0xff) as i32);
        debug_assert!(ok);
        let ok = self.code.store(src, t, off & 0xff);
        debug_assert!(ok);
        self.sc_free(1);
        Ok(())
    }

    /// A load whose address scratch must not come from the general pool,
    /// and may not be `dst` — reaching `ctx.builtins[id]` loads through the
    /// same register it computes the address in, and builtin 85 and up sit
    /// past `l32i`'s 1020.
    fn load_at_with(&mut self, dst: Reg, base: Reg, off: u32, t: Reg) -> Result<(), Refusal> {
        if self.code.load(dst, base, off) {
            return Ok(());
        }
        if off >= 32_768 {
            return Err(self.offset_reach(off));
        }
        let ok = self.code.addmi(t, base, (off & !0xff) as i32);
        debug_assert!(ok);
        let ok = self.code.load(dst, t, off & 0xff);
        debug_assert!(ok);
        Ok(())
    }

    /// A store whose scratch must not come from the general pool — used
    /// while a call's arguments occupy `a10…a15`.
    fn store_at_with(&mut self, src: Reg, base: Reg, off: u32, t: Reg) -> Result<(), Refusal> {
        if self.code.store(src, base, off) {
            return Ok(());
        }
        if off >= 32_768 {
            return Err(self.offset_reach(off));
        }
        let ok = self.code.addmi(t, base, (off & !0xff) as i32);
        debug_assert!(ok);
        let ok = self.code.store(src, t, off & 0xff);
        debug_assert!(ok);
        Ok(())
    }

    /// `dst = base + off` — the address of a frame slot, for the boxed
    /// argument pointer a generic builtin call takes.
    fn addr_of(&mut self, dst: Reg, base: Reg, off: u32) -> Result<(), Refusal> {
        if self.code.addi(dst, base, off as i32) {
            return Ok(());
        }
        // `addmi` moves a multiple of 256 and `addi` the rest, but `addi`'s
        // range is −128..=127, not 0..=255 — so the multiple is ROUNDED TO
        // NEAREST and the remainder may be negative.
        let hi = (off + 128) & !0xff;
        if hi > 32_512 {
            return Err(self.offset_reach(off));
        }
        let lo = off as i32 - hi as i32;
        let ok = self.code.addmi(dst, base, hi as i32);
        debug_assert!(ok);
        if lo != 0 {
            let ok = self.code.addi(dst, dst, lo);
            debug_assert!(ok);
        }
        Ok(())
    }

    fn offset_reach(&self, off: u32) -> Refusal {
        Refusal::OffsetReach {
            fn_idx: self.f.fn_idx,
            off,
        }
    }

    fn reach(&self, e: OutOfReach) -> Refusal {
        Refusal::JumpReach {
            fn_idx: self.f.fn_idx,
            word: 0,
            distance: e.distance.unsigned_abs(),
        }
    }

    // ------------------------------------------------- operand-stack access

    fn kind(&self, d: usize) -> Kind {
        self.f.stack.get(d).copied().unwrap_or(Kind::Num)
    }

    /// Bring depth `d`'s payload word into a register.
    fn rd(&mut self, plan: &FnPlan, d: usize) -> Result<Reg, Refusal> {
        let k = self.kind(d);
        if let Some(r) = FnPlan::depth_reg(d, k) {
            if self.f.in_reg[d] {
                return Ok(r);
            }
            self.load_at(r, A1, plan.depth_home(d))?;
            self.f.in_reg[d] = true;
            return Ok(r);
        }
        let r = self.sc();
        self.load_at(r, A1, payload_at(plan.depth_home(d), k))?;
        Ok(r)
    }

    /// Depth `d`'s tag word. A non-`Dyn` slot stores no tag — its kind IS
    /// the tag — so one is materialised.
    fn rd_tag(&mut self, plan: &FnPlan, d: usize) -> Result<Reg, Refusal> {
        let k = self.kind(d);
        let r = self.sc();
        if k == Kind::Dyn {
            self.load_at(r, A1, tag_at(plan.depth_home(d)))?;
        } else {
            self.imm(r, tag_of(k) as i32);
        }
        Ok(r)
    }

    /// Depth `d` coerced to a raw number the way `Value::num` does: a `Dyn`
    /// slot keeps its payload only when its tag says `Num`, and a slot
    /// statically known to hold a reference is simply 0 (§3.5's
    /// "arithmetic on a `Dyn` operand" row — `Value::num` is oracle-pinned
    /// to treat references as zero).
    fn rd_num(&mut self, plan: &FnPlan, d: usize) -> Result<Reg, Refusal> {
        match self.kind(d) {
            Kind::Num => self.rd(plan, d),
            Kind::Dyn => {
                // A `Dyn` depth is never register-homed, so `pay` is a
                // scratch and safe to overwrite.
                let pay = self.rd(plan, d)?;
                let tag = self.sc();
                self.load_at(tag, A1, tag_at(plan.depth_home(d)))?;
                let site = self.code.branch_z_forward(ZCond::Eqz, tag);
                self.imm(pay, 0);
                let here = self.code.here();
                self.code
                    .patch_branch_z(site, here)
                    .map_err(|e| self.reach(e))?;
                self.sc_free(1);
                Ok(pay)
            }
            _ => {
                let r = self.sc();
                self.imm(r, 0);
                Ok(r)
            }
        }
    }

    /// The register a non-`Dyn` value destined for depth `d` may be
    /// computed into.
    fn dest(&mut self, d: usize, k: Kind) -> Reg {
        match FnPlan::depth_reg(d, k) {
            Some(r) => r,
            None => self.sc(),
        }
    }

    /// Record `r` as depth `d`'s non-`Dyn` value.
    fn put(&mut self, plan: &FnPlan, d: usize, k: Kind, r: Reg) -> Result<(), Refusal> {
        match FnPlan::depth_reg(d, k) {
            Some(home) => {
                if home != r {
                    self.code.mov_n(home, r);
                }
                self.f.in_reg[d] = true;
            }
            None => self.store_at(r, A1, payload_at(plan.depth_home(d), k))?,
        }
        Ok(())
    }

    /// Record a `(tag, payload)` pair as depth `d`'s `Dyn` value.
    fn put_dyn(&mut self, plan: &FnPlan, d: usize, tag: Reg, pay: Reg) -> Result<(), Refusal> {
        let h = plan.depth_home(d);
        self.store_at(tag, A1, tag_at(h))?;
        self.store_at(pay, A1, h + dev32::VALUE_PAYLOAD)?;
        Ok(())
    }

    /// Push a kind onto the emitter's copy of the abstract stack.
    fn push(&mut self, k: Kind) {
        let d = self.f.stack.len();
        if d < REG_DEPTHS {
            // whoever pushed it will call `put`, which sets this
            self.f.in_reg[d] = false;
        }
        self.f.stack.push(k);
    }

    fn pop(&mut self) {
        let d = self.f.stack.len() - 1;
        if d < REG_DEPTHS {
            self.f.in_reg[d] = false;
        }
        self.f.stack.pop();
    }

    /// Drop every depth at or above `n`, forgetting any register homes
    /// they held — the bulk `pop` a call with several arguments does.
    fn truncate(&mut self, n: usize) {
        for d in n..REG_DEPTHS {
            self.f.in_reg[d] = false;
        }
        self.f.stack.truncate(n);
    }

    /// A base register and a NARROW offset for global `g`. Globals are
    /// `Vm::globals` — since #642 a `#[repr(C)]` array of `Value`s, eight
    /// bytes each, tag at +0 and payload at +4 — reached through
    /// `ctx.globals` (§3.8). Folding the wide part into the base here
    /// keeps every access a plain `l32i`/`s32i` and costs no scratch.
    fn global_base(&mut self, g: usize) -> Result<(Reg, u32), Refusal> {
        let gb = self.sc();
        let ok = self.code.l32i(gb, A2, dev32::GLOBALS);
        debug_assert!(ok);
        let mut off = g as u32 * dev32::VALUE;
        if off + dev32::VALUE_PAYLOAD > 1020 {
            if off >= 32_768 {
                return Err(self.offset_reach(off));
            }
            let ok = self.code.addmi(gb, gb, (off & !0xff) as i32);
            debug_assert!(ok);
            off &= 0xff;
        }
        Ok((gb, off))
    }

    /// Spill every register-homed depth to its frame home. Done before any
    /// `callx8` (which clobbers `a8…a15`), before every branch and at every
    /// branch target — which is what keeps register homing purely
    /// intra-block, and so free of any cross-edge agreement problem (§3.4).
    fn spill_all(&mut self, plan: &FnPlan) -> Result<(), Refusal> {
        for d in 0..REG_DEPTHS.min(self.f.stack.len()) {
            if self.f.in_reg[d] {
                let r = REG_DEPTH_BASE + d as u8;
                let k = self.kind(d);
                self.store_at(r, A1, payload_at(plan.depth_home(d), k))?;
            }
        }
        self.f.in_reg = [false; REG_DEPTHS];
        Ok(())
    }

    // ------------------------------------------- call-argument marshalling
    //
    // While a call's arguments are being built, `a10…a15` are the argument
    // registers and the general scratch pool overlaps them. Everything in
    // this section therefore clobbers only its destination plus `a8`/`a9`,
    // which are dead because `spill_all` ran first.

    /// Load depth `d`'s payload word into `dst`.
    fn arg_pay(&mut self, plan: &FnPlan, dst: Reg, d: usize) -> Result<(), Refusal> {
        let k = self.kind(d);
        self.load_at(dst, A1, payload_at(plan.depth_home(d), k))
    }

    /// Load depth `d`'s tag word into `dst`.
    fn arg_tag(&mut self, plan: &FnPlan, dst: Reg, d: usize) -> Result<(), Refusal> {
        let k = self.kind(d);
        if k == Kind::Dyn {
            self.load_at(dst, A1, tag_at(plan.depth_home(d)))
        } else {
            self.imm(dst, tag_of(k) as i32);
            Ok(())
        }
    }

    /// Load depth `d` into `dst`, coerced by `Value::num`.
    fn arg_num(&mut self, plan: &FnPlan, dst: Reg, d: usize) -> Result<(), Refusal> {
        match self.kind(d) {
            Kind::Num => self.arg_pay(plan, dst, d),
            Kind::Dyn => {
                self.arg_pay(plan, dst, d)?;
                self.load_at(A8, A1, tag_at(plan.depth_home(d)))?;
                let site = self.code.branch_z_forward(ZCond::Eqz, A8);
                self.imm(dst, 0);
                let here = self.code.here();
                self.code
                    .patch_branch_z(site, here)
                    .map_err(|e| self.reach(e))
            }
            _ => {
                self.imm(dst, 0);
                Ok(())
            }
        }
    }

    /// Box depth `d` into the frame's boxed-argument scratch at index `j`
    /// (§3.3) — tag word then payload word, i.e. a `Value`.
    fn box_arg(&mut self, plan: &FnPlan, j: usize, d: usize) -> Result<(), Refusal> {
        let off = plan.scratch_off + j as u32 * dev32::VALUE;
        self.arg_tag(plan, A8, d)?;
        self.store_at_with(A8, A1, off + dev32::VALUE_TAG, A9)?;
        self.arg_pay(plan, A8, d)?;
        self.store_at_with(A8, A1, off + dev32::VALUE_PAYLOAD, A9)
    }

    /// Box a compile-time `Num` immediate into the scratch — the trailing
    /// constants of `CallBuiltinC`/`CC`.
    fn box_const(&mut self, plan: &FnPlan, j: usize, v: i32) -> Result<(), Refusal> {
        let off = plan.scratch_off + j as u32 * dev32::VALUE;
        self.imm(A8, TAG_NUM as i32);
        self.store_at_with(A8, A1, off + dev32::VALUE_TAG, A9)?;
        self.imm(A8, v);
        self.store_at_with(A8, A1, off + dev32::VALUE_PAYLOAD, A9)
    }

    // ---------------------------------------------------------- branching

    /// A conditional branch to a bytecode word. Backward targets are
    /// encoded directly when they are in reach; everything else is
    /// `b<inverted> +6; j target` with a fixup (§3.7).
    fn branch_word(
        &mut self,
        cond: Cond,
        s: Reg,
        t: Reg,
        target: u32,
        plan: &FnPlan,
    ) -> Result<(), Refusal> {
        self.spill_all(plan)?;
        if let Some(off) = self.f.word_off.get(target as usize).copied().flatten() {
            if self.code.branch(cond, s, t, off).is_ok() {
                return Ok(());
            }
        }
        let here = self.code.here();
        self.code
            .branch(cond.invert(), s, t, here + 6)
            .map_err(|e| self.reach(e))?;
        let site = self.code.j_forward();
        self.f.fix.push((site, target));
        Ok(())
    }

    /// The same for `beqz`/`bnez`.
    fn branch_z_word(
        &mut self,
        cond: ZCond,
        s: Reg,
        target: u32,
        plan: &FnPlan,
    ) -> Result<(), Refusal> {
        self.spill_all(plan)?;
        if let Some(off) = self.f.word_off.get(target as usize).copied().flatten() {
            if self.code.branch_z(cond, s, off).is_ok() {
                return Ok(());
            }
        }
        let here = self.code.here();
        self.code
            .branch_z(cond.invert(), s, here + 6)
            .map_err(|e| self.reach(e))?;
        let site = self.code.j_forward();
        self.f.fix.push((site, target));
        Ok(())
    }

    /// An unconditional jump to a bytecode word.
    fn jump_word(&mut self, target: u32, plan: &FnPlan) -> Result<(), Refusal> {
        self.spill_all(plan)?;
        if let Some(off) = self.f.word_off.get(target as usize).copied().flatten() {
            return self.code.j(off).map_err(|e| self.reach(e));
        }
        let site = self.code.j_forward();
        self.f.fix.push((site, target));
        Ok(())
    }

    /// `j` to this function's bail epilogue, which is emitted last.
    fn goto_bail(&mut self) {
        let site = self.code.j_forward();
        self.f.bail_fix.push(site);
    }

    /// The status check after every fallible call (§3.6). Uses `a8`, which
    /// is dead across a call.
    fn bail_if_status(&mut self) -> Result<(), Refusal> {
        let ok = self.code.l32i(A8, A2, dev32::STATUS);
        debug_assert!(ok);
        let here = self.code.here();
        self.code
            .branch_z(ZCond::Eqz, A8, here + 6)
            .map_err(|e| self.reach(e))?;
        self.goto_bail();
        Ok(())
    }

    /// Store the site of the instruction about to run, so a failing helper
    /// attributes its error exactly as the interpreter does (§3.6).
    ///
    /// **Both words.** §3.6 says "the emitter stores the instruction's
    /// fn-relative word index (2 instructions)", but a returning `CallFn`
    /// has left `ctx.fn_idx` set to the CALLEE's, so the function index has
    /// to be restored too or an error after a call is attributed to the
    /// wrong function.
    fn site(&mut self, at: usize) {
        self.imm(A8, at as i32);
        let ok = self.code.s32i(A8, A2, dev32::INSN_AT);
        debug_assert!(ok);
        let fi = self.f.fn_idx as i32;
        self.imm(A8, fi);
        let ok = self.code.s32i(A8, A2, dev32::FN_IDX);
        debug_assert!(ok);
    }

    /// `mov.n a10, a2; l32r a8, helper; callx8 a8` for a helper that takes
    /// only the context, then `j bail`. The out-of-line half of the fuel
    /// and depth guards.
    fn bail_via(&mut self, addr: u32) {
        self.code.mov_n(A10, A2);
        self.load_lit(A8, Lit::Word(addr));
        self.code.callx8(A8);
        self.goto_bail();
    }

    /// The back-edge fuel check of §3.6. `a8` is free here: a fuel check
    /// only ever sits at a branch target, where every register-homed depth
    /// has been spilled.
    fn fuel(&mut self) -> Result<(), Refusal> {
        let ok = self.code.l32i(A8, A2, dev32::FUEL);
        debug_assert!(ok);
        let ok = self.code.addi_n(A8, A8, -1);
        debug_assert!(ok);
        let ok = self.code.s32i(A8, A2, dev32::FUEL);
        debug_assert!(ok);
        let site = self.code.branch_z_forward(ZCond::Nez, A8);
        self.bail_via(self.env.helpers.bail_fuel);
        let here = self.code.here();
        self.code
            .patch_branch_z(site, here)
            .map_err(|e| self.reach(e))
    }
}

// ============================================================ the function

impl<'a> Emitter<'a> {
    fn function(&mut self, fi: usize, plan: &FnPlan) -> Result<(), Refusal> {
        // `l32r` literals, `callx8` targets and `entry` all want 4-aligned
        // addresses (§3.7).
        self.code.align4();
        let start = self.code.here();
        self.entries.push(start as u32);
        self.abi.push(FnAbi {
            frame: plan.frame,
            args_in_regs: plan.conv == ParamConv::Regs,
            params: plan.params as u8,
            ret_dyn: plan.boxed_return(),
        });

        let f = &self.prog.fns[fi];
        let base = f.code_start as usize;
        let n = f.code_len as usize;
        self.f = Frame {
            fn_idx: fi as u16,
            word_off: alloc::vec![None; n + 1],
            fix: Vec::new(),
            bail_fix: Vec::new(),
            stack: Vec::new(),
            in_reg: [false; REG_DEPTHS],
            sc: 0,
            sc_overflow: false,
        };

        self.prologue(plan)?;


        let mut at = 0usize;
        while at < n {
            self.f.sc = 0;
            let w = self.prog.words[base + at];
            let o = enc::opcode(w);
            let Some(st) = plan.map[at].clone() else {
                // Unreachable: a dead epilogue after an explicit `return`.
                // Nothing branches here (a branch target is reachable by
                // definition), so nothing has to be emitted.
                at += ilen(o);
                continue;
            };
            if plan.block_start[at] {
                // The FALL-THROUGH edge into a branch target owes a spill:
                // a branch arriving here already spilled (§3.4), but the
                // instruction before may have left a value in a register
                // home. It goes BEFORE the label, so a branch landing here
                // does not re-execute it — and `spill_all` emits nothing
                // when nothing is in a register, which is exactly the
                // case where control arrived by a branch.
                self.f.stack = st.clone();
                // `spill_all` also clears `in_reg`, which is the other
                // half of what a block boundary means.
                self.spill_all(plan)?;
                self.f.sc = 0;
            }
            self.f.word_off[at] = Some(self.code.here());
            self.f.stack = st;
            if plan.back_target[at] {
                self.fuel()?;
            }
            self.insn(plan, at, base, w, o)?;
            if self.f.sc_overflow {
                return Err(Refusal::ScratchExhausted {
                    fn_idx: plan.fn_idx,
                    word: at as u32,
                    op: o,
                });
            }

            at += ilen(o);
        }
        // Falling off the end is the implicit `RetNull`.
        self.f.word_off[n] = Some(self.code.here());
        self.f.stack.clear();
        self.ret_null(plan)?;


        self.epilogue(plan)?;
        self.patch_function()
    }

    fn prologue(&mut self, plan: &FnPlan) -> Result<(), Refusal> {
        if !self.code.entry(A1, plan.frame) {
            return Err(Refusal::FrameTooLarge {
                fn_idx: plan.fn_idx,
                bytes: plan.frame as usize,
            });
        }
        // Call-depth guard (§3.6): the native stack replaces MAX_DEPTH.
        let ok = self.code.l32i(A8, A2, dev32::STACK_LIMIT);
        debug_assert!(ok);
        let site = self.code.branch_forward(Cond::Geu, A1, A8);
        self.bail_via(self.env.helpers.bail_depth);
        let here = self.code.here();
        self.code
            .patch_branch(site, here)
            .map_err(|e| self.reach(e))?;

        // Parameters handed over through `ctx.args` are copied into frame
        // homes before anything else runs, which is what keeps recursion
        // safe — the handoff area is live only across the call (§3.2).
        if plan.conv == ParamConv::CtxArgs {
            for i in 0..plan.params {
                let k = plan.slot_kind[i];
                let src = plan.arg_off[i];
                let Some(home) = plan.local_home[i] else {
                    continue;
                };
                if k == Kind::Dyn {
                    self.load_at(A8, A2, src)?;
                    self.store_at(A8, A1, tag_at(home))?;
                    self.load_at(A8, A2, src + 4)?;
                    self.store_at(A8, A1, home + dev32::VALUE_PAYLOAD)?;
                } else {
                    self.load_at(A8, A2, src)?;
                    self.store_at(A8, A1, home)?;
                }
                self.f.sc = 0;
            }
        }

        // Locals past the parameters take the interpreter's default,
        // `Value::Num(0)` — which is two zero words, so one zero register
        // covers both halves of a `Dyn` home.
        let mut zeroed = false;
        for i in plan.params..plan.locals {
            let Some(home) = plan.local_home[i] else {
                continue;
            };
            if !zeroed {
                self.imm(A8, 0);
                zeroed = true;
            }
            self.store_at(A8, A1, home)?;
            if plan.slot_kind[i] == Kind::Dyn {
                self.store_at(A8, A1, home + dev32::VALUE_PAYLOAD)?;
            }
            self.f.sc = 0;
        }
        Ok(())
    }

    /// The one bail epilogue per function (§3.6): set the status and
    /// return. The error itself is already in `*ctx.err`.
    fn epilogue(&mut self, _plan: &FnPlan) -> Result<(), Refusal> {
        if self.f.bail_fix.is_empty() {
            return Ok(());
        }
        let here = self.code.here();
        let fixes = core::mem::take(&mut self.f.bail_fix);
        for site in fixes {
            self.code.patch_j(site, here).map_err(|e| self.reach(e))?;
        }
        self.imm(A8, STATUS_ERR);
        let ok = self.code.s32i(A8, A2, dev32::STATUS);
        debug_assert!(ok);
        self.code.retw_n();
        Ok(())
    }

    fn patch_function(&mut self) -> Result<(), Refusal> {
        let fixes = core::mem::take(&mut self.f.fix);
        for (site, target) in fixes {
            let Some(off) = self.f.word_off.get(target as usize).copied().flatten() else {
                return Err(Refusal::JumpReach {
                    fn_idx: self.f.fn_idx,
                    word: target,
                    distance: 0,
                });
            };
            self.code.patch_j(site, off).map_err(|e| self.reach(e))?;
        }
        Ok(())
    }
}

// =================================================== instruction selection

impl<'a> Emitter<'a> {
    #[allow(clippy::too_many_lines)]
    fn insn(
        &mut self,
        plan: &FnPlan,
        at: usize,
        base: usize,
        w: u32,
        o: u8,
    ) -> Result<(), Refusal> {


        match o {
            op::CONST_NUM => {
                let v = self.prog.words[base + at + 1] as i32;
                self.push_const(plan, v)
            }
            op::CONST_FUN => self.push_handle(plan, Kind::Fun, enc::imm16(w) as i32),
            op::CONST_BUILTIN => self.push_handle(plan, Kind::Builtin, enc::imm16(w) as i32),
            op::LOAD_G => self.push_global(plan, enc::imm16(w) as usize),
            op::STORE_G => self.store_global(plan, enc::imm16(w) as usize, false),
            op::STORE_G_POP => self.store_global(plan, enc::imm16(w) as usize, true),
            op::LOAD_L => self.push_local(plan, enc::imm8(w) as usize),
            op::STORE_L => self.store_local(plan, enc::imm8(w) as usize, false),
            op::STORE_L_POP => self.store_local(plan, enc::imm8(w) as usize, true),
            op::LOAD_LL => {
                self.push_local(plan, enc::imm8(w) as usize)?;
                self.push_local(plan, enc::imm8b(w) as usize)
            }
            op::LOAD_LG => {
                self.push_local(plan, enc::imm8(w) as usize)?;
                self.push_global(plan, enc::imm16hi(w) as usize)
            }
            op::LOAD_GL => {
                self.push_global(plan, enc::imm16(w) as usize)?;
                self.push_local(plan, enc::argc(w) as usize)
            }
            op::LOAD_IDX => self.load_idx(plan, at),
            op::LOAD_L_IDX => {
                self.push_local(plan, enc::imm8(w) as usize)?;
                self.load_idx(plan, at)
            }
            op::LOAD_G_L_IDX => {
                self.push_global(plan, enc::imm16(w) as usize)?;
                self.push_local(plan, enc::argc(w) as usize)?;
                self.load_idx(plan, at)
            }
            op::STORE_IDX => self.store_idx(plan, at),
            op::ARR_LEN => self.arr_len(plan, at),
            op::NEW_ARRAY => self.new_array(plan, at, enc::imm16(w) as usize),
            op::CONST_ARR => self.const_arr(plan, at, enc::imm16(w) as u32),
            op::ASSERT => self.assert(plan, at, enc::imm16(w) as u32),
            op::DUP => {
                let d = self.f.stack.len();
                self.copy_slot(plan, d - 1, d)
            }
            op::DUP2 => {
                let d = self.f.stack.len();
                self.copy_slot(plan, d - 2, d)?;
                self.copy_slot(plan, d - 1, d + 1)
            }
            op::POP => {
                self.pop();
                Ok(())
            }
            op::NEG => {
                let d = self.f.stack.len() - 1;
                let a = self.rd_num(plan, d)?;
                let r = self.dest(d, Kind::Num);
                self.code.neg(r, a);
                self.pop();
                self.push(Kind::Num);
                self.put(plan, d, Kind::Num, r)
            }
            op::NOT => self.logical_not(plan),
            op::BIT_NOT => {
                let d = self.f.stack.len() - 1;
                let a = self.rd_num(plan, d)?;
                let t = self.sc();
                let r = self.dest(d, Kind::Num);
                // `!x & !0xFFFF` — the one bitwise op that zeros the low
                // 16 bits of its result (`fixed.rs` `impl Not`).
                self.imm(t, -1);
                self.code.xor(r, a, t);
                let ok = self.code.srli(r, r, 15);
                debug_assert!(ok);
                let ok = self.code.srli(r, r, 1);
                debug_assert!(ok);
                let ok = self.code.slli(r, r, 16);
                debug_assert!(ok);
                self.pop();
                self.push(Kind::Num);
                self.put(plan, d, Kind::Num, r)
            }
            op::BOX => self.box_top(plan),
            op::CONST_OP => {
                let sub = enc::imm8(w);
                let c = self.prog.words[base + at + 1] as i32;
                self.push_const(plan, c)?;
                self.binop(plan, sub, at)
            }
            op::LOAD_L_CONST_OP => {
                let sub = enc::imm8b(w);
                let c = self.prog.words[base + at + 1] as i32;
                self.push_local(plan, enc::imm8(w) as usize)?;
                self.push_const(plan, c)?;
                self.binop(plan, sub, at)
            }
            op::LOAD_G_CONST_OP => {
                let sub = enc::argc(w);
                let c = self.prog.words[base + at + 1] as i32;
                self.push_global(plan, enc::imm16(w) as usize)?;
                self.push_const(plan, c)?;
                self.binop(plan, sub, at)
            }
            op::JMP => self.jump_word(enc::imm24(w), plan),
            op::JMP_IF_FALSE => {
                let t = enc::imm24(w);
                let d = self.f.stack.len() - 1;
                self.cond_jump(plan, d, false, t)?;
                self.pop();
                Ok(())
            }
            op::JMP_IF_TRUE_PEEK => {
                let t = enc::imm24(w);
                let d = self.f.stack.len() - 1;
                self.cond_jump(plan, d, true, t)
            }
            op::JMP_IF_FALSE_PEEK => {
                let t = enc::imm24(w);
                let d = self.f.stack.len() - 1;
                self.cond_jump(plan, d, false, t)
            }
            op::CMP_JF => {
                let sub = enc::imm8(w);
                let target = self.prog.words[base + at + 1];
                self.cmp_jf(plan, sub, target)
            }
            op::RET => self.ret(plan),
            op::RET_NULL => self.ret_null(plan),
            op::POP_RET_NULL => {
                self.pop();
                self.ret_null(plan)
            }
            op::CALL_FN => self.call_fn(plan, at, enc::imm16(w), enc::argc(w) as usize),
            op::CALL_BUILTIN => {
                self.call_builtin(plan, at, enc::imm16(w), enc::argc(w) as usize, &[])
            }
            op::CALL_BUILTIN_C => {
                let c = [self.prog.words[base + at + 1] as i32];
                self.call_builtin(plan, at, enc::imm16(w), enc::argc(w) as usize, &c)
            }
            op::CALL_BUILTIN_CC => {
                let c = [
                    self.prog.words[base + at + 1] as i32,
                    self.prog.words[base + at + 2] as i32,
                ];
                self.call_builtin(plan, at, enc::imm16(w), enc::argc(w) as usize, &c)
            }
            op::CALL_VALUE => self.call_value(plan, at, enc::imm8(w) as usize),
            o if is_binop_sub(o) => self.binop(plan, o, at),
            _ => Err(Refusal::UnsupportedOpcode {
                op: o,
                fn_idx: plan.fn_idx,
                word: at as u32,
            }),
        }
    }

    // ------------------------------------------------------------ pushes

    fn push_const(&mut self, plan: &FnPlan, v: i32) -> Result<(), Refusal> {
        self.step();
        let d = self.f.stack.len();
        let r = self.dest(d, Kind::Num);
        self.imm(r, v);
        self.push(Kind::Num);
        self.put(plan, d, Kind::Num, r)
    }

    fn push_handle(&mut self, plan: &FnPlan, k: Kind, v: i32) -> Result<(), Refusal> {
        self.step();
        let d = self.f.stack.len();
        let r = self.dest(d, k);
        self.imm(r, v);
        self.push(k);
        self.put(plan, d, k, r)
    }

    /// `LoadG g` (§3.8).
    fn push_global(&mut self, plan: &FnPlan, g: usize) -> Result<(), Refusal> {
        self.step();
        let k = self.kinds.global(g);
        let d = self.f.stack.len();
        let (gb, off) = self.global_base(g)?;
        if k == Kind::Dyn {
            let tag = self.sc();
            let pay = self.sc();
            self.load_at(tag, gb, off + dev32::VALUE_TAG)?;
            self.load_at(pay, gb, off + dev32::VALUE_PAYLOAD)?;
            self.push(Kind::Dyn);
            self.put_dyn(plan, d, tag, pay)
        } else {
            let r = self.dest(d, k);
            self.load_at(r, gb, off + dev32::VALUE_PAYLOAD)?;
            self.push(k);
            self.put(plan, d, k, r)
        }
    }

    /// `StoreG g` (peek) and `StoreGPop`. Both words are written even for a
    /// non-`Dyn` global: the tag is part of the `Value` the interpreter and
    /// every builtin read back, so leaving a stale one there would be a
    /// silently wrong kind.
    fn store_global(&mut self, plan: &FnPlan, g: usize, pop: bool) -> Result<(), Refusal> {
        let d = self.f.stack.len() - 1;
        let (gb, off) = self.global_base(g)?;
        let tag = self.rd_tag(plan, d)?;
        self.store_at(tag, gb, off + dev32::VALUE_TAG)?;
        let pay = self.rd(plan, d)?;
        self.store_at(pay, gb, off + dev32::VALUE_PAYLOAD)?;
        if pop {
            self.pop();
        }
        Ok(())
    }

    fn push_local(&mut self, plan: &FnPlan, i: usize) -> Result<(), Refusal> {
        self.step();
        let k = plan.slot_kind[i];
        let d = self.f.stack.len();
        match plan.local(i) {
            SlotHome::Reg(src) => {
                let r = self.dest(d, k);
                self.code.mov_n(r, src);
                self.push(k);
                self.put(plan, d, k, r)
            }
            SlotHome::Frame(off) => {
                if k == Kind::Dyn {
                    let tag = self.sc();
                    let pay = self.sc();
                    self.load_at(tag, A1, tag_at(off))?;
                    self.load_at(pay, A1, off + dev32::VALUE_PAYLOAD)?;
                    self.push(Kind::Dyn);
                    self.put_dyn(plan, d, tag, pay)
                } else {
                    let r = self.dest(d, k);
                    self.load_at(r, A1, off)?;
                    self.push(k);
                    self.put(plan, d, k, r)
                }
            }
        }
    }

    fn store_local(&mut self, plan: &FnPlan, i: usize, pop: bool) -> Result<(), Refusal> {
        let sk = plan.slot_kind[i];
        let d = self.f.stack.len() - 1;
        match plan.local(i) {
            SlotHome::Reg(dst) => {
                // A register-homed local is never `Dyn` (ParamConv::Regs
                // requires it), so one word is the whole value.
                let r = self.rd(plan, d)?;
                self.code.mov_n(dst, r);
            }
            SlotHome::Frame(off) => {
                if sk == Kind::Dyn {
                    let tag = self.rd_tag(plan, d)?;
                    self.store_at(tag, A1, tag_at(off))?;
                    let pay = self.rd(plan, d)?;
                    self.store_at(pay, A1, off + dev32::VALUE_PAYLOAD)?;
                } else {
                    let r = self.rd(plan, d)?;
                    self.store_at(r, A1, off)?;
                }
            }
        }
        if pop {
            self.pop();
        }
        Ok(())
    }

    /// Copy the value at depth `from` to depth `to` — `Dup`/`Dup2`, and the
    /// result shuffle `StoreIdx` needs.
    fn copy_slot(&mut self, plan: &FnPlan, from: usize, to: usize) -> Result<(), Refusal> {
        self.step();
        let k = self.kind(from);
        if k == Kind::Dyn {
            let tag = self.sc();
            let pay = self.sc();
            self.load_at(tag, A1, tag_at(plan.depth_home(from)))?;
            self.load_at(pay, A1, plan.depth_home(from) + dev32::VALUE_PAYLOAD)?;
            while self.f.stack.len() <= to {
                self.push(Kind::Dyn);
            }
            self.f.stack[to] = Kind::Dyn;
            self.put_dyn(plan, to, tag, pay)
        } else {
            let r = self.rd(plan, from)?;
            while self.f.stack.len() <= to {
                self.push(k);
            }
            self.f.stack[to] = k;
            self.put(plan, to, k, r)
        }
    }

    /// `Box`: widen the top to `Dyn` by materialising its tag beside the
    /// payload. A `Dyn` slot occupies two words everywhere, so the value
    /// moves from a register or a one-word home into the two-word one.
    fn box_top(&mut self, plan: &FnPlan) -> Result<(), Refusal> {
        let d = self.f.stack.len() - 1;
        let k = self.kind(d);
        let tag = self.rd_tag(plan, d)?;
        let pay = self.rd(plan, d)?;
        self.f.stack[d] = Kind::Dyn;
        if d < REG_DEPTHS {
            self.f.in_reg[d] = false;
        }
        let _ = k;
        self.put_dyn(plan, d, tag, pay)
    }

    // ------------------------------------------------------------- arrays

    fn load_idx(&mut self, plan: &FnPlan, at: usize) -> Result<(), Refusal> {
        self.step();
        let d = self.f.stack.len();
        let arr = d - 2;
        let idx = d - 1;
        let res = elem_kind(self.kind(arr));
        self.spill_all(plan)?;
        self.site(at);
        self.code.mov_n(A10, A2);
        self.arg_tag(plan, 11, arr)?;
        self.arg_pay(plan, 12, arr)?;
        self.arg_num(plan, 13, idx)?;
        let h = if res == Kind::Dyn {
            self.env.helpers.arr_load_dyn
        } else {
            self.env.helpers.arr_load_num
        };
        self.load_lit(A8, Lit::Word(h));
        self.code.callx8(A8);
        self.bail_if_status()?;
        self.pop();
        self.pop();
        self.push(res);
        if res == Kind::Dyn {
            // `RetDyn` comes back in `a2:a3` (callee view) = `a10:a11`.
            self.put_dyn(plan, arr, 10, 11)
        } else {
            self.put(plan, arr, res, A10)
        }
    }

    fn store_idx(&mut self, plan: &FnPlan, at: usize) -> Result<(), Refusal> {
        let d = self.f.stack.len();
        let arr = d - 3;
        let idx = d - 2;
        let val = d - 1;
        let vk = self.kind(val);
        self.spill_all(plan)?;
        self.site(at);
        self.code.mov_n(A10, A2);
        self.arg_tag(plan, 11, arr)?;
        self.arg_pay(plan, 12, arr)?;
        self.arg_num(plan, 13, idx)?;
        self.arg_tag(plan, 14, val)?;
        self.arg_pay(plan, 15, val)?;
        self.load_lit(A8, Lit::Word(self.env.helpers.arr_store));
        self.code.callx8(A8);
        self.bail_if_status()?;
        // The interpreter pushes the stored value back (assignment is an
        // expression); here that is a move from the value's home to the
        // result's.
        self.f.sc = 0;
        self.copy_slot(plan, val, arr)?;
        self.truncate(arr + 1);
        self.f.stack[arr] = vk;
        Ok(())
    }

    fn arr_len(&mut self, plan: &FnPlan, at: usize) -> Result<(), Refusal> {
        let d = self.f.stack.len() - 1;
        self.spill_all(plan)?;
        self.site(at);
        self.code.mov_n(A10, A2);
        self.arg_tag(plan, 11, d)?;
        self.arg_pay(plan, 12, d)?;
        self.load_lit(A8, Lit::Word(self.env.helpers.arr_len));
        self.code.callx8(A8);
        self.bail_if_status()?;
        self.pop();
        self.push(Kind::Num);
        self.put(plan, d, Kind::Num, A10)
    }

    fn new_array(&mut self, plan: &FnPlan, at: usize, n: usize) -> Result<(), Refusal> {
        let d = self.f.stack.len();
        let bottom = d - n;
        self.spill_all(plan)?;
        for j in 0..n {
            self.box_arg(plan, j, bottom + j)?;
        }
        self.site(at);
        let res = if (0..n).all(|j| self.kind(bottom + j) == Kind::Num) {
            Kind::ArrNum
        } else {
            Kind::Arr
        };
        self.code.mov_n(A10, A2);
        self.imm(11, n as i32);
        self.addr_of(12, A1, plan.scratch_off)?;
        self.load_lit(A8, Lit::Word(self.env.helpers.new_array));
        self.code.callx8(A8);
        self.bail_if_status()?;
        self.truncate(bottom);
        self.push(res);
        self.put(plan, bottom, res, A10)
    }

    fn const_arr(&mut self, plan: &FnPlan, at: usize, d: u32) -> Result<(), Refusal> {
        let top = self.f.stack.len();
        self.spill_all(plan)?;
        self.site(at);
        self.code.mov_n(A10, A2);
        self.imm(11, d as i32);
        self.load_lit(A8, Lit::Word(self.env.helpers.const_arr));
        self.code.callx8(A8);
        self.bail_if_status()?;
        self.push(Kind::ArrNum);
        self.put(plan, top, Kind::ArrNum, A10)
    }

    /// `Assert m`: call the helper only when the condition is falsy, which
    /// is what keeps the `format!` off the hot path exactly as the
    /// interpreter's `#[cold] assert_failed` does.
    fn assert(&mut self, plan: &FnPlan, at: usize, m: u32) -> Result<(), Refusal> {
        let d = self.f.stack.len() - 1;
        let k = self.kind(d);
        self.spill_all(plan)?;
        let mut skips: Vec<usize> = Vec::new();
        match k {
            Kind::Num => {
                let r = self.rd(plan, d)?;
                skips.push(self.code.branch_z_forward(ZCond::Nez, r));
            }
            Kind::Dyn => {
                let tag = self.sc();
                self.load_at(tag, A1, tag_at(plan.depth_home(d)))?;
                skips.push(self.code.branch_z_forward(ZCond::Nez, tag));
                let pay = self.sc();
                self.load_at(pay, A1, plan.depth_home(d) + dev32::VALUE_PAYLOAD)?;
                skips.push(self.code.branch_z_forward(ZCond::Nez, pay));
            }
            // A reference is always truthy (`Value::truthy`), so the
            // assertion can never fire.
            _ => {
                self.pop();
                return Ok(());
            }
        }
        self.site(at);
        self.code.mov_n(A10, A2);
        self.imm(11, m as i32);
        self.load_lit(A8, Lit::Word(self.env.helpers.assert_fail));
        self.code.callx8(A8);
        self.goto_bail();
        let here = self.code.here();
        for s in skips {
            self.code
                .patch_branch_z(s, here)
                .map_err(|e| self.reach(e))?;
        }
        self.pop();
        Ok(())
    }

    // ------------------------------------------------------- control flow

    /// `JmpIfFalse` / the two peeking forms.
    fn cond_jump(
        &mut self,
        plan: &FnPlan,
        d: usize,
        on_truthy: bool,
        target: u32,
    ) -> Result<(), Refusal> {
        match self.kind(d) {
            Kind::Num => {
                let r = self.rd(plan, d)?;
                let c = if on_truthy { ZCond::Nez } else { ZCond::Eqz };
                self.branch_z_word(c, r, target, plan)
            }
            Kind::Dyn => {
                let h = plan.depth_home(d);
                if on_truthy {
                    // truthy ⟺ tag ≠ 0 (a reference) OR payload ≠ 0
                    let tag = self.sc();
                    self.load_at(tag, A1, tag_at(h))?;
                    self.branch_z_word(ZCond::Nez, tag, target, plan)?;
                    self.f.sc = 0;
                    let pay = self.sc();
                    self.load_at(pay, A1, h + dev32::VALUE_PAYLOAD)?;
                    self.branch_z_word(ZCond::Nez, pay, target, plan)
                } else {
                    // falsy ⟺ tag == 0 AND payload == 0
                    let tag = self.sc();
                    self.load_at(tag, A1, tag_at(h))?;
                    let skip = self.code.branch_z_forward(ZCond::Nez, tag);
                    let pay = self.sc();
                    self.load_at(pay, A1, h + dev32::VALUE_PAYLOAD)?;
                    self.branch_z_word(ZCond::Eqz, pay, target, plan)?;
                    let here = self.code.here();
                    self.code
                        .patch_branch_z(skip, here)
                        .map_err(|e| self.reach(e))
                }
            }
            // A reference is always truthy.
            _ => {
                if on_truthy {
                    self.jump_word(target, plan)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// The fused `<cmp>; JmpIfFalse t` — one conditional branch, no
    /// materialised boolean (§3.5).
    fn cmp_jf(&mut self, plan: &FnPlan, sub: u8, target: u32) -> Result<(), Refusal> {
        let d = self.f.stack.len();
        let a = d - 2;
        let b = d - 1;
        let numeric = matches!(sub, op::LT | op::LE | op::GT | op::GE)
            || (self.kind(a) == Kind::Num && self.kind(b) == Kind::Num);
        if numeric {
            let ra = self.rd_num(plan, a)?;
            let rb = self.rd_num(plan, b)?;
            let (cond, swap) = cmp_branch(sub);
            // Jump when the comparison is FALSE, so the branch tests the
            // inverted relation with the same operand order.
            let (s, t) = if swap { (rb, ra) } else { (ra, rb) };
            self.pop();
            self.pop();
            self.branch_word(cond.invert(), s, t, target, plan)
        } else {
            // `==`/`!=` on anything but two numbers is reference identity
            // (`vm::value_eq`), so materialise the boolean and branch on it.
            self.compare(plan, sub)?;
            let d = self.f.stack.len() - 1;
            let r = self.rd(plan, d)?;
            self.pop();
            self.branch_z_word(ZCond::Eqz, r, target, plan)
        }
    }

    fn ret(&mut self, plan: &FnPlan) -> Result<(), Refusal> {
        let d = self.f.stack.len() - 1;
        if plan.boxed_return() {
            let tag = self.rd_tag(plan, d)?;
            let pay = self.rd(plan, d)?;
            self.code.mov_n(A2, tag);
            self.code.mov_n(3, pay);
        } else {
            let r = self.rd(plan, d)?;
            self.code.mov_n(A2, r);
        }
        self.code.retw_n();
        self.pop();
        Ok(())
    }

    /// `RetNull` returns `Value::default()` — `Num(0)`, which is a zero tag
    /// and a zero payload, so both halves are the same `movi`.
    fn ret_null(&mut self, plan: &FnPlan) -> Result<(), Refusal> {
        self.imm(A2, 0);
        if plan.boxed_return() {
            self.imm(3, 0);
        }
        self.code.retw_n();
        Ok(())
    }

    // -------------------------------------------------------------- calls

    fn call_fn(
        &mut self,
        plan: &FnPlan,
        at: usize,
        callee: u16,
        argc: usize,
    ) -> Result<(), Refusal> {
        let ci = callee as usize;
        let cp = self.prog.fns[ci].params as usize;
        let ck: Vec<Kind> = self.kinds.fns[ci].slots.iter().take(cp).copied().collect();
        let cret = self.kinds.ret(ci);
        // The CALLEE's conventions, read from its plan rather than
        // re-derived — one place decides them (`plan.rs`).
        let creg = self.plans[ci].conv == ParamConv::Regs;
        let boxed = self.plans[ci].boxed_return();
        debug_assert_eq!(creg, cp <= MAX_REG_PARAMS && ck.iter().all(|k| *k != Kind::Dyn));
        let d = self.f.stack.len();
        let bottom = d - argc;

        self.spill_all(plan)?;
        self.fuel()?;
        self.site(at);

        if creg {
            for j in 0..cp {
                let dst = 11 + j as u8;
                if j < argc {
                    let src = bottom + j;
                    if ck[j] == Kind::Num {
                        self.arg_num(plan, dst, src)?;
                    } else {
                        self.arg_pay(plan, dst, src)?;
                    }
                } else {
                    // `push_frame` defaults the slots past the argument
                    // count; the emitter knows `argc` statically, so the
                    // default is materialised here.
                    self.imm(dst, 0);
                }
            }
        } else {
            let mut off = dev32::ARGS;
            for j in 0..cp {
                if ck[j] == Kind::Dyn {
                    if j < argc {
                        self.arg_tag(plan, A8, bottom + j)?;
                        self.store_at_with(A8, A2, off, A9)?;
                        self.arg_pay(plan, A8, bottom + j)?;
                        self.store_at_with(A8, A2, off + 4, A9)?;
                    } else {
                        self.imm(A8, 0);
                        self.store_at_with(A8, A2, off, A9)?;
                        self.store_at_with(A8, A2, off + 4, A9)?;
                    }
                    off += 8;
                } else {
                    if j < argc {
                        if ck[j] == Kind::Num {
                            self.arg_num(plan, A8, bottom + j)?;
                        } else {
                            self.arg_pay(plan, A8, bottom + j)?;
                        }
                    } else {
                        self.imm(A8, 0);
                    }
                    self.store_at_with(A8, A2, off, A9)?;
                    off += 4;
                }
            }
        }
        self.code.mov_n(A10, A2);
        self.load_lit(A8, Lit::FnAddr(callee));
        self.code.callx8(A8);
        self.bail_if_status()?;

        self.truncate(bottom);
        self.push(cret);
        // A callee that returns boxed hands back `(tag, payload)` in
        // `a10:a11` even when its declared kind is narrower, so an unboxed
        // result is the PAYLOAD half, not `a10`.
        if cret == Kind::Dyn {
            self.put_dyn(plan, bottom, A10, 11)
        } else if boxed {
            self.put(plan, bottom, cret, 11)
        } else {
            self.put(plan, bottom, cret, A10)
        }
    }

    fn call_builtin(
        &mut self,
        plan: &FnPlan,
        at: usize,
        b: u16,
        argc: usize,
        consts: &[i32],
    ) -> Result<(), Refusal> {
        let from_stack = argc - consts.len();
        let d = self.f.stack.len();
        let bottom = d - from_stack;

        let entry = &BUILTIN_ENTRIES[b as usize];
        let sig = builtin_sig(b);

        // The pushed kind — the same rule `kinds::walk_fn` applies.
        let res = match sig.ret {
            SigRet::Num => Kind::Num,
            SigRet::Dyn => Kind::Dyn,
            SigRet::NewArrNum => Kind::ArrNum,
            SigRet::Arg(i) => {
                let i = i as usize;
                if i < from_stack {
                    self.kind(bottom + i)
                } else {
                    Kind::Num
                }
            }
        };

        self.spill_all(plan)?;

        // The direct tier-1 path (§3.5/§4), used only when the call's
        // arity matches the signature EXACTLY. **Deviation from §4**, which
        // has the emitter materialise a builtin's default arguments (the
        // 0.5 duty of a one-argument `square`); the emitter carries no
        // table of defaults, so a mismatched arity falls back to `generic`,
        // which is the interpreter's own marshalling and cannot be wrong.

        let direct_arity = match entry.sig() {
            DirectSig::N1 => Some((1usize, false)),
            DirectSig::N2 => Some((2, false)),
            DirectSig::N3 => Some((3, false)),
            DirectSig::N4 => Some((4, false)),
            DirectSig::C0 => Some((0, true)),
            DirectSig::C1 => Some((1, true)),
            DirectSig::C2 => Some((2, true)),
            DirectSig::C3 => Some((3, true)),
            DirectSig::None => None,
        };
        if let Some((n, takes_ctx)) = direct_arity {
            if n == argc && entry.ret_kind == RET_NUM {
                let first = if takes_ctx {
                    self.code.mov_n(A10, A2);
                    11u8
                } else {
                    10
                };
                for j in 0..argc {
                    let dst = first + j as u8;
                    if j < from_stack {
                        self.arg_num(plan, dst, bottom + j)?;
                    } else {
                        self.imm(dst, consts[j - from_stack]);
                    }
                }

                let ok = self.code.l32i(A8, A2, dev32::BUILTINS);
                debug_assert!(ok);
                self.load_at_with(A8, A8, b as u32 * dev32::BUILTIN_ENTRY + dev32::ENTRY_DIRECT, A9)?;
                self.code.callx8(A8);
                // A direct entry cannot fail, so there is no status check.

                self.truncate(bottom);
                self.push(Kind::Num);
                return self.put(plan, bottom, Kind::Num, A10);

            }
        }

        // The generic path: box every argument into the frame scratch and
        // let the wrapper run the interpreter's own ladder (§4).
        for j in 0..argc {
            if j < from_stack {
                self.box_arg(plan, j, bottom + j)?;
            } else {
                self.box_const(plan, j, consts[j - from_stack])?;
            }
        }
        self.site(at);
        self.code.mov_n(A10, A2);
        self.addr_of(11, A1, plan.scratch_off)?;
        self.imm(12, argc as i32);
        let ok = self.code.l32i(A8, A2, dev32::BUILTINS);
        debug_assert!(ok);
        self.load_at_with(A8, A8, b as u32 * dev32::BUILTIN_ENTRY + dev32::ENTRY_GENERIC, A9)?;
        self.code.callx8(A8);
        self.bail_if_status()?;
        self.truncate(bottom);
        self.push(res);

        if res == Kind::Dyn {
            self.put_dyn(plan, bottom, A10, 11)
        } else {
            // `RetDyn` is (tag, payload); an unboxed result is the payload.
            self.put(plan, bottom, res, 11)
        }
    }

    /// `CallValue argc` — the callee is a run-time value.
    ///
    /// **Deviation from §3.5**, which has a `call_value` helper resolve the
    /// callee AND call it. That would be a Rust → native trampoline, which
    /// §4 says this design does not have and which cannot be modelled on a
    /// host. `call_value_target` resolves only; the dispatch below is
    /// native code, so native → native stays native.
    fn call_value(&mut self, plan: &FnPlan, at: usize, argc: usize) -> Result<(), Refusal> {
        let d = self.f.stack.len();
        let bottom = d - argc - 1;
        self.spill_all(plan)?;
        for j in 0..argc {
            self.box_arg(plan, j, bottom + 1 + j)?;
        }
        self.site(at);
        self.code.mov_n(A10, A2);
        self.arg_tag(plan, 11, bottom)?;
        self.arg_pay(plan, 12, bottom)?;
        self.load_lit(A8, Lit::Word(self.env.helpers.call_value));
        self.code.callx8(A8);
        // `Ret2 { val, status }` → `a10` = val, `a11` = the discriminant:
        // 0 native address, 1 builtin id, 2 error. The native arm is a few
        // hundred bytes of `ctx.args` filling, so the branch that skips it
        // has to be a `bnez` (±2 KB reach) and not a `beqi` (±128 B).
        let not_native = self.code.branch_z_forward(ZCond::Nez, 11);

        // --- native: fill ctx.args with the boxed arguments. Every
        // parameter of a function reachable as a value is `Dyn` (§2.3, and
        // `plan_all` refuses a program where it is not), so the layout is
        // two words per parameter, and the slots past `argc` read
        // `Num(0)` — exactly `push_frame`'s defaulting.
        self.code.mov_n(A9, A10);
        for j in 0..(luxel_core::jit::CTX_ARGS / 2) {
            let off = dev32::ARGS + j as u32 * 8;
            if j < argc {
                let s = plan.scratch_off + j as u32 * dev32::VALUE;
                self.load_at(A8, A1, s + dev32::VALUE_TAG)?;
                self.store_at_with(A8, A2, off, A10)?;
                self.load_at(A8, A1, s + dev32::VALUE_PAYLOAD)?;
                self.store_at_with(A8, A2, off + 4, A10)?;
            } else {
                self.imm(A8, 0);
                self.store_at_with(A8, A2, off, A10)?;
                self.store_at_with(A8, A2, off + 4, A10)?;
            }
        }
        self.code.mov_n(A10, A2);
        self.code.callx8(A9);
        let to_done = self.code.j_forward();

        // --- builtin: `ctx.builtins[id].generic(ctx, &scratch, argc)`
        let here = self.code.here();
        self.code
            .patch_branch_z(not_native, here)
            .map_err(|e| self.reach(e))?;
        let to_err = self.code.branch_i_forward(false, 11, 1);
        self.code.mov_n(A9, A10);
        self.imm(A8, dev32::BUILTIN_ENTRY as i32);
        self.code.mull(A9, A9, A8);
        let ok = self.code.l32i(A8, A2, dev32::BUILTINS);
        debug_assert!(ok);
        self.code.add(A9, A9, A8);
        let ok = self.code.l32i(A9, A9, dev32::ENTRY_GENERIC);
        debug_assert!(ok);
        self.code.mov_n(A10, A2);
        self.addr_of(11, A1, plan.scratch_off)?;
        self.imm(12, argc as i32);
        self.code.callx8(A9);
        let past_err = self.code.j_forward();

        // --- error: the helper has already filled `*ctx.err` and set the
        // status, so this arm is only the jump to the bail epilogue.
        let here = self.code.here();
        self.code
            .patch_branch_i(to_err, here)
            .map_err(|e| self.reach(e))?;
        self.goto_bail();

        let here = self.code.here();
        self.code
            .patch_j(past_err, here)
            .map_err(|e| self.reach(e))?;
        self.code.patch_j(to_done, here).map_err(|e| self.reach(e))?;
        self.bail_if_status()?;
        self.truncate(bottom);
        self.push(Kind::Dyn);
        self.put_dyn(plan, bottom, A10, 11)
    }

    // --------------------------------------------------------- arithmetic

    /// `Not`: `Value::truthy` negated, as `Fx::ONE` or `Fx::ZERO`.
    fn logical_not(&mut self, plan: &FnPlan) -> Result<(), Refusal> {
        let d = self.f.stack.len() - 1;
        let k = self.kind(d);
        let r = self.sc();
        self.imm(r, 0);
        let mut skips: Vec<usize> = Vec::new();
        match k {
            Kind::Num => {
                let a = self.rd(plan, d)?;
                skips.push(self.code.branch_z_forward(ZCond::Nez, a));
            }
            Kind::Dyn => {
                let tag = self.sc();
                self.load_at(tag, A1, tag_at(plan.depth_home(d)))?;
                skips.push(self.code.branch_z_forward(ZCond::Nez, tag));
                let pay = self.sc();
                self.load_at(pay, A1, plan.depth_home(d) + dev32::VALUE_PAYLOAD)?;
                skips.push(self.code.branch_z_forward(ZCond::Nez, pay));
            }
            // always truthy ⇒ `!x` is always 0, already in `r`
            _ => {}
        }
        if !skips.is_empty() {
            self.imm(r, ONE);
        }
        let here = self.code.here();
        for s in skips {
            self.code
                .patch_branch_z(s, here)
                .map_err(|e| self.reach(e))?;
        }
        self.pop();
        self.push(Kind::Num);
        self.put(plan, d, Kind::Num, r)
    }

    fn binop(&mut self, plan: &FnPlan, sub: u8, at: usize) -> Result<(), Refusal> {
        self.step();
        match sub {
            op::EQ | op::NE => return self.compare(plan, sub),
            op::LT | op::LE | op::GT | op::GE => return self.compare(plan, sub),
            _ => {}
        }
        let d = self.f.stack.len();
        let a = d - 2;
        let b = d - 1;
        match sub {
            op::DIV | op::POW => {
                self.spill_all(plan)?;
                self.arg_num(plan, A10, a)?;
                self.arg_num(plan, 11, b)?;
                let h = if sub == op::DIV {
                    self.env.helpers.fx_div
                } else {
                    self.env.helpers.fx_pow
                };
                self.load_lit(A8, Lit::Word(h));
                self.code.callx8(A8);
                let _ = at;
                self.pop();
                self.pop();
                self.push(Kind::Num);
                return self.put(plan, a, Kind::Num, A10);
            }
            _ => {}
        }
        let ra = self.rd_num(plan, a)?;
        let rb = self.rd_num(plan, b)?;
        // Most rows write their destination in the same instruction that
        // reads the operands, so the destination may alias one of them.
        // `Rem` does not: its zero guard materialises the 0 FIRST, which
        // would clobber `ra` when both are the depth's register home.
        let r = if sub == op::REM {
            self.sc()
        } else {
            self.dest(a, Kind::Num)
        };
        match sub {
            op::ADD => self.code.add(r, ra, rb),
            op::SUB => self.code.sub(r, ra, rb),
            op::BIT_AND => self.code.and(r, ra, rb),
            op::BIT_OR => self.code.or(r, ra, rb),
            op::BIT_XOR => self.code.xor(r, ra, rb),
            op::MUL => {
                // Exact `(a·b) >> 16` of the 64-bit product (`fixed.rs`
                // `impl Mul`): the high half from `mulsh`, the low half
                // from `mull`, then one funnel shift. `rb` is dead after
                // the multiply, so it carries the low half and only one
                // extra register is needed.
                let hi = self.sc();
                self.code.mulsh(hi, ra, rb);
                self.code.mull(rb, ra, rb);
                let ok = self.code.ssai(16);
                debug_assert!(ok);
                self.code.src(r, hi, rb);
            }
            op::REM => {
                // `wrapping_rem`, sign of the dividend, and `x % 0 == 0`
                // (oracle-verified). `rems` traps on a zero divisor, hence
                // the guard.
                self.imm(r, 0);
                let skip = self.code.branch_z_forward(ZCond::Eqz, rb);
                self.code.rems(r, ra, rb);
                let here = self.code.here();
                self.code
                    .patch_branch_z(skip, here)
                    .map_err(|e| self.reach(e))?;
            }
            op::SHL | op::SHR => {
                // The shift count is `(rhs.to_int_trunc() & 31)`, and
                // `to_int_trunc` TRUNCATES TOWARD ZERO
                // (`i32::wrapping_div(65536)`).
                //
                // **§3.5's `srai t, b, 16` is wrong here**: that is a
                // FLOOR, so a right-hand side in (−1, 0) — `x << -0.5`,
                // which the oracle pins as a shift by 0 — would become a
                // shift by 31. One `quos` by 65536 is the exact
                // truncating division, and the divisor is a constant so
                // it can never trap.
                let k = self.sc();
                self.imm(k, ONE);
                self.code.quos(k, rb, k);
                if sub == op::SHL {
                    self.code.ssl(k);
                    self.code.sll(r, ra);
                } else {
                    self.code.ssr(k);
                    self.code.sra(r, ra);
                }
            }
            _ => {
                return Err(Refusal::UnsupportedOpcode {
                    op: sub,
                    fn_idx: plan.fn_idx,
                    word: at as u32,
                })
            }
        }
        self.pop();
        self.pop();
        self.push(Kind::Num);
        self.put(plan, a, Kind::Num, r)
    }

    /// The six relational operators, materialised as `0` or `Fx::ONE`.
    ///
    /// `==`/`!=` are REFERENCE IDENTITY (`vm::value_eq`), not numeric
    /// equality — the one place operand kinds matter. §3.5's "unbox to 0"
    /// rule would make `arr == 0` true; it is false.
    fn compare(&mut self, plan: &FnPlan, sub: u8) -> Result<(), Refusal> {
        self.step();
        let d = self.f.stack.len();
        let a = d - 2;
        let b = d - 1;
        let eqish = sub == op::EQ || sub == op::NE;
        let ka = self.kind(a);
        let kb = self.kind(b);
        let r;
        if eqish && (ka == Kind::Dyn || kb == Kind::Dyn || ka != kb) {
            let want_eq = sub == op::EQ;
            if ka != Kind::Dyn && kb != Kind::Dyn && tag_of(ka) != tag_of(kb) {
                // Different kinds can never be `value_eq`, whatever the
                // payloads hold.
                r = self.sc();
                self.imm(r, if want_eq { 0 } else { ONE });
            } else {
                r = self.sc();
                self.imm(r, if want_eq { 0 } else { ONE });
                let ta = self.sc();
                let tb = self.sc();
                self.read_word(plan, ta, a, true)?;
                self.read_word(plan, tb, b, true)?;
                let no = self.code.branch_forward(Cond::Ne, ta, tb);
                self.read_word(plan, ta, a, false)?;
                self.read_word(plan, tb, b, false)?;
                let no2 = self.code.branch_forward(Cond::Ne, ta, tb);
                self.imm(r, if want_eq { ONE } else { 0 });
                let here = self.code.here();
                self.code.patch_branch(no, here).map_err(|e| self.reach(e))?;
                self.code
                    .patch_branch(no2, here)
                    .map_err(|e| self.reach(e))?;
            }
        } else {
            let ra = self.rd_num(plan, a)?;
            let rb = self.rd_num(plan, b)?;
            r = self.sc();
            self.imm(r, 0);
            let (cond, swap) = cmp_branch(sub);
            let (s, t) = if swap { (rb, ra) } else { (ra, rb) };
            let no = self.code.branch_forward(cond.invert(), s, t);
            self.imm(r, ONE);
            let here = self.code.here();
            self.code.patch_branch(no, here).map_err(|e| self.reach(e))?;
        }
        self.pop();
        self.pop();
        self.push(Kind::Num);
        self.put(plan, a, Kind::Num, r)
    }

    /// Read depth `d`'s tag or payload word into a GIVEN register — the
    /// register-reuse the `value_eq` sequence needs to stay inside four
    /// scratch registers.
    fn read_word(
        &mut self,
        plan: &FnPlan,
        dst: Reg,
        d: usize,
        tag: bool,
    ) -> Result<(), Refusal> {
        let k = self.kind(d);
        if tag {
            if k == Kind::Dyn {
                self.load_at(dst, A1, tag_at(plan.depth_home(d)))
            } else {
                self.imm(dst, tag_of(k) as i32);
                Ok(())
            }
        } else if k == Kind::Dyn {
            self.load_at(dst, A1, plan.depth_home(d) + dev32::VALUE_PAYLOAD)
        } else if let Some(reg) = FnPlan::depth_reg(d, k) {
            if self.f.in_reg[d] {
                self.code.mov_n(dst, reg);
                Ok(())
            } else {
                self.load_at(dst, A1, plan.depth_home(d))
            }
        } else {
            self.load_at(dst, A1, payload_at(plan.depth_home(d), k))
        }
    }
}

// ================================================================== finish

impl<'a> Emitter<'a> {
    /// Prepend the literal pool, fill it in, and patch every `l32r`.
    fn finish(mut self) -> Result<NativeImage, Refusal> {

        let pool_len = self.pool.keys.len() as u32;
        let p = pool_len as usize * 4;
        let mut img = Asm::with_pool(p);
        img.append(self.code.bytes());

        for e in self.entries.iter_mut() {
            *e += p as u32;
        }
        for (i, k) in self.pool.keys.iter().enumerate() {
            let w = match *k {
                Lit::Word(w) => w,
                Lit::FnAddr(f) => self.env.code_base + self.entries[f as usize],
            };
            img.put_word(i * 4, w);
        }
        for (site, li) in &self.l32r_fix {
            img.patch_l32r(p + site, li * 4)
                .map_err(|e| Refusal::L32rReach {
                    fn_idx: u16::MAX,
                    word: 0,
                    distance: e.distance.unsigned_abs(),
                })?;
        }

        let words = img.into_words();
        let bytes = words.len() * 4;
        if bytes > self.env.max_code {
            return Err(Refusal::TooLarge {
                bytes,
                max: self.env.max_code,
            });
        }
        let exports = self
            .prog
            .exported_fns
            .iter()
            .map(|(n, i)| (n.clone(), self.entries[*i as usize]))
            .collect();
        Ok(NativeImage {
            words,
            pool_len,
            entries: self.entries,
            exports,
            abi: self.abi,
        })
    }
}
