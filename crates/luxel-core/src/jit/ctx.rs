//! [`JitCtx`] — the one pointer generated code carries (docs/jit-design.md
//! §3.8), and the field offsets the emitter encodes into `l32i`/`s32i`.

use crate::jit::table::BuiltinEntry;
use crate::vm::{Program, Vm, VmError};

/// [`JitCtx::status`] after a call that succeeded.
pub const STATUS_OK: i32 = 0;
/// [`JitCtx::status`] after a call that raised a runtime error. Generated
/// code does `l32i t, ctx, OFFSET_STATUS; bnez t, bail` after every
/// fallible call (§3.6); the error itself is in `*ctx.err`.
pub const STATUS_ERR: i32 = 1;

/// Handoff slots for arguments that do not fit the five register params:
/// docs/jit-design.md §3.2. A `Dyn` argument takes two, and `MAX_ARGS`
/// boxed arguments plus the register spill is the worst case, so 34 words.
pub const CTX_ARGS: usize = 34;

/// The context every native function and every helper is handed in `a2`.
///
/// **`#[repr(C)]` and the `OFFSET_*` constants below are the contract.**
/// The emitter (phase 2) knows this struct only through those constants; it
/// never reads a Rust type. Field ORDER is therefore free to change as long
/// as the constants move with it — they are derived with
/// [`core::mem::offset_of`], and the 32-bit (device) layout is additionally
/// pinned by literal `const` assertions at the bottom of this file so that a
/// reorder is a build failure and not a silent ABI break.
///
/// Two fields deviate from the §3.8 sketch, both deliberately:
///
/// - **`err` is out of line.** [`VmError`] owns a `String`, so it is
///   neither `repr(C)` nor a fixed size, and putting it in this struct
///   would make every offset after it rustc's choice. `err` is instead a
///   `*mut Option<VmError>` pointing at a slot the CALLER owns (the
///   interpreter's stack frame, or the engine's activation), which helpers
///   fill. Generated code never touches it: it only tests `status`.
/// - **`prog` was added.** The generic builtin wrappers run the
///   interpreter's own arms, and those take `&Program` — the constant pool
///   an `ArrRepr::Const` array reads through, the assert messages. There is
///   nowhere else to get it from: `Vm` does not hold one (a `Program` can
///   be a borrowed `'static` flash slot, `Words::Static`). It sits next to
///   `vm` because the two are the same fact: "the engine this code belongs
///   to".
#[repr(C)]
pub struct JitCtx {
    /// The VM whose globals, arrays, brush and RNG the helpers work on.
    pub vm: *mut Vm,
    /// The program being executed — the constant pool and assert messages
    /// the interpreter's builtin arms read. See the type-level note.
    pub prog: *const Program,
    /// [`STATUS_OK`] / [`STATUS_ERR`]. Every fallible helper writes it;
    /// generated code branches on it.
    pub status: i32,
    /// Fn-relative WORD index of the instruction being executed, stored
    /// before each fallible helper call so `Vm::err_at` attributes exactly
    /// as the interpreter does (§3.6).
    pub insn_at: u32,
    /// Index of the running function, for the same attribution.
    pub fn_idx: u16,
    /// Explicit padding: `fn_idx` is a `u16` in a `u32` slot, and the
    /// emitter's offsets must not depend on how rustc would pack it.
    pub _pad: u16,
    /// Remaining instruction budget, charged at back-edges and calls (§3.6).
    pub fuel: i32,
    /// Native stack floor; the prologue's depth check (§3.6) compares `a1`
    /// against it.
    pub stack_limit: usize,
    /// Argument handoff area for params beyond the five register slots and
    /// for every `Dyn` param (§3.2). Live only between a call instruction
    /// and the callee's prologue copy, which is what keeps recursion safe.
    pub args: [i32; CTX_ARGS],
    /// Where a failing helper puts its [`VmError`]. Caller-owned; never
    /// null while native code runs. See the type-level note.
    pub err: *mut Option<VmError>,
    /// Native entry point per bytecode function index, for `CallValue`
    /// resolving a `Fun` handle (§3.5).
    pub fn_table: *const usize,
    /// [`crate::jit::BUILTIN_ENTRIES`], so a `CallBuiltin` is one load plus
    /// an indirect call rather than an `l32r` of a global per call site.
    pub builtins: *const BuiltinEntry,
}

// ---------------------------------------------------------------- offsets

/// Byte offset of [`JitCtx::vm`] — what the emitter encodes.
pub const OFFSET_VM: usize = core::mem::offset_of!(JitCtx, vm);
/// Byte offset of [`JitCtx::prog`].
pub const OFFSET_PROG: usize = core::mem::offset_of!(JitCtx, prog);
/// Byte offset of [`JitCtx::status`].
pub const OFFSET_STATUS: usize = core::mem::offset_of!(JitCtx, status);
/// Byte offset of [`JitCtx::insn_at`].
pub const OFFSET_INSN_AT: usize = core::mem::offset_of!(JitCtx, insn_at);
/// Byte offset of [`JitCtx::fn_idx`].
pub const OFFSET_FN_IDX: usize = core::mem::offset_of!(JitCtx, fn_idx);
/// Byte offset of [`JitCtx::fuel`].
pub const OFFSET_FUEL: usize = core::mem::offset_of!(JitCtx, fuel);
/// Byte offset of [`JitCtx::stack_limit`].
pub const OFFSET_STACK_LIMIT: usize = core::mem::offset_of!(JitCtx, stack_limit);
/// Byte offset of [`JitCtx::args`].
pub const OFFSET_ARGS: usize = core::mem::offset_of!(JitCtx, args);
/// Byte offset of [`JitCtx::err`].
pub const OFFSET_ERR: usize = core::mem::offset_of!(JitCtx, err);
/// Byte offset of [`JitCtx::fn_table`].
pub const OFFSET_FN_TABLE: usize = core::mem::offset_of!(JitCtx, fn_table);
/// Byte offset of [`JitCtx::builtins`].
pub const OFFSET_BUILTINS: usize = core::mem::offset_of!(JitCtx, builtins);
/// Size of the whole context, for whoever allocates one.
pub const SIZEOF_JITCTX: usize = core::mem::size_of::<JitCtx>();

// Every offset is 4-aligned (every field is a word or wider), which is what
// `l32i`/`s32i`'s scaled immediate needs, and no field overlaps `_pad`.
const _: () = assert!(OFFSET_VM == 0);
const _: () = assert!(OFFSET_STATUS % 4 == 0);
const _: () = assert!(OFFSET_INSN_AT % 4 == 0);
const _: () = assert!(OFFSET_FUEL % 4 == 0);
const _: () = assert!(OFFSET_ARGS % 4 == 0);
const _: () = assert!(core::mem::offset_of!(JitCtx, _pad) == OFFSET_FN_IDX + 2);
const _: () = assert!(OFFSET_FUEL == OFFSET_FN_IDX + 4);

// The DEVICE layout, pinned by literal numbers. This is the one the
// emitter compiles for; if a field is reordered or resized these fail and
// the fix is to re-read docs/jit-design.md §3.8, not to edit the numbers.
// `l32i`'s unsigned scaled immediate reaches 0..=1020, so every offset a
// prologue or a fuel check uses must stay well inside that — `args` at 28
// and `err`/`fn_table`/`builtins` at 164/168/172 all do.
#[cfg(target_pointer_width = "32")]
mod pinned32 {
    use super::*;
    const _: () = assert!(OFFSET_VM == 0);
    const _: () = assert!(OFFSET_PROG == 4);
    const _: () = assert!(OFFSET_STATUS == 8);
    const _: () = assert!(OFFSET_INSN_AT == 12);
    const _: () = assert!(OFFSET_FN_IDX == 16);
    const _: () = assert!(OFFSET_FUEL == 20);
    const _: () = assert!(OFFSET_STACK_LIMIT == 24);
    const _: () = assert!(OFFSET_ARGS == 28);
    const _: () = assert!(OFFSET_ERR == 164);
    const _: () = assert!(OFFSET_FN_TABLE == 168);
    const _: () = assert!(OFFSET_BUILTINS == 172);
    const _: () = assert!(SIZEOF_JITCTX == 176);
    // every offset generated code encodes is inside `l32i`'s reach
    const _: () = assert!(SIZEOF_JITCTX <= 1020);
}

impl JitCtx {
    /// A context for a bare builtin call — no native frame, no fuel
    /// accounting, no function table. This is what the `dispatch-table`
    /// interpreter variant and the host parity tests build: everything the
    /// `generic` wrappers read (`vm`, `prog`, `status`, `err`) and nothing
    /// else.
    ///
    /// # Safety of the pointers
    /// The returned context borrows `vm`, `prog` and `err` as raw pointers.
    /// It must not outlive them, and nothing else may touch `*vm` while a
    /// wrapper is running through it — exactly the aliasing rule the
    /// interpreter already keeps for `&mut self`.
    pub fn for_builtin_call(vm: &mut Vm, prog: &Program, err: &mut Option<VmError>) -> JitCtx {
        JitCtx {
            vm,
            prog,
            status: STATUS_OK,
            insn_at: 0,
            fn_idx: u16::MAX,
            _pad: 0,
            fuel: 0,
            stack_limit: 0,
            args: [0; CTX_ARGS],
            err,
            fn_table: core::ptr::null(),
            builtins: crate::jit::BUILTIN_ENTRIES.as_ptr(),
        }
    }

    /// Record a runtime error and set [`STATUS_ERR`].
    ///
    /// # Safety
    /// `self.err` must point at a live `Option<VmError>`.
    #[inline]
    pub unsafe fn fail(&mut self, e: VmError) {
        *self.err = Some(e);
        self.status = STATUS_ERR;
    }
}
