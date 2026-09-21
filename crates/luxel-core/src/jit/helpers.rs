//! The `extern "C"` helpers generated Xtensa code calls (Gitea #651, phase
//! 2 of #607) — the rows of docs/jit-design.md §3.5 that say "helpers",
//! plus the three bail stubs of §3.6.
//!
//! Every one of these is a THIN WRAPPER over the interpreter's own arm in
//! `crate::vm`. There is no second implementation of indexing, of the array
//! budget, of the copy-on-write promotion or of an error message anywhere
//! in the JIT: a helper unboxes the ABI words, calls the same `Vm` method
//! `Vm::run` calls, and re-packs the result. That is what makes
//! `tests/jithelpers.rs` able to assert helper-against-interpreter rather
//! than helper-against-expectation.
//!
//! **Calling convention** (§3.2). `ctx: *mut JitCtx` is `a2`; the rest are
//! single words in `a3…a7`. Anything that can fail signals through the
//! context — [`JitCtx::fail`] sets [`STATUS_ERR`] and fills `*ctx.err` —
//! and returns a zero/default value, so the emitted code is always
//! `callx8 h; l32i t, ctx, OFFSET_STATUS; bnez t, bail`. Errors never
//! travel in a return value; that is the one shape the whole ABI has
//! (§3.2, and `table.rs`'s `generic` wrappers do the same).
//!
//! **Array operands arrive as a (tag, payload) PAIR**, never as a bare
//! arena id, so the statically-known-`Arr` case and the `Dyn` case are the
//! same helper: the "indexing a non-array value" error has to be reachable
//! from the `Dyn` path, and duplicating the helper per kind would mean
//! duplicating that check. Two words in registers cost the emitter
//! nothing — a `Dyn` slot already IS two words (§3.3), and an `Arr` slot
//! materialises the tag with one `movi` (the `Box` row of §3.5).
//!
//! # Deviation from §3.5
//!
//! §3.5's `CallValue` row says the helper "resolves `Fun` →
//! `ctx.fn_table[idx]` (native, Dyn params) or `Builtin` → generic table"
//! and that the emitted code is just `callx8 call_value` — i.e. the helper
//! would CALL the resolved target. It does not, and cannot:
//!
//! - That would be a Rust → native trampoline, and §4 is explicit that
//!   this design has none. Every other crossing goes the other way
//!   (generated code calls Rust), which is what keeps the windowed-ABI
//!   story to "helpers are ordinary `extern "C"` functions".
//! - It would be untestable on the host. Everything in this file is
//!   differentially tested against the interpreter on x86, and x86 cannot
//!   execute the Xtensa code a trampoline would have to jump to.
//!
//! [`call_value_target`] therefore RESOLVES ONLY and hands back a target
//! plus a discriminant; the emitted code performs the `callx8` (native
//! function) or the table call (builtin) itself, exactly as it already
//! does for the `CallFn` and `CallBuiltin` rows. Nothing else about the
//! row changes: the argument boxing, the `ctx.args` handoff and the
//! post-call status test are the ones §3.5 already specifies.
//!
//! # Error attribution
//!
//! `Vm::err_at` builds a [`VmError`] from `self.frames.last()`, and during
//! native execution there are NO interpreter frames — it would report
//! `fn_idx: u16::MAX, pc: u32::MAX`. §3.6 has the emitter store the site
//! into `ctx.fn_idx` / `ctx.insn_at` before each fallible call instead, and
//! [`ctx_err`] is `err_at` reading it from there. Every fallible helper
//! reports through it, so a runtime error out of native code is
//! indistinguishable from the interpreter's.

use alloc::string::String;

use crate::fixed::Fx;
use crate::fmath;
use crate::jit::ctx::JitCtx;
use crate::jit::table::RetDyn;
use crate::jit::Ret2;
use crate::vm::{Value, ValueRaw, VmError, ERR_EXEC_LIMIT, TAG_BUILTIN, TAG_FUN};

// --------------------------------------------------------- attribution

/// `Vm::err_at` for native code: the site comes from `ctx.fn_idx` /
/// `ctx.insn_at` rather than from an interpreter frame (docs/jit-design.md
/// §3.6).
///
/// The `line`/`col` lookup is `err_at`'s, verbatim —
/// `prog.fns[fn_idx].pos_at(pc)` — and `is_assert` is false, because the
/// only assert error is [`assert_fail`]'s and that one comes out of
/// `Vm::assert_failed`. A `fn_idx` past the end of `prog.fns` cannot be
/// produced by the emitter, but it can be produced by a hand-built context
/// (`JitCtx::for_builtin_call` leaves `u16::MAX`), so the lookup is
/// checked rather than indexed: a helper must not panic.
///
/// # Safety
/// `ctx.prog` must point at the live [`crate::vm::Program`].
unsafe fn ctx_err(ctx: &JitCtx, message: String) -> VmError {
    let prog = &*ctx.prog;
    let pc = ctx.insn_at;
    let (line, col) = match prog.fns.get(ctx.fn_idx as usize) {
        Some(f) => f.pos_at(pc),
        None => (0, 0),
    };
    VmError {
        message,
        fn_idx: ctx.fn_idx,
        pc,
        line,
        col,
        is_assert: false,
    }
}

/// [`ctx_err`] + [`JitCtx::fail`] for the `&'static str` messages the
/// interpreter's `fail!` sites raise. Out of line and `#[cold]` for the
/// same reason `Vm::err_static` is: `String` allocation and the error path
/// have no business in a helper's hot body.
///
/// # Safety
/// As [`ctx_err`], plus `ctx.err` must point at a live slot.
#[inline(never)]
#[cold]
unsafe fn fail_str(ctx: &mut JitCtx, message: &str) {
    let e = ctx_err(ctx, String::from(message));
    ctx.fail(e);
}

// ------------------------------------------------------ infallible math

/// `Div` on two raw 16.16 words — the `Div` row of docs/jit-design.md §3.5.
///
/// `fixed.rs`'s `impl Div for Fx` has three exact 32-bit paths and an i64
/// fallback, which is why the row keeps it in Rust rather than emitting a
/// divide: division by zero is 0 (oracle-verified), and `MIN / -1` has to
/// wrap the same way on every path. Cannot fail, so no context.
pub extern "C" fn fx_div(a: i32, b: i32) -> i32 {
    (Fx::from_raw(a) / Fx::from_raw(b)).raw()
}

/// `Pow` on two raw 16.16 words — the `Pow` row of docs/jit-design.md
/// §3.5, i.e. `fmath::pow`. Cannot fail, so no context.
pub extern "C" fn fx_pow(a: i32, b: i32) -> i32 {
    fmath::pow(Fx::from_raw(a), Fx::from_raw(b)).raw()
}

// ------------------------------------------------------------- indexing

/// Unbox an ABI (tag, payload) pair. One place, so no helper can get the
/// order wrong.
#[inline]
fn value_of(tag: u32, payload: u32) -> Value {
    Value::from_raw(ValueRaw { tag, payload })
}

/// `LoadIdx` where the static kinds are `ArrNum, Num` — the
/// `arr_load_num` row of docs/jit-design.md §3.5, i.e. the `op::LOAD_IDX`
/// arm of `Vm::run` (`Vm::index_read`).
///
/// `idx` is a RAW `Fx` word and truncates toward zero, with the bounds
/// check on the truncated index (`a[3.5]` on a 3-slot array is out of
/// range) — the semantics `index_read` implements and the comment above
/// `op::LOAD_IDX` records from the oracle.
///
/// The result is coerced with `Value::num` rather than trusted to be a
/// `Num`. The `ArrNum` kind PROMISES numeric elements, but the promise is
/// the verifier's, and an init-time failure that left a non-`Num` in the
/// array must produce `0` here — the same value the interpreter's
/// arithmetic would see — not a reinterpreted arena id
/// (docs/jit-design.md §2.3, last bullet).
///
/// Errors: `"indexing a non-array value"` for a non-`Arr` operand,
/// `"array index out of bounds"` for a negative or past-the-end index.
/// Returns `0` on failure.
///
/// # Safety
/// `ctx` must be a live [`JitCtx`] whose `vm`, `prog` and `err` point at
/// live objects.
pub unsafe extern "C" fn arr_load_num(ctx: *mut JitCtx, atag: u32, apay: u32, idx: i32) -> i32 {
    let c = &mut *ctx;
    let vm = &mut *c.vm;
    let prog = &*c.prog;
    match vm.index_read(prog, value_of(atag, apay), Fx::from_raw(idx)) {
        Ok(v) => v.num().raw(),
        Err(m) => {
            fail_str(c, m);
            0
        }
    }
}

/// `LoadIdx` where the array operand is `Arr` or `Dyn` — the
/// `arr_load_dyn` row of docs/jit-design.md §3.5. Identical to
/// [`arr_load_num`] but for returning the element as a boxed [`RetDyn`]
/// (two words, `a2:a3`) instead of coercing it to a number.
///
/// Returns [`RetDyn::ZERO`] on failure.
///
/// # Safety
/// As [`arr_load_num`].
pub unsafe extern "C" fn arr_load_dyn(ctx: *mut JitCtx, atag: u32, apay: u32, idx: i32) -> RetDyn {
    let c = &mut *ctx;
    let vm = &mut *c.vm;
    let prog = &*c.prog;
    match vm.index_read(prog, value_of(atag, apay), Fx::from_raw(idx)) {
        Ok(v) => RetDyn::from_value(v),
        Err(m) => {
            fail_str(c, m);
            RetDyn::ZERO
        }
    }
}

/// `StoreIdx` — the `arr_store` row of docs/jit-design.md §3.5, i.e. the
/// `op::STORE_IDX` arm of `Vm::run`.
///
/// The arm in order: non-`Arr` operand → `"indexing a non-array value"`;
/// negative raw index → `"array index out of bounds"`; `Vm::arr_mut`,
/// which is the copy-on-write promotion of an `ArrRepr::Const` array and
/// can refuse on the byte budget (its `String` is propagated verbatim);
/// `get_mut` past the end → `"array index out of bounds"`.
///
/// Returns nothing. The interpreter pushes the stored value back on the
/// value stack (`a[i] = v` is an expression), but generated code already
/// holds that value in a register and the §3.5 row is a bare `callx8`, so
/// there is nothing to hand back.
///
/// # Safety
/// As [`arr_load_num`].
pub unsafe extern "C" fn arr_store(
    ctx: *mut JitCtx,
    atag: u32,
    apay: u32,
    idx: i32,
    vtag: u32,
    vpay: u32,
) {
    let c = &mut *ctx;
    let vm = &mut *c.vm;
    let prog = &*c.prog;
    let val = value_of(vtag, vpay);
    let Value::Arr(a) = value_of(atag, apay) else {
        return fail_str(c, "indexing a non-array value");
    };
    // `idx` IS `Fx::raw()`, so the interpreter's `idx.raw() < 0` is this.
    if idx < 0 {
        return fail_str(c, "array index out of bounds");
    }
    let i = Fx::from_raw(idx).to_int_trunc() as usize;
    match vm.arr_mut(prog, a) {
        Ok(v) => match v.get_mut(i) {
            Some(slot) => *slot = val,
            None => fail_str(c, "array index out of bounds"),
        },
        Err(m) => fail_str(c, &m),
    }
}

/// `ArrLen` — the `op::ARR_LEN` arm of `Vm::run` (docs/jit-design.md
/// §3.5). `".length of a non-array value"` for a non-`Arr` operand,
/// otherwise the length as a raw 16.16 word. Returns `0` on failure.
///
/// # Safety
/// As [`arr_load_num`].
pub unsafe extern "C" fn arr_len(ctx: *mut JitCtx, atag: u32, apay: u32) -> i32 {
    let c = &mut *ctx;
    let vm = &mut *c.vm;
    let prog = &*c.prog;
    let Value::Arr(a) = value_of(atag, apay) else {
        fail_str(c, ".length of a non-array value");
        return 0;
    };
    Fx::from_int(vm.arr(prog, a).len() as i32).raw()
}

// ------------------------------------------------------- array creation

/// `NewArray n` — the `op::NEW_ARRAY` arm of `Vm::run`
/// (docs/jit-design.md §3.5).
///
/// BUDGET FIRST, exactly as the arm: `Vm::alloc_array_zeroed(n)` runs
/// before a single element is stored, so a refusal leaves the arena
/// untouched. Its `String` (element budget, slot cap, byte budget, OOM) is
/// propagated verbatim. Then element `i` is filled from `vals[i]`.
///
/// Returns the arena id — the payload of the `Value::Arr` the arm pushes —
/// so the emitted code pairs it with a `movi tag, 1`. Returns `0` on
/// failure, which generated code must not read: it tests `ctx.status`.
///
/// # Safety
/// As [`arr_load_num`], and `vals` must address at least `n` [`ValueRaw`]s
/// (it is the frame's boxed-args scratch, §3.3). `n == 0` never
/// dereferences it.
pub unsafe extern "C" fn new_array(ctx: *mut JitCtx, n: u32, vals: *const ValueRaw) -> u32 {
    let c = &mut *ctx;
    let vm = &mut *c.vm;
    let prog = &*c.prog;
    let n = n as usize;
    let v = match vm.alloc_array_zeroed(n) {
        Ok(v) => v,
        Err(m) => {
            fail_str(c, &m);
            return 0;
        }
    };
    // `alloc_array_zeroed` always returns `Value::Arr`; matched rather than
    // `unreachable!()` because a helper must not panic.
    let Value::Arr(id) = v else { return 0 };
    // Freshly allocated ⇒ `ArrRepr::Owned`, so `arr_mut` cannot fail here
    // (no const promotion to pay for). The interpreter reaches the storage
    // through `self.arrays[id]` directly; this is the same slot.
    if let Ok(slots) = vm.arr_mut(prog, id) {
        let mut i = 0;
        while i < n {
            slots[i] = Value::from_raw(*vals.add(i));
            i += 1;
        }
    }
    id
}

/// `ConstArr d` — the `op::CONST_ARR` arm of `Vm::run`
/// (docs/jit-design.md §3.5).
///
/// `len` comes from `prog.pool[d]` and the arena entry SHARES the const
/// pool until first mutation (`Vm::alloc_const_array`); the copy-on-write
/// promotion is [`arr_store`]'s. Returns the arena id, or `0` with the
/// budget error set.
///
/// # Safety
/// As [`arr_load_num`]. `d` is a decoder-validated pool index (the
/// interpreter indexes `prog.pool` the same way), so it must be in range.
pub unsafe extern "C" fn const_arr(ctx: *mut JitCtx, d: u32) -> u32 {
    let c = &mut *ctx;
    let vm = &mut *c.vm;
    let prog = &*c.prog;
    // decoder-validated: d < pool.len()
    let len = prog.pool[d as usize].len as usize;
    let v = match vm.alloc_const_array(d, len) {
        Ok(v) => v,
        Err(m) => {
            fail_str(c, &m);
            return 0;
        }
    };
    let Value::Arr(id) = v else { return 0 };
    id
}

// ------------------------------------------------------------ callvalue

/// [`call_value_target`] resolved a bytecode function: `val` is its NATIVE
/// ENTRY ADDRESS, from `ctx.fn_table`. The emitted code does `callx8`.
pub const CALL_TARGET_NATIVE: i32 = 0;
/// [`call_value_target`] resolved a builtin: `val` is its builtin ID. The
/// emitted code calls `ctx.builtins[id].generic` with the boxed args.
pub const CALL_TARGET_BUILTIN: i32 = 1;
/// [`call_value_target`] refused: the callee is not callable. `ctx.status`
/// is [`crate::jit::STATUS_ERR`] and `*ctx.err` is filled; the emitted
/// code branches to `bail`. **The only outcome that touches `ctx`.**
pub const CALL_TARGET_ERR: i32 = 2;

/// Resolve a `CallValue` callee — the `CallValue` row of
/// docs/jit-design.md §3.5, i.e. the `op::CALL_VALUE` arm of `Vm::run`,
/// MINUS the call itself. See this module's "Deviation from §3.5".
///
/// `Ret2::status` is a DISCRIMINANT, not a status word — the one place in
/// the ABI where it is, which is why the three values have names:
///
/// | `status` | `val` | emitted code |
/// |---|---|---|
/// | [`CALL_TARGET_NATIVE`] (0) | native entry address, `ctx.fn_table[payload]` | `callx8 val` |
/// | [`CALL_TARGET_BUILTIN`] (1) | builtin id | call `ctx.builtins[val].generic` |
/// | [`CALL_TARGET_ERR`] (2) | `0` | branch to `bail` |
///
/// Only outcome 2 sets `ctx.status`; 0 and 1 leave the context alone, so a
/// resolve costs nothing and the caller distinguishes them on `a3` rather
/// than on a context load.
///
/// The `Fun` case is guarded against a `fn_table` overrun (and a null
/// table) and reports that as outcome 2 with the same
/// `"call of a non-function value"` text. The interpreter cannot produce
/// an out-of-range `Value::Fun` — the decoder validates every `ConstFun`
/// index — but a helper handed a corrupt handle must not read wild memory,
/// and the bound is `prog.fns.len()`: `fn_table` has one entry per
/// bytecode function.
///
/// # Safety
/// As [`arr_load_num`], and `ctx.fn_table` must be null or address
/// `prog.fns.len()` words.
pub unsafe extern "C" fn call_value_target(ctx: *mut JitCtx, tag: u32, payload: u32) -> Ret2 {
    let c = &mut *ctx;
    match tag {
        TAG_FUN => {
            let prog = &*c.prog;
            if c.fn_table.is_null() || payload as usize >= prog.fns.len() {
                fail_str(c, "call of a non-function value");
                return Ret2 {
                    val: 0,
                    status: CALL_TARGET_ERR,
                };
            }
            Ret2 {
                val: c.fn_table.add(payload as usize).read() as i32,
                status: CALL_TARGET_NATIVE,
            }
        }
        TAG_BUILTIN => Ret2 {
            val: payload as i32,
            status: CALL_TARGET_BUILTIN,
        },
        // `TAG_NUM` and `TAG_ARR` both land here, matching the arm's `_`.
        _ => {
            fail_str(c, "call of a non-function value");
            Ret2 {
                val: 0,
                status: CALL_TARGET_ERR,
            }
        }
    }
}

// ---------------------------------------------------------------- bails

/// `Assert m` — the `op::ASSERT` arm of `Vm::run`, called when the
/// asserted value is falsy (the emitted code does the truthiness test
/// itself: `Not`/`JmpIfFalse` are two instructions, §3.5). ALWAYS fails.
///
/// The message is `Vm::assert_failed`'s, which is
/// `"pattern requires: {msg} (pixelCount = {px})"` with `is_assert = true`
/// — a declared configuration invariant, which the engine treats
/// differently from a bug (it blocks rendering for the pattern's
/// lifetime). That format string and the `pixelCount` read are NOT
/// restated here: `assert_failed` is called, so the interpreter's text and
/// this one cannot drift. The price is that `assert_failed` builds its
/// `VmError` through `Vm::err_at`, which finds no interpreter frame during
/// native execution and stamps `fn_idx: u16::MAX, pc: u32::MAX`; the site
/// is patched up from the context afterwards (§3.6). Patching four fields
/// is the cheaper half of the trade — the alternative is a second copy of
/// a user-facing string.
///
/// # Safety
/// As [`arr_load_num`].
pub unsafe extern "C" fn assert_fail(ctx: *mut JitCtx, m: u32) {
    let c = &mut *ctx;
    let vm = &mut *c.vm;
    let prog = &*c.prog;
    let mut e = vm.assert_failed(prog, m as u16);
    // Re-attribute: see the note above. `is_assert` and `message` are
    // `assert_failed`'s and are left alone.
    let (line, col) = match prog.fns.get(c.fn_idx as usize) {
        Some(f) => f.pos_at(c.insn_at),
        None => (0, 0),
    };
    e.fn_idx = c.fn_idx;
    e.pc = c.insn_at;
    e.line = line;
    e.col = col;
    c.fail(e);
}

/// The fuel check's bail (docs/jit-design.md §3.6): `ERR_EXEC_LIMIT`,
/// byte for byte the interpreter's `fail!(ERR_EXEC_LIMIT)`, so
/// `VmError::is_resource_guard` keeps classifying it as a resource guard
/// and the engine keeps the frame-fatal blast radius it has today.
///
/// Native code charges fuel at back-edges and calls only, so the budget
/// buys a native loop ~10× more iterations than an interpreted one — the
/// same number, a different exchange rate (§3.6).
///
/// # Safety
/// As [`arr_load_num`].
pub unsafe extern "C" fn bail_fuel(ctx: *mut JitCtx) {
    let c = &mut *ctx;
    fail_str(c, ERR_EXEC_LIMIT);
}

/// The prologue depth check's bail (docs/jit-design.md §3.6). The native
/// stack replaces `MAX_DEPTH`, but the error is the interpreter's
/// `push_frame` text so a runaway recursion reads the same either way.
///
/// # Safety
/// As [`arr_load_num`].
pub unsafe extern "C" fn bail_depth(ctx: *mut JitCtx) {
    let c = &mut *ctx;
    fail_str(c, "call depth exceeded");
}

// A helper address is what the emitter puts in the literal pool, so every
// one of these has to be a real `extern "C"` function item with a stable
// signature — not a generic, not an `#[inline]` that could be elided. The
// assertions are cheap and they are what fails the build if a signature
// drifts from the table above.
const _: () = {
    let _: extern "C" fn(i32, i32) -> i32 = fx_div;
    let _: extern "C" fn(i32, i32) -> i32 = fx_pow;
    let _: unsafe extern "C" fn(*mut JitCtx, u32, u32, i32) -> i32 = arr_load_num;
    let _: unsafe extern "C" fn(*mut JitCtx, u32, u32, i32) -> RetDyn = arr_load_dyn;
    let _: unsafe extern "C" fn(*mut JitCtx, u32, u32, i32, u32, u32) = arr_store;
    let _: unsafe extern "C" fn(*mut JitCtx, u32, u32) -> i32 = arr_len;
    let _: unsafe extern "C" fn(*mut JitCtx, u32, *const ValueRaw) -> u32 = new_array;
    let _: unsafe extern "C" fn(*mut JitCtx, u32) -> u32 = const_arr;
    let _: unsafe extern "C" fn(*mut JitCtx, u32, u32) -> Ret2 = call_value_target;
    let _: unsafe extern "C" fn(*mut JitCtx, u32) = assert_fail;
    let _: unsafe extern "C" fn(*mut JitCtx) = bail_fuel;
    let _: unsafe extern "C" fn(*mut JitCtx) = bail_depth;
};
