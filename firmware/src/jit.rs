//! On-device JIT: compile at activation, run the engine's entries natively
//! (Gitea #658, phase 3 of #607; docs/jit-design.md §5 "Executable memory
//! and lifecycle", §6 "Engine integration").
//!
//! Three things live here, and nothing else in the firmware knows about
//! native code at all:
//!
//! 1. **[`ExecBuf`]** — the executable RAM. ONE implementation this phase:
//!    a `#[link_section = ".rwtext"]` static, split into [`HALVES`] so a
//!    crossfade can hold the outgoing and incoming images at once. `.rwtext`
//!    is instruction-bus RAM on every Xtensa board we build (SRAM0 on the
//!    classic ESP32, the unified SRAM on the S3), written with 32-bit stores
//!    through its own address and fenced with one `isync`. §5's PSRAM arena
//!    is NOT built yet — see "JIT" in docs/firmware.md for why and what it
//!    would buy.
//! 2. **[`XtensaCall`]** — the one place in the tree where an integer
//!    becomes a function pointer. Every `FnAbi` shape the emitter produces
//!    has a typed `extern "C"` signature here; there is no inline assembly
//!    on the call path.
//! 3. **[`try_compile`]** — the activation hook. Whole-program or nothing,
//!    and a refusal is never a failed pattern: the interpreter runs it,
//!    with the reason in `/api/status`'s `jit.reason`.
//!
//! ```text
//!   try_budgeted_engine
//!        │  engine built, init has run
//!        ▼
//!   jit::try_compile ─refusal─► interpreter, jit.state = "interp"
//!        │ ok
//!        ├─ luxel_jit::compile(prog, kinds, env)   pure, no device
//!        ├─ ExecBuf::write  (32-bit stores + isync)
//!        └─ Engine::install_native(NativeProgram { entries, abi, … })
//!                 │
//!                 ▼
//!           render_pixels → XtensaCall::enter → entry / retw.n
//! ```

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use embassy_time::Instant;
use esp_println::println;

use luxel_core::engine::Engine;
use luxel_core::jit::{
    ctx_arg_words, ExecLease, JitCtx, NativeAbi, NativeCall, NativeProgram, CTX_ARGS,
};
use luxel_jit::{Env, Helpers, Refusal};

// ---------------------------------------------------------------- sizing

// `JIT_STATIC_KB` — executable RAM the JIT owns, per chip, and the one
// number that trades native code against everything else `.rwtext` shares.
// It is PER BOARD because the two Xtensa parts have opposite constraints:
// the classic ESP32's `.rwtext` is a dedicated instruction region and costs
// only flash image, while the S3's is the same unified SRAM `.stack` comes
// out of, and docs/firmware.md floors that at 24 KB. Both numbers below are
// measured, not chosen; docs/boards.md's JIT column carries them too, and
// `cargo test -p luxel-jit --test compile_all -- --nocapture` prints how
// much of `library/` a given cap covers (307 patterns, mean image 3.4 KB,
// 32 KB worst case). A pattern over the cap refuses with `too-large` and is
// interpreted — a correct outcome, not a failure.

/// Classic ESP32: 24 KB. SRAM0 is a DEDICATED 128 KB instruction region,
/// separate from the DRAM `.stack` comes out of, so the buffer costs flash
/// image and no stack at all — the binding limit is the region itself.
/// Measured on `board-esp32-generic` + `jit`, 2026-09-21: `.rwtext` 67,224
/// B and `.rwtext.wifi` 51,800 B of 131,072, leaving ~12 KB spare.
///
/// 12 KB per half covers 96 % of `library/`. It is bigger than the S3's
/// not because this board matters more — it ships interpreter-only — but
/// because it is the board QEMU models, so a cap that refused the §7.1
/// patterns would blind the only gate that EXECUTES emitted code
/// (`tools/qemu/jit-test.py`).
#[cfg(feature = "esp32")]
pub const JIT_STATIC_KB: usize = 24;

/// **S3: 14 KB, and it is bought, not found.** `.rwtext` and `.stack` are
/// the same unified SRAM here, so every byte of this buffer is a byte of
/// render-task stack — and docs/firmware.md floors that at 24 KB.
///
/// Measured on `board-seengreat-hub75`, 2026-09-21:
///
/// | build | `.rwtext` | `.stack` |
/// |---|---|---|
/// | interpreter, `iram-vm` (shipped before #658) | 30,152 B | 27,364 B |
/// | + a 16 KB exec buffer | 46,032 B | 9,924 B — rejected |
/// | `iram-vm` traded for a 14 KB buffer | 30,472 B | 25,484 B |
///
/// The trade is `board-target.sh`'s: `iram-vm` is `Vm::run`, the
/// interpreter's per-pixel loop, which a natively-compiled pattern never
/// enters. Its 13,572 B plus the 1.7 KB `.rwtext` genuinely had spare is
/// what pays for this.
///
/// 7 KB per half covers most of `library/` (mean image 3.4 KB, median
/// well under it — `cargo test -p luxel-jit --test compile_all --
/// --nocapture` prints the distribution); the tail refuses with
/// `too-large` and interprets, which is a correct outcome and not a
/// failure. docs/jit-design.md §5's PSRAM arena is what lifts the cap —
/// the Seengreat's 8 MB is already mapped and costs no internal SRAM —
/// and it is the phase-3b work these numbers argue for.
#[cfg(not(feature = "esp32"))]
pub const JIT_STATIC_KB: usize = 14;

/// Images the buffer holds at once. Two, because a crossfade keeps the
/// outgoing engine alive while the incoming one renders (`drop_prev` frees
/// the outgoing half).
pub const HALVES: usize = 2;

/// Bytes per image. This is `Env::max_code`, i.e. `JIT_MAX_CODE` of §5 —
/// over it, [`Refusal::TooLarge`] and the interpreter runs the pattern.
pub const HALF_BYTES: usize = JIT_STATIC_KB * 1024 / HALVES;
const HALF_WORDS: usize = HALF_BYTES / 4;

/// The executable buffer.
///
/// `static mut` rather than a `Mutex<RefCell<…>>`: this memory is written
/// by the render task and then EXECUTED, so there is no safe abstraction
/// that describes it. Access is serialised by [`TAKEN`] — one lease per
/// half, handed out once and released by `Drop`.
#[link_section = ".rwtext"]
static mut EXEC: [[u32; HALF_WORDS]; HALVES] = [[0; HALF_WORDS]; HALVES];

/// Which halves are leased. `false` = free.
static TAKEN: [AtomicBool; HALVES] = [AtomicBool::new(false), AtomicBool::new(false)];

/// A claim on one half of [`EXEC`]. Dropping it frees the half; the
/// [`NativeProgram`] that owns it is dropped with the engine, so the code
/// is released exactly when nothing can enter it any more.
struct Half(usize);

impl Drop for Half {
    fn drop(&mut self) {
        TAKEN[self.0].store(false, Ordering::Release);
    }
}

impl ExecLease for Half {}

/// Take a free half, or `None` when both are in flight (a crossfade
/// starting while another has not finished).
fn claim() -> Option<Half> {
    (0..HALVES)
        .find(|&i| {
            TAKEN[i]
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        })
        .map(Half)
}

/// Execution address of a half's first word. `.rwtext` is mapped on the
/// instruction bus, so the address the code is WRITTEN at is the address it
/// RUNS at — no DBUS/IBUS translation and no cache maintenance, which is
/// the whole reason this is the phase-3 implementation.
fn half_addr(idx: usize) -> usize {
    // SAFETY: taking the address of a static; nothing is read or written.
    unsafe { core::ptr::addr_of!(EXEC[idx]) as usize }
}

/// Copy an image into a half and make it fetchable.
///
/// 32-bit stores only: on the classic ESP32, SRAM0 is instruction-bus
/// memory and a sub-word access to it faults (`LoadStoreError`). `isync`
/// then discards whatever the core prefetched from these addresses —
/// there is no data cache in front of internal SRAM on either chip, so
/// that is the entire coherency step.
///
/// # Safety
/// `idx` must be a held lease and `words` must fit [`HALF_WORDS`].
unsafe fn write_half(idx: usize, words: &[u32]) {
    debug_assert!(words.len() <= HALF_WORDS);
    let base = core::ptr::addr_of_mut!(EXEC[idx]) as *mut u32;
    for (i, w) in words.iter().enumerate() {
        base.add(i).write_volatile(*w);
    }
    // The one `asm!` in the JIT, and it takes no operands: there is no
    // intrinsic for `isync` and nothing else orders an instruction fetch
    // against the stores above.
    #[cfg(target_arch = "xtensa")]
    core::arch::asm!("isync", options(nostack, nomem, preserves_flags));
}

// ------------------------------------------------------------ the call

/// The `FnAbi` shapes the emitter produces, as typed `extern "C"` function
/// pointers (docs/jit-design.md §3.2).
///
/// Generated functions are ordinary windowed-ABI functions — `entry a1, F`
/// on entry, `retw.n` on exit — which is exactly what Rust's `extern "C"`
/// emits a `callx8` for on these targets, so the call needs no assembly.
///
/// **The return value is deliberately typed `i32` for every shape,
/// `ret_dyn` included.** The engine never reads one: `beforeRender`,
/// `render*` and `renderFrame` all return into the void. A two-word return
/// comes back in `a10:a11` with no hidden `sret` pointer (§7.1's ABI
/// probe), so a caller that declares one word and ignores `a11` is ABI
/// compatible with both shapes — and declaring the narrower one means the
/// call site cannot accidentally depend on a register the callee may not
/// have written.
mod abi {
    use luxel_core::jit::JitCtx;
    pub type F0 = unsafe extern "C" fn(*mut JitCtx) -> i32;
    pub type F1 = unsafe extern "C" fn(*mut JitCtx, i32) -> i32;
    pub type F2 = unsafe extern "C" fn(*mut JitCtx, i32, i32) -> i32;
    pub type F3 = unsafe extern "C" fn(*mut JitCtx, i32, i32, i32) -> i32;
    pub type F4 = unsafe extern "C" fn(*mut JitCtx, i32, i32, i32, i32) -> i32;
    pub type F5 = unsafe extern "C" fn(*mut JitCtx, i32, i32, i32, i32, i32) -> i32;
}

/// Enters compiled Xtensa code.
///
/// Installed on every [`NativeProgram`] the device builds; the host tests
/// install an ISA-model caller instead, so `Engine`'s path is the same code
/// in both (see `crates/luxel-jit/tests/engine_diff.rs`).
pub struct XtensaCall;

impl NativeCall for XtensaCall {
    unsafe fn enter(&self, addr: usize, ctx: *mut JitCtx, a: NativeAbi, args: &[i32]) {
        let p = addr as *const ();
        let g = |i: usize| args.get(i).copied().unwrap_or(0);
        if !a.args_in_regs {
            // `ParamConv::CtxArgs`: every parameter travels through the
            // handoff area and the callee's prologue copies it into its
            // frame before anything else runs (§3.2).
            let mut buf = [0i32; CTX_ARGS];
            let n = ctx_arg_words(a, args, &mut buf);
            // `addr_of_mut!` rather than `(*ctx).args[..n]`: indexing would
            // autoref through the raw pointer, which is a reference to
            // memory generated code is about to read behind our back.
            let dst = core::ptr::addr_of_mut!((*ctx).args) as *mut i32;
            core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, n);
            core::mem::transmute::<*const (), abi::F0>(p)(ctx);
            return;
        }
        match a.params {
            0 => core::mem::transmute::<*const (), abi::F0>(p)(ctx),
            1 => core::mem::transmute::<*const (), abi::F1>(p)(ctx, g(0)),
            2 => core::mem::transmute::<*const (), abi::F2>(p)(ctx, g(0), g(1)),
            3 => core::mem::transmute::<*const (), abi::F3>(p)(ctx, g(0), g(1), g(2)),
            4 => core::mem::transmute::<*const (), abi::F4>(p)(ctx, g(0), g(1), g(2), g(3)),
            // `ParamConv::Regs` tops out at five parameters (§3.9).
            _ => core::mem::transmute::<*const (), abi::F5>(p)(ctx, g(0), g(1), g(2), g(3), g(4)),
        };
    }
}

// --------------------------------------------------------------- status

/// `/api/status`'s `jit.state`.
pub const STATE_OFF: u8 = 0;
/// Running the interpreter although the feature is built in — `jit.reason`
/// says why.
pub const STATE_INTERP: u8 = 1;
/// The live pattern is compiled.
pub const STATE_NATIVE: u8 = 2;

static STATE: AtomicU8 = AtomicU8::new(STATE_INTERP);
static CODE_BYTES: AtomicU32 = AtomicU32::new(0);
static COMPILE_US: AtomicU32 = AtomicU32::new(0);
/// Index into [`REASONS`]; `u8::MAX` = none.
static REASON: AtomicU8 = AtomicU8::new(u8::MAX);

/// The `jit.reason` vocabulary. `Refusal::id()` is the emitter's half of
/// it (docs/jit-design.md §4a) and the browser's compile-time lint uses the
/// same spellings (`luxel_core::jitlint::JitRefusal::id`), so one word
/// means one thing wherever it is read.
///
/// The three device-only reasons — `debug`, `init-error`, `no-buffer` —
/// are the ones no compile-time lint can predict, which is exactly why
/// §4a routes them through `/api/status` instead.
const REASONS: [&str; 15] = [
    "unsupported",
    "too-large",
    "l32r-reach",
    "frame-size",
    "kinds",
    "param-overflow",
    "offset-reach",
    "scratch",
    "address-region",
    "jump-reach",
    "untyped",
    "debug",
    "init-error",
    "no-buffer",
    "disabled",
];

fn reason_index(id: &str) -> u8 {
    REASONS.iter().position(|r| *r == id).unwrap_or(0) as u8
}

fn set_state(state: u8, reason: Option<&str>, bytes: usize, us: u32) {
    REASON.store(reason.map_or(u8::MAX, reason_index), Ordering::Relaxed);
    CODE_BYTES.store(bytes as u32, Ordering::Relaxed);
    COMPILE_US.store(us, Ordering::Relaxed);
    STATE.store(state, Ordering::Release);
}

/// `(state, reason, code_bytes, compile_us)` for `/api/status`
/// (docs/api.md). `state` is the wire string, not the constant: the JSON
/// vocabulary is the one thing outside this module, so it is spelled once,
/// here.
pub fn jit_status() -> (&'static str, Option<&'static str>, u32, u32) {
    let r = REASON.load(Ordering::Relaxed);
    let state = match STATE.load(Ordering::Acquire) {
        STATE_NATIVE => "native",
        STATE_OFF => "off",
        _ => "interp",
    };
    (
        state,
        REASONS.get(r as usize).copied(),
        CODE_BYTES.load(Ordering::Relaxed),
        COMPILE_US.load(Ordering::Relaxed),
    )
}

/// Runtime switch: the next activation compiles only if this is set.
///
/// **It defaults to OFF, and that is the phase-3 landing state, not an
/// oversight.** Every gate is green: 307 of 307 library patterns render
/// bit-identical frames through a real `Engine`
/// (`crates/luxel-jit/tests/engine_diff.rs`), and `tools/qemu/jit-test.py`
/// runs emitted code on an emulated ESP32 and compares the published
/// frame against the same image interpreting.
///
/// It is off because docs/jit-design.md §7.3 has not run: **no S3 has
/// executed a byte of this.** A crash on the render core of a board with
/// no serial port is the worst failure mode this project has, and the
/// QEMU gate has already caught one bug that every host gate passed
/// (docs/firmware.md, "The trap the QEMU gate caught"), which is the
/// argument for making the first on-metal run a deliberate act with
/// someone watching the panel rather than something that happens on the
/// next OTA. `POST /api/jit {"on":true}` turns it on for one session;
/// `JIT_OFF=1` at build time removes it from the image entirely.
///
/// `#[no_mangle]` so it is addressable by symbol from outside the running
/// image. That is not decoration — it is how `tools/qemu/jit-test.py`
/// flips the JIT on a board with no network, by writing one byte through
/// QEMU's gdbstub.
#[no_mangle]
pub static LUXEL_JIT_ENABLED: AtomicBool = AtomicBool::new(false);

/// Keeps [`LUXEL_JIT_ENABLED`] in the image: `#[no_mangle]` names a symbol,
/// it does not stop `--gc-sections` dropping one nothing references (the
/// same trap `lx_abi_probe_ret2` hit in #642).
#[used]
static LUXEL_JIT_ENABLED_KEEP: &AtomicBool = &LUXEL_JIT_ENABLED;

/// Set the runtime switch. Takes effect at the next activation — a live
/// pattern keeps running as whatever it was compiled as, which is the only
/// answer that cannot tear a frame.
pub fn set_enabled(on: bool) {
    LUXEL_JIT_ENABLED.store(on, Ordering::Relaxed);
}

/// Read the runtime switch.
pub fn enabled() -> bool {
    LUXEL_JIT_ENABLED.load(Ordering::Relaxed)
}

// ----------------------------------------------------------- stack floor

/// How much of the render task's stack native code may spend before the
/// prologue's depth guard fires (docs/jit-design.md §3.6 — the native stack
/// replaces the interpreter's `MAX_DEPTH`).
///
/// The AppCpu stack is 20 KB (`core1::STACK_BYTES`) and the render task's
/// own Rust frames are already on it when a pattern is entered, so this is
/// a slice of what is left, not the whole stack.
const STACK_BUDGET: usize = 8 * 1024;
/// Never let the guard sit closer than this to the real bottom of the
/// stack, whatever the budget says.
///
/// **8 KB, because the guard only bounds NATIVE frames.** The Rust a
/// native function calls — a builtin wrapper, a §3.5 helper — pushes its
/// own frame below `a1` with no check of its own, and the bulk/canvas
/// wrappers are among the biggest frames in the image
/// (`tools/stack-check.py` lists them). So the reserve has to cover the
/// deepest Rust frame reachable from native code, not just "a bit"; the
/// 2 KB this started at was chosen without measuring.
///
/// It was NOT the cause of the crash `tools/qemu/jit-test.py` found —
/// raising it changed nothing, and the real cause was the window save
/// area at the wrong end of the frame (`luxel_jit::plan::WINDOW_SAVE`,
/// docs/firmware.md "The trap the QEMU gate caught"). It is kept at 8 KB
/// on its own merits.
const STACK_RESERVE: usize = 8 * 1024;

/// The floor the prologue compares `a1` against.
///
/// Two bounds, and the tighter (higher) one wins:
///
/// - **[`STACK_BUDGET`] below the stack pointer here.** Sound on any task
///   whose remaining stack exceeds the budget, and knowable without asking
///   the executor anything — which matters because the render task is an
///   embassy task and embassy tasks do not own stacks, they borrow the
///   thread's.
/// - **`core1::stack_floor() + `[`STACK_RESERVE`].** The real bottom, on
///   the dual-core boards where the render task runs on the AppCpu's own
///   heap-leaked stack and `core1` knows where it starts.
///
/// Computed at ACTIVATION, on the render task, and the render task's stack
/// depth at a pattern entry is the same at activation as it is at frame
/// time (both are called from the same place in the frame loop), so the
/// first bound is measured where it will be used.
fn stack_limit() -> usize {
    let probe = 0u32;
    let here = &probe as *const u32 as usize;
    let budget = here.saturating_sub(STACK_BUDGET);
    match crate::core1::stack_floor() {
        Some(f) => budget.max(f + STACK_RESERVE),
        None => budget,
    }
}

// ------------------------------------------------------------- compile

/// Absolute addresses of the §3.5 helpers, as the emitter wants them.
fn helpers() -> Helpers {
    use luxel_core::jit as h;
    Helpers {
        fx_div: h::fx_div as *const () as u32,
        fx_pow: h::fx_pow as *const () as u32,
        arr_load_num: h::arr_load_num as *const () as u32,
        arr_load_dyn: h::arr_load_dyn as *const () as u32,
        arr_store: h::arr_store as *const () as u32,
        arr_len: h::arr_len as *const () as u32,
        new_array: h::new_array as *const () as u32,
        const_arr: h::const_arr as *const () as u32,
        call_value: h::call_value_target as *const () as u32,
        assert_fail: h::assert_fail as *const () as u32,
        bail_fuel: h::bail_fuel as *const () as u32,
        bail_depth: h::bail_depth as *const () as u32,
    }
}

/// Compile the engine's program and install it, or leave the engine
/// interpreting and record why (docs/jit-design.md §5 "Lifecycle").
///
/// Whole-program or nothing. Every exit but the last leaves a perfectly
/// good interpreted pattern behind — **a refusal is never a failed
/// pattern**, which is what lets this run unconditionally at every
/// activation:
///
/// | reason | when |
/// |---|---|
/// | `disabled` | the runtime switch is off ([`set_enabled`]) |
/// | `debug` | the debugger is attached — it steps the interpreter (§3.6) |
/// | `init-error` | init did not run to completion, so §2.3's kind exemption does not hold |
/// | `untyped` | the blob carries no `kinds` section |
/// | `no-buffer` | both exec halves are in flight (two crossfades deep) |
/// | `too-large` and the rest | [`Refusal`], whole-program, from the emitter |
///
/// Called from `try_budgeted_engine` — the choke point every activation
/// funnels through (boot default, `/api/code`, store activate, library
/// swap, crossfade).
#[inline(never)]
pub fn try_compile(e: &mut Engine) {
    if !enabled() {
        // Off by default — see LUXEL_JIT_ENABLED for the trap that is
        // still open. This line is what the QEMU gate asserts to prove the
        // switch took.
        println!("jit: interpreter (disabled)");
        set_state(STATE_INTERP, Some("disabled"), 0, 0);
        return;
    }
    if let Some(r) = e.jit_ineligible() {
        println!("jit: interpreter ({r})");
        set_state(STATE_INTERP, Some(r), 0, 0);
        return;
    }
    let Some(half) = claim() else {
        println!("jit: interpreter (no exec buffer free)");
        set_state(STATE_INTERP, Some("no-buffer"), 0, 0);
        return;
    };
    let base = half_addr(half.0);
    let t0 = Instant::now();
    // The compile itself borrows the program immutably and allocates only
    // its output; it executes nothing and touches no device (§3.9).
    let built = {
        let prog = e.program();
        let Some(kinds) = prog.kinds.as_ref() else {
            println!("jit: interpreter (blob carries no kinds section)");
            set_state(STATE_INTERP, Some("untyped"), 0, 0);
            return;
        };
        let env = Env {
            code_base: base as u32,
            builtins: luxel_core::jit::BUILTIN_ENTRIES.as_ptr() as u32,
            helpers: helpers(),
            max_code: HALF_BYTES,
        };
        luxel_jit::compile(prog, kinds, &env)
    };
    let img = match built {
        Ok(i) => i,
        Err(r) => {
            // The full wording goes to the serial console; `/api/status`
            // carries the stable id, which is the vocabulary the editor
            // lint and the console share (§4a).
            println!("jit: interpreter ({})", r.reason());
            set_state(STATE_INTERP, Some(refusal_id(&r)), 0, 0);
            return;
        }
    };
    // SAFETY: `half` is a held lease, and `compile` refused anything over
    // `max_code`, so the image fits.
    unsafe { write_half(half.0, &img.words) };
    let compile_us = t0.elapsed().as_micros().min(u32::MAX as u64) as u32;
    let bytes = img.len_bytes();
    let np = NativeProgram {
        entries: img.entries.iter().map(|o| base + *o as usize).collect(),
        abi: img.abi.iter().map(native_abi).collect(),
        code_bytes: bytes,
        compile_us,
        stack_limit: stack_limit(),
        call: alloc::boxed::Box::new(XtensaCall),
        lease: alloc::boxed::Box::new(half),
    };
    // The stack floor is narrated because it is the one number here that
    // cannot be checked from outside the running image, and getting it
    // wrong is a reboot rather than a wrong pixel (see `stack_limit`).
    println!(
        "jit: native, {} fns, {} B code ({} B pool), {} us, sp {:#x} floor {:#x}/{:#x}",
        img.entries.len(),
        bytes,
        img.pool_len as usize * 4,
        compile_us,
        &compile_us as *const u32 as usize,
        np.stack_limit,
        crate::core1::stack_floor().unwrap_or(0),
    );
    e.install_native(np);
    set_state(STATE_NATIVE, None, bytes, compile_us);
}

/// `luxel_jit::FnAbi` → the engine's view of it. The two are separate types
/// on purpose: `luxel-jit` depends on `luxel-core`, so `luxel-core` cannot
/// name the emitter's.
fn native_abi(a: &luxel_jit::FnAbi) -> NativeAbi {
    NativeAbi {
        args_in_regs: a.args_in_regs,
        params: a.params,
        ret_dyn: a.ret_dyn,
        // Every entry the engine calls takes numbers (`index`, `x`, `y`,
        // `z`, `delta`), so no handoff slot of ours is ever a two-word
        // `Dyn` one. The mask is carried rather than assumed because a
        // `CallValue` through `ctx.fn_table` can reach a function with
        // `Dyn` parameters, and the layout must be the same on both sides.
        dyn_params: a.dyn_params,
    }
}

/// [`Refusal::id`], which is `&'static str` in `luxel-jit`'s data segment;
/// re-resolved through [`REASONS`] so `/api/status` reports one of OUR
/// strings and the two lists cannot silently diverge.
fn refusal_id(r: &Refusal) -> &'static str {
    REASONS[reason_index(r.id()) as usize]
}

/// Forget the compiled image and report the interpreter. Called when an
/// engine is dropped without a replacement (pattern rejected, output
/// stopped) so `/api/status` never claims native code for a pattern that
/// is no longer loaded.
pub fn note_interpreted(reason: &'static str) {
    set_state(STATE_INTERP, Some(reason), 0, 0);
}
