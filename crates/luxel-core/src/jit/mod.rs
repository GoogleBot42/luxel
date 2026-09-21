//! The JIT's ABI surface: the context the generated code is handed, and the
//! builtin entry table it calls through (Gitea #642, phase 1 of #607).
//!
//! docs/jit-design.md §3.2 (calling convention), §3.8 ([`JitCtx`]) and §4
//! (the builtin table) are the spec; this module is that spec as code.
//! **Nothing here emits or executes native code.** Phase 1 ships the data
//! layout — pinned, `const`-asserted and differentially tested against the
//! interpreter on the host — so that phase 2's emitter has a contract to
//! compile against rather than a document to re-read.
//!
//! Four things live here:
//!
//! - [`JitCtx`] — the single pointer every generated function and every
//!   helper carries (`a2` under the windowed ABI). Its field offsets are
//!   [`ctx::OFFSET_VM`] and friends; **the emitter reads those constants
//!   and nothing else** about this struct.
//! - [`BuiltinEntry`] / [`BUILTIN_ENTRIES`] — one entry per builtin id, in
//!   [`crate::vm::BUILTINS`] order (append-only). `generic` is the
//!   any-arity wrapper that runs exactly the interpreter's arm; `direct` is
//!   the raw numeric entry point for the tier-1 set of §3.5.
//! - the §3.5 **helpers** (`helpers.rs`) — the `extern "C"` functions
//!   generated code calls for the rows that need one: indexing, array
//!   creation, `CallValue` resolution, `Div`/`Pow`, and the §3.6 bails.
//!   Each is a thin wrapper over the interpreter's own arm and is
//!   differentially tested against it (`tests/jithelpers.rs`).
//! - [`NativeProgram`] / [`NativeCall`] / [`ExecLease`] (`native.rs`, Gitea
//!   #658) — the ENGINE's half: entry addresses, the per-function calling
//!   convention, and the one object that turns an address into a call.
//!   Nothing here knows the emitter (`luxel-jit` depends on this crate, not
//!   the other way round) or where executable memory comes from.
//! - [`RetDyn`] — a boxed [`crate::vm::Value`] returned by value in two
//!   words (`a2:a3` on Xtensa, §3.2). Errors never travel in the return
//!   value: a failing call sets [`JitCtx::status`] non-zero and fills
//!   `*ctx.err`, so every `generic` wrapper has ONE shape.
//!
//! Compiled under the `jit` feature (on by default, like `kinds`, so the
//! browser, the CLI and `cargo test` all carry it) or under
//! `dispatch-table`. The firmware depends on luxel-core with
//! `default-features = false` and names neither unless a board asks for it,
//! so a board with no backend pays nothing.

pub mod ctx;
mod helpers;
pub mod native;
mod table;
#[cfg(test)]
mod tests;

pub use ctx::{
    dev32, JitCtx, CTX_ARGS, OFFSET_ARGS, OFFSET_BUILTINS, OFFSET_ERR, OFFSET_FN_IDX,
    OFFSET_FN_TABLE, OFFSET_FUEL, OFFSET_GLOBALS, OFFSET_INSN_AT, OFFSET_PROG, OFFSET_STACK_LIMIT,
    OFFSET_STATUS, OFFSET_VM, SIZEOF_JITCTX, STATUS_ERR, STATUS_OK,
};
pub use native::{ctx_arg_words, ExecLease, NativeAbi, NativeCall, NativeProgram};
pub use helpers::{
    arr_len, arr_load_dyn, arr_load_num, arr_store, assert_fail, bail_depth, bail_fuel,
    call_value_target, const_arr, fx_div, fx_pow, new_array, CALL_TARGET_BUILTIN, CALL_TARGET_ERR,
    CALL_TARGET_NATIVE,
};
pub use table::{
    BuiltinEntry, Direct, DirectSig, RetDyn, BUILTIN_ENTRIES, RET_ARG_BASE, RET_DYN,
    RET_NEW_ARRNUM, RET_NUM,
};

/// The two-word fallible return of docs/jit-design.md §3.2: a value plus a
/// status word, returned in `a2:a3` on Xtensa.
///
/// Phase 1 does not use it — every builtin returns [`RetDyn`] and reports
/// through `ctx.status` — but the shape is what §7.1's ABI pin is about,
/// and [`abi_probe_ret2`] is the function the objdump check disassembles.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ret2 {
    pub val: i32,
    pub status: i32,
}

/// The §7.1 ABI probe: a plain `extern "C"` function returning a two-word
/// `#[repr(C)]` struct.
///
/// Its only purpose is to be disassembled. On the host a unit test calls it
/// (`crates/luxel-core/tests/abi_probe.rs`); on Xtensa the question is
/// whether the two words come back in `a2:a3` rather than through a hidden
/// sret pointer, and the answer is read out of the S3 image with
/// `xtensa-esp32s3-elf-objdump`. `#[no_mangle]` so the symbol survives
/// `--gc-sections` and is findable by name; `#[inline(never)]` so there is
/// a body to look at.
#[no_mangle]
#[inline(never)]
pub extern "C" fn lx_abi_probe_ret2(a: i32, b: i32) -> Ret2 {
    Ret2 {
        val: a.wrapping_add(b),
        status: a ^ b,
    }
}

/// Keeps [`lx_abi_probe_ret2`] in the image. `#[no_mangle]` names a symbol;
/// it does NOT stop `--gc-sections` from dropping a function no one calls,
/// and the first S3 build of #642 dropped it — the probe has to be
/// REFERENCED by something the linker keeps. Twelve bytes, in a build that
/// carries the JIT at all.
#[used]
static LX_ABI_PROBE_KEEP: extern "C" fn(i32, i32) -> Ret2 = lx_abi_probe_ret2;
