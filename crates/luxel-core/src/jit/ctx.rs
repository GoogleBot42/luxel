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
    /// `Vm::globals`'s first element (Gitea #651, phase 2). docs/jit-design.md
    /// §3.8 says a `LoadG`/`StoreG` on a typed global is an `l32i`/`s32i`
    /// into "a `#[repr(C)]` globals array the helper side shares" — and
    /// since #642 pinned [`Value`] as `#[repr(C, u32)]`, `Vm::globals`
    /// ALREADY is that array: 8 bytes per global, tag at +0, payload at +4.
    /// What was missing was a way for generated code to find it, because
    /// `Vm` itself is not `repr(C)` and so cannot be offset into. Hence this
    /// field, appended so every offset above is unchanged.
    ///
    /// # Invariant
    /// `Vm::globals` is sized once by `Vm::new` and never grows, so the
    /// pointer stays valid for the VM's life. A helper that ever resized it
    /// would dangle this — nothing does, and nothing may.
    pub globals: *mut crate::vm::ValueRaw,
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
/// Byte offset of [`JitCtx::globals`].
pub const OFFSET_GLOBALS: usize = core::mem::offset_of!(JitCtx, globals);
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
    const _: () = assert!(OFFSET_GLOBALS == 176);
    const _: () = assert!(SIZEOF_JITCTX == 180);
    // every offset generated code encodes is inside `l32i`'s reach
    const _: () = assert!(SIZEOF_JITCTX <= 1020);
    // …and `dev32` is the same layout, which is the point of that module.
    const _: () = assert!(super::dev32::VM as usize == OFFSET_VM);
    const _: () = assert!(super::dev32::PROG as usize == OFFSET_PROG);
    const _: () = assert!(super::dev32::STATUS as usize == OFFSET_STATUS);
    const _: () = assert!(super::dev32::INSN_AT as usize == OFFSET_INSN_AT);
    const _: () = assert!(super::dev32::FN_IDX as usize == OFFSET_FN_IDX);
    const _: () = assert!(super::dev32::FUEL as usize == OFFSET_FUEL);
    const _: () = assert!(super::dev32::STACK_LIMIT as usize == OFFSET_STACK_LIMIT);
    const _: () = assert!(super::dev32::ARGS as usize == OFFSET_ARGS);
    const _: () = assert!(super::dev32::ERR as usize == OFFSET_ERR);
    const _: () = assert!(super::dev32::FN_TABLE as usize == OFFSET_FN_TABLE);
    const _: () = assert!(super::dev32::BUILTINS as usize == OFFSET_BUILTINS);
    const _: () = assert!(super::dev32::GLOBALS as usize == OFFSET_GLOBALS);
    const _: () = assert!(super::dev32::SIZEOF as usize == SIZEOF_JITCTX);
    const _: () =
        assert!(super::dev32::BUILTIN_ENTRY as usize == core::mem::size_of::<BuiltinEntry>());
    const _: () =
        assert!(super::dev32::ENTRY_DIRECT as usize == core::mem::offset_of!(BuiltinEntry, direct));
    const _: () = assert!(
        super::dev32::ENTRY_DIRECT_SIG as usize == core::mem::offset_of!(BuiltinEntry, direct_sig)
    );
    const _: () = assert!(
        super::dev32::ENTRY_RET_KIND as usize == core::mem::offset_of!(BuiltinEntry, ret_kind)
    );
    const _: () = assert!(
        super::dev32::FN_TABLE_STRIDE as usize == core::mem::size_of::<usize>()
    );
}

/// **The layout the EMITTER compiles for**, as literal numbers.
///
/// The `OFFSET_*` constants above are `offset_of!` on the HOST, and the
/// emitter runs on a 64-bit host in every test and every CI run — where
/// `OFFSET_PROG` is 8, not 4. Generated code must carry the 32-bit device
/// offsets whatever it was generated on, so those are spelled here as
/// literals, and the `pinned32` module above asserts the two agree on the
/// one target where both are meaningful. Change a field and the device
/// build fails; change it without touching this module and the device build
/// fails too.
pub mod dev32 {
    pub const VM: u32 = 0;
    pub const PROG: u32 = 4;
    pub const STATUS: u32 = 8;
    pub const INSN_AT: u32 = 12;
    pub const FN_IDX: u32 = 16;
    pub const FUEL: u32 = 20;
    pub const STACK_LIMIT: u32 = 24;
    pub const ARGS: u32 = 28;
    pub const ERR: u32 = 164;
    pub const FN_TABLE: u32 = 168;
    pub const BUILTINS: u32 = 172;
    pub const GLOBALS: u32 = 176;
    pub const SIZEOF: u32 = 180;

    /// `size_of::<BuiltinEntry>()` on the device: two words plus the two
    /// `u8` columns, padded to the 4-byte alignment of a function pointer.
    pub const BUILTIN_ENTRY: u32 = 12;
    /// `offset_of!(BuiltinEntry, generic)`.
    pub const ENTRY_GENERIC: u32 = 0;
    /// `offset_of!(BuiltinEntry, direct)`.
    pub const ENTRY_DIRECT: u32 = 4;
    /// `offset_of!(BuiltinEntry, direct_sig)`.
    pub const ENTRY_DIRECT_SIG: u32 = 8;
    /// `offset_of!(BuiltinEntry, ret_kind)`.
    pub const ENTRY_RET_KIND: u32 = 9;

    /// `size_of::<Value>()` — the same on every target (#642 pinned it).
    pub const VALUE: u32 = 8;
    /// `offset_of!(ValueRaw, tag)`.
    pub const VALUE_TAG: u32 = 0;
    /// `offset_of!(ValueRaw, payload)`.
    pub const VALUE_PAYLOAD: u32 = 4;

    /// One `usize` of `JitCtx::fn_table`.
    pub const FN_TABLE_STRIDE: u32 = 4;
}

// `Value`'s layout is target-independent, so these can be asserted
// everywhere rather than only on a 32-bit build.
const _: () = assert!(dev32::VALUE as usize == core::mem::size_of::<crate::vm::ValueRaw>());
const _: () =
    assert!(dev32::VALUE_TAG as usize == core::mem::offset_of!(crate::vm::ValueRaw, tag));
const _: () =
    assert!(dev32::VALUE_PAYLOAD as usize == core::mem::offset_of!(crate::vm::ValueRaw, payload));
const _: () = assert!(dev32::ENTRY_GENERIC as usize == core::mem::offset_of!(BuiltinEntry, generic));

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
        let globals = vm.globals.as_mut_ptr().cast::<crate::vm::ValueRaw>();
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
            globals,
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
