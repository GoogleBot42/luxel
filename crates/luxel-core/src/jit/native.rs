//! What the ENGINE needs in order to run a compiled program: the entry
//! addresses, the per-function calling convention, and one object that
//! knows how to make the call (Gitea #658, phase 3 of #607;
//! docs/jit-design.md §6).
//!
//! Deliberately ignorant of the emitter. `luxel-jit` depends on this crate,
//! so nothing here can name [`luxel_jit::FnAbi`] or [`luxel_jit::NativeImage`];
//! the firmware translates one into the other at activation. It is also
//! ignorant of where executable memory comes from — that is a board fact
//! (`.rwtext` static, PSRAM arena, …), so it arrives as an opaque
//! [`ExecLease`] whose `Drop` frees whatever it was.
//!
//! ```text
//!   firmware                     luxel-core (here)          generated code
//!   ────────                     ─────────────────          ──────────────
//!   luxel_jit::compile ──image──► NativeProgram ──enter()──► entry / retw.n
//!   ExecBuf half ──────lease────►      │
//!   XtensaCall ───────caller────►      │
//!                                Engine::render_pixels
//! ```
//!
//! **The call itself is the one place a function pointer is conjured out of
//! an integer**, which is why it is behind the [`NativeCall`] trait rather
//! than written inline in the engine: the device installs a caller that
//! transmutes the address to a typed `extern "C"` pointer, and the host
//! test suite installs one that runs the same bytes through an Xtensa ISA
//! model. The engine's path is then literally the same code under both.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::jit::ctx::JitCtx;

/// How one compiled function is entered — [`luxel_jit::FnAbi`] minus the
/// frame size, which only the callee's own prologue cares about
/// (docs/jit-design.md §3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeAbi {
    /// `ParamConv::Regs`: the parameters ride in `a11…a15` (caller view).
    /// False = `ParamConv::CtxArgs`, every parameter through
    /// [`JitCtx::args`], and the caller passes none in registers.
    pub args_in_regs: bool,
    /// How many parameters the callee declares.
    pub params: u8,
    /// The result comes back in two words (`a10:a11`) rather than one.
    ///
    /// The engine never reads a result — `beforeRender`, `render*` and
    /// `renderFrame` all return into the void — so this is carried for
    /// completeness and for the host tests, not consulted on the call path.
    pub ret_dyn: bool,
    /// Bit `i` set = parameter `i` is `Dyn`, so it takes TWO handoff words
    /// (tag then payload). Only meaningful when `args_in_regs` is false.
    /// See [`ctx_arg_words`].
    pub dyn_params: u32,
}

/// Lay arguments out the way a `ParamConv::CtxArgs` callee's prologue will
/// read them out of [`JitCtx::args`] (docs/jit-design.md §3.2): one word
/// for a non-`Dyn` parameter, two — tag then payload — for a `Dyn` one, in
/// parameter order.
///
/// Fills `out` and returns how many words it used. Arguments are RAW
/// payload words (`Fx::raw()` for a number), so a `Dyn` parameter is boxed
/// here as `Num`: tag [`crate::vm::TAG_NUM`], payload the word.
///
/// One function, two destinations: the device copies `out` into the real
/// `JitCtx`, and the ISA-model harness writes it into model memory. That
/// is the point — a layout disagreement between caller and callee is a
/// wild read of somebody's frame, and there is only one place to get it
/// wrong.
///
/// Words past [`crate::jit::CTX_ARGS`] are dropped rather than written:
/// `plan_all` refuses such a function at compile time
/// (`Refusal::ParamOverflow`), so this cannot be reached with an image the
/// engine actually installed.
pub fn ctx_arg_words(abi: NativeAbi, args: &[i32], out: &mut [i32; crate::jit::CTX_ARGS]) -> usize {
    let mut n = 0usize;
    for i in 0..abi.params as usize {
        let v = args.get(i).copied().unwrap_or(0);
        let is_dyn = i < 32 && abi.dyn_params & (1 << i) != 0;
        if is_dyn {
            if n + 2 > out.len() {
                break;
            }
            out[n] = crate::vm::TAG_NUM as i32;
            out[n + 1] = v;
            n += 2;
        } else {
            if n + 1 > out.len() {
                break;
            }
            out[n] = v;
            n += 1;
        }
    }
    n
}

/// A live claim on executable memory. Dropping it frees the buffer.
///
/// Opaque on purpose: the engine's only interest is that the code stays
/// mapped for exactly as long as the [`NativeProgram`] does, which `Drop`
/// order gives for free.
pub trait ExecLease {}

/// Enter native code.
///
/// # Safety
/// Implementors execute an address as code. `addr` must be an entry point
/// of the image the lease covers, `abi` must be that function's, `ctx` must
/// point at a fully populated [`JitCtx`] whose `vm`/`prog`/`globals`/
/// `err`/`fn_table`/`builtins` are live, and `args` must hold at least
/// `abi.params` words in parameter order.
pub trait NativeCall {
    /// # Safety
    /// See the trait-level contract.
    unsafe fn enter(&self, addr: usize, ctx: *mut JitCtx, abi: NativeAbi, args: &[i32]);
}

/// A whole program compiled to native code, plus the memory it lives in.
///
/// Whole-program or nothing (decision 2, §4a): the presence of one of these
/// on an [`crate::engine::Engine`] means every entry point is native.
pub struct NativeProgram {
    /// Absolute entry address per BYTECODE function index. Doubles as
    /// [`JitCtx::fn_table`], which is what a `CallValue` on a `Fun` handle
    /// resolves through (§3.5) — the two are the same list, so they cannot
    /// disagree.
    pub entries: Vec<usize>,
    /// Calling convention per function, same index order as `entries`.
    pub abi: Vec<NativeAbi>,
    /// Bytes of the image, literal pool included — `/api/status`'s
    /// `jit.code_bytes`.
    pub code_bytes: usize,
    /// What compiling it cost, in microseconds — `jit.compile_us`.
    pub compile_us: u32,
    /// Native stack floor for the prologue's depth guard (§3.6). Below it
    /// a call bails with "call depth exceeded" instead of running off the
    /// render task's stack.
    pub stack_limit: usize,
    /// The thing that actually makes the call.
    pub call: Box<dyn NativeCall>,
    /// Kept alive, never read. Its `Drop` releases the exec buffer, and it
    /// is declared AFTER `call` so the code outlives nothing that could
    /// still enter it.
    pub lease: Box<dyn ExecLease>,
}

impl core::fmt::Debug for NativeProgram {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NativeProgram")
            .field("fns", &self.entries.len())
            .field("code_bytes", &self.code_bytes)
            .field("compile_us", &self.compile_us)
            .finish()
    }
}

impl NativeProgram {
    /// `(entry address, abi)` for a bytecode function index.
    #[inline]
    pub fn entry(&self, fn_idx: u16) -> Option<(usize, NativeAbi)> {
        let i = fn_idx as usize;
        match (self.entries.get(i), self.abi.get(i)) {
            (Some(&a), Some(&abi)) if a != 0 => Some((a, abi)),
            _ => None,
        }
    }
}
