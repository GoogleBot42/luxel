//! On-device JIT: compile at activation, run the engine's entries natively
//! (Gitea #658, phase 3 of #607; docs/jit-design.md §5 "Executable memory
//! and lifecycle", §6 "Engine integration").
//!
//! Three things live here, and nothing else in the firmware knows about
//! native code at all:
//!
//! 1. **The executable RAM**, per chip (docs/jit-design.md §5, #665):
//!    - **S3 — [`Lease`]**: a block of the PSRAM arena (`psram::alloc_exec`),
//!      written through the DBUS window, made visible to instruction
//!      fetch with one data-cache write-back and one instruction-cache
//!      invalidate, and EXECUTED through the IBUS mirror at
//!      `+0x0600_0000`. A board with no PSRAM (`board-s3-devkit`) or an
//!      arena that is full falls back to a main-heap block executed
//!      through internal SRAM's own instruction-bus alias
//!      (`+0x006F_0000`), and from there to the interpreter. Either way it
//!      costs no `.rwtext`, so the S3 boards keep `iram-vm`.
//!    - **classic ESP32 — [`Half`]**: a `#[link_section = ".rwtext"]`
//!      static in SRAM0, split in two so a crossfade can hold both images.
//!      Instruction-bus RAM: written with 32-bit stores through its own
//!      address and fenced with one `isync`.
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

// ------------------------------------------------------- exec memory (S3)

/// `/api/status`'s `jit.place` and `POST /api/jit`'s `place` — where a
/// compiled image lives. `AUTO` is only ever a request: PSRAM when the
/// arena has room, internal SRAM otherwise.
pub const PLACE_AUTO: u8 = 0;
pub const PLACE_PSRAM: u8 = 1;
pub const PLACE_INTERNAL: u8 = 2;
/// The classic ESP32's `.rwtext` static.
pub const PLACE_RWTEXT: u8 = 3;

/// The requested placement (`POST /api/jit {"place":…}`), read at the next
/// activation. The lever behind docs/jit-design.md §7.3's microbench —
/// the same native function from PSRAM and from internal SRAM.
static PLACE_WANT: AtomicU8 = AtomicU8::new(PLACE_AUTO);

/// Set the placement request. Like [`set_enabled`], it applies at the
/// next activation, never to the live image.
pub fn set_place(place: u8) {
    PLACE_WANT.store(place, Ordering::Relaxed);
}

/// Program cap in PSRAM: `JIT_MAX_CODE` of §5. Well inside `l32r` reach
/// (256 KB) and `j` reach (±128 KB); nothing in `library/` is a tenth of
/// it. The effective cap on an activation is lower — see [`code_cap`].
#[cfg(not(feature = "esp32"))]
pub const JIT_MAX_CODE: usize = 128 * 1024;

/// Program cap for the internal-SRAM fallback (`JIT_INTERNAL_MAX` of §5).
/// This memory is the main heap, which the pattern's own engine and every
/// HTTP request share — 8 KB is enough for most of `library/` and small
/// enough not to matter beside a 4096-px engine.
#[cfg(not(feature = "esp32"))]
pub const JIT_INTERNAL_MAX: usize = 8 * 1024;

/// The IBUS mirror of the PSRAM DBUS window: the S3's instruction and
/// data windows share one MMU table, so a page mapped for data at
/// `0x3C00_0000 + n` is fetchable at `0x4200_0000 + n`. Espressif's own
/// `elf_loader` executes PSRAM code the same way (§5).
#[cfg(not(feature = "esp32"))]
const PSRAM_DBUS_LO: usize = 0x3C00_0000;
#[cfg(not(feature = "esp32"))]
const PSRAM_DBUS_HI: usize = 0x3E00_0000;
#[cfg(not(feature = "esp32"))]
const PSRAM_DBUS_TO_IBUS: usize = 0x0600_0000;

/// Internal SRAM1 is the same physical memory on both buses:
/// `dram_seg` at `0x3FC8_8000` and `iram_seg` at `0x4037_8000`. The main
/// heap is a `.bss` static in `dram_seg`, so a heap block executes at its
/// address plus this. No cache sits in front of internal SRAM on either
/// bus, so the only fence is `isync`.
#[cfg(not(feature = "esp32"))]
const SRAM1_DATA_LO: usize = 0x3FC8_8000;
#[cfg(not(feature = "esp32"))]
const SRAM1_DATA_HI: usize = 0x3FCF_0000;
#[cfg(not(feature = "esp32"))]
const SRAM1_DATA_TO_IRAM: usize = 0x006F_0000;

/// A claim on executable memory on the S3. Dropping it frees the block;
/// the [`NativeProgram`] that owns it is dropped with the engine, so the
/// code is released exactly when nothing can enter it any more.
#[cfg(not(feature = "esp32"))]
struct Lease {
    /// Data-side address: what was written.
    data: *mut u8,
    /// Bytes allocated (the cap, not the image — see [`claim`]).
    len: usize,
    place: u8,
}

// SAFETY: the block is only ever touched from the render task, and the
// pointer is an owned allocation — `NativeProgram` boxes the lease and
// travels with the engine between tasks only while nothing executes it.
#[cfg(not(feature = "esp32"))]
unsafe impl Send for Lease {}

#[cfg(not(feature = "esp32"))]
impl Drop for Lease {
    fn drop(&mut self) {
        match self.place {
            #[cfg(feature = "psram-arena")]
            PLACE_PSRAM => unsafe { crate::psram::free_exec(self.data, self.len) },
            _ => unsafe {
                alloc::alloc::dealloc(self.data, internal_layout(self.len));
            },
        }
    }
}

#[cfg(not(feature = "esp32"))]
impl ExecLease for Lease {}

#[cfg(not(feature = "esp32"))]
fn internal_layout(len: usize) -> core::alloc::Layout {
    // 16: `entry` targets and the pool want 4; a wider line keeps the
    // block's start away from a neighbour's tail for no cost.
    core::alloc::Layout::from_size_align(len.max(4), 16).expect("layout")
}

/// What a compile may spend on its OUTPUT this activation, in bytes.
///
/// The image is emitted straight into the exec block (`compile_into`),
/// so in PSRAM the cap is simply `JIT_MAX_CODE`. The internal fallback's
/// block is a main-heap allocation, so there the cap is what the heap can
/// spare over `RUNTIME_FLOOR` (docs/firmware.md) once the emitter's own
/// bookkeeping ([`emit_heap_need`]) is paid for — never more than
/// `JIT_INTERNAL_MAX`.
#[cfg(not(feature = "esp32"))]
fn code_cap(place: u8, need: usize) -> usize {
    use luxel_core::budget::RUNTIME_FLOOR;
    let limit = match place {
        PLACE_INTERNAL => {
            // The block stays for the pattern's life, so it is charged
            // against the RUNTIME floor, not the compile-time one.
            let free = esp_alloc::HEAP.free() as usize;
            JIT_INTERNAL_MAX.min(free.saturating_sub(RUNTIME_FLOOR + need))
        }
        _ => JIT_MAX_CODE,
    };
    limit & !3
}

/// Heap the EMITTER needs for its bookkeeping — plans, stack maps, the
/// literal-pool index, fixup lists — for a given program, in bytes.
///
/// `words * EMIT_PER_WORD + fns * EMIT_PER_FN + EMIT_FIXED`, the rule
/// `crates/luxel-jit/tests/alloc_peak.rs` fits over every `library/`
/// pattern under a counting allocator and gates in CI (host bytes, so a
/// third or so over what the device actually spends — safe, not tight).
/// `try_compile` checks it against the heap BEFORE compiling and refuses
/// `no-memory` rather than running out half way: the first on-metal run
/// (2026-09-23) took the panel down exactly that way, a `Vec<Kind>` clone
/// inside `plan_all` with 48 bytes left, on a core with no serial port.
/// The three constants move together with the test's.
pub const EMIT_PER_WORD: usize = 24;
pub const EMIT_PER_FN: usize = 240;
pub const EMIT_FIXED: usize = 1024;

fn emit_heap_need(prog: &luxel_core::vm::Program) -> usize {
    prog.words.len() * EMIT_PER_WORD + prog.fns.len() * EMIT_PER_FN + EMIT_FIXED
}

/// Heap that must stay free UNDER the emitter's bookkeeping while it
/// compiles, in bytes.
///
/// Not `RUNTIME_FLOOR` (20 KB): that is the steady-state rule for a
/// pattern that stays loaded, sized for the web server's JSON buffers and
/// a store write racing the engine for the life of the pattern. The
/// bookkeeping lives for the ~10 ms of one compile on the render task,
/// and [`emit_heap_need`] already overstates it by about a third (host
/// bytes). 12 KB covers a `/api/status` body and change for that window.
/// Measured 2026-09-24 on the panel: `aurora-2d` at 4096 px leaves
/// 31,148 B free and needs 14,848 B by the rule — over the runtime floor
/// it refuses, over this one it compiles and runs 2.1x.
const COMPILE_FLOOR: usize = 12 * 1024;

/// Take executable memory for an image of at most `cap` bytes.
///
/// PSRAM first when the arena exists and the request allows it; internal
/// SRAM otherwise. `Err(reason)` is a `jit.reason` id.
#[cfg(not(feature = "esp32"))]
fn claim(want: u8, need: usize) -> Result<(Lease, usize), &'static str> {
    #[cfg(feature = "psram-arena")]
    if want != PLACE_INTERNAL {
        let cap = code_cap(PLACE_PSRAM, need);
        if cap >= 64 {
            let p = crate::psram::alloc_exec(cap);
            let a = p as usize;
            if !p.is_null() {
                if (PSRAM_DBUS_LO..PSRAM_DBUS_HI).contains(&a) {
                    return Ok((
                        Lease {
                            data: p,
                            len: cap,
                            place: PLACE_PSRAM,
                        },
                        cap,
                    ));
                }
                // Not in the window we know how to mirror: never execute
                // through a guessed alias.
                unsafe { crate::psram::free_exec(p, cap) };
                println!("jit: arena block {a:#x} outside the PSRAM window");
            }
        }
        if want == PLACE_PSRAM {
            return Err("no-buffer");
        }
    }
    #[cfg(not(feature = "psram-arena"))]
    if want == PLACE_PSRAM {
        return Err("no-buffer");
    }
    let cap = code_cap(PLACE_INTERNAL, need);
    if cap < 64 {
        return Err("no-buffer");
    }
    let p = unsafe { alloc::alloc::alloc(internal_layout(cap)) };
    if p.is_null() {
        return Err("no-buffer");
    }
    let a = p as usize;
    if !(SRAM1_DATA_LO..SRAM1_DATA_HI).contains(&a) {
        unsafe { alloc::alloc::dealloc(p, internal_layout(cap)) };
        println!("jit: heap block {a:#x} outside SRAM1, no instruction alias");
        return Err("no-buffer");
    }
    Ok((
        Lease {
            data: p,
            len: cap,
            place: PLACE_INTERNAL,
        },
        cap,
    ))
}

/// Execution address of a lease's first byte.
#[cfg(not(feature = "esp32"))]
fn exec_addr(l: &Lease) -> usize {
    match l.place {
        PLACE_PSRAM => l.data as usize + PSRAM_DBUS_TO_IBUS,
        _ => l.data as usize + SRAM1_DATA_TO_IRAM,
    }
}

/// The lease as the emitter's output slice — the image is written
/// straight into executable memory through its data-side alias, so the
/// internal heap never holds a copy (#665).
///
/// # Safety
/// The lease is held and nothing executes from it yet.
#[cfg(not(feature = "esp32"))]
unsafe fn out_slice<'a>(l: &'a mut Lease) -> &'a mut [u8] {
    core::slice::from_raw_parts_mut(l.data, l.len)
}

/// Make the first `len` bytes of a lease fetchable (§5).
///
/// PSRAM sits behind the S3's write-back data cache and the separate
/// instruction cache, and the two do not talk: the write-back pushes the
/// image out to the chip, the invalidate drops whatever the instruction
/// side had for those lines. Both are ROM routines the flash driver and
/// esp-hal's DMA buffers already use. `isync` last, so the pipeline holds
/// nothing prefetched from these addresses — that is the whole step for
/// internal SRAM, which has no cache in front of it on either bus.
///
/// # Safety
/// `len` must not exceed the lease.
#[cfg(not(feature = "esp32"))]
unsafe fn publish(l: &Lease, len: usize) {
    debug_assert!(len <= l.len);
    if l.place == PLACE_PSRAM {
        unsafe extern "C" {
            // esp32s3.rom.ld (esp-rom-sys). The `rom_` spelling is the
            // linker script's, not a wrapper of ours.
            fn rom_Cache_WriteBack_Addr(addr: u32, items: u32);
            fn Cache_Suspend_DCache_Autoload() -> u32;
            fn Cache_Resume_DCache_Autoload(v: u32);
            fn Cache_Invalidate_Addr(addr: u32, size: u32) -> i32;
        }
        // Suspend autoload around the write-back, as esp-hal does: an
        // autoloaded line landing mid-operation would be written back
        // too, which is harmless but slow.
        let al = Cache_Suspend_DCache_Autoload();
        rom_Cache_WriteBack_Addr(l.data as u32, len as u32);
        Cache_Resume_DCache_Autoload(al);
        Cache_Invalidate_Addr(exec_addr(l) as u32, len as u32);
    }
    #[cfg(target_arch = "xtensa")]
    core::arch::asm!("isync", options(nostack, nomem, preserves_flags));
}

// ------------------------------------------------ exec memory (classic)

/// Classic ESP32: 24 KB of `.rwtext`. SRAM0 is a DEDICATED 128 KB
/// instruction region, separate from the DRAM `.stack` comes out of, so
/// the buffer costs flash image and no stack at all — the binding limit
/// is the region itself. Measured on `board-esp32-generic` + `jit`,
/// 2026-09-21: `.rwtext` 67,224 B and `.rwtext.wifi` 51,800 B of 131,072,
/// leaving ~12 KB spare.
///
/// 12 KB per half covers 96 % of `library/` (`cargo test -p luxel-jit
/// --test compile_all -- --nocapture` prints the curve); the tail refuses
/// with `too-large` and interprets, which is a correct outcome and not a
/// failure. There is no PSRAM on these boards and SRAM0 is the only
/// instruction-bus RAM, so this static is the whole story here (#666).
#[cfg(feature = "esp32")]
pub const JIT_STATIC_KB: usize = 24;

/// Images the buffer holds at once. Two, because a crossfade keeps the
/// outgoing engine alive while the incoming one renders (`drop_prev` frees
/// the outgoing half).
#[cfg(feature = "esp32")]
pub const HALVES: usize = 2;

/// Bytes per image. This is `Env::max_code`, i.e. `JIT_MAX_CODE` of §5 —
/// over it, [`Refusal::TooLarge`] and the interpreter runs the pattern.
#[cfg(feature = "esp32")]
pub const HALF_BYTES: usize = JIT_STATIC_KB * 1024 / HALVES;
#[cfg(feature = "esp32")]
const HALF_WORDS: usize = HALF_BYTES / 4;

/// The executable buffer.
///
/// `static mut` rather than a `Mutex<RefCell<…>>`: this memory is written
/// by the render task and then EXECUTED, so there is no safe abstraction
/// that describes it. Access is serialised by [`TAKEN`] — one lease per
/// half, handed out once and released by `Drop`.
#[cfg(feature = "esp32")]
#[link_section = ".rwtext"]
static mut EXEC: [[u32; HALF_WORDS]; HALVES] = [[0; HALF_WORDS]; HALVES];

/// Which halves are leased. `false` = free.
#[cfg(feature = "esp32")]
static TAKEN: [AtomicBool; HALVES] = [AtomicBool::new(false), AtomicBool::new(false)];

/// A claim on one half of [`EXEC`]. Dropping it frees the half; the
/// [`NativeProgram`] that owns it is dropped with the engine, so the code
/// is released exactly when nothing can enter it any more.
#[cfg(feature = "esp32")]
struct Half(usize);

#[cfg(feature = "esp32")]
impl Drop for Half {
    fn drop(&mut self) {
        TAKEN[self.0].store(false, Ordering::Release);
    }
}

#[cfg(feature = "esp32")]
impl ExecLease for Half {}

/// Take a free half, or `no-buffer` when both are in flight (a crossfade
/// starting while another has not finished).
#[cfg(feature = "esp32")]
fn claim(_want: u8, _need: usize) -> Result<(Half, usize), &'static str> {
    (0..HALVES)
        .find(|&i| {
            TAKEN[i]
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        })
        .map(|i| (Half(i), HALF_BYTES))
        .ok_or("no-buffer")
}

/// Execution address of a half's first word. `.rwtext` is mapped on the
/// instruction bus, so the address the code is WRITTEN at is the address
/// it RUNS at — no DBUS/IBUS translation and no cache maintenance.
#[cfg(feature = "esp32")]
fn exec_addr(h: &Half) -> usize {
    // SAFETY: taking the address of a static; nothing is read or written.
    unsafe { core::ptr::addr_of!(EXEC[h.0]) as usize }
}

/// Copy a staged image into a half and make it fetchable.
///
/// SRAM0 is instruction-bus memory and a sub-word access to it faults
/// (`LoadStoreError`), while the emitter writes bytes — so the classic
/// ESP32 does NOT emit in place. `try_compile` stages the image in a heap
/// buffer (fallibly reserved; these boards have ~80 KB of heap and no
/// 4096-px engines, so the copy is affordable here in a way it is not on
/// the panel) and this copies it over with 32-bit stores. `isync` then
/// discards whatever the core prefetched from these addresses — there is
/// no data cache in front of internal SRAM, so that is the entire
/// coherency step.
///
/// # Safety
/// `h` must be a held lease and `bytes` must fit [`HALF_BYTES`].
#[cfg(feature = "esp32")]
unsafe fn publish(h: &Half, bytes: &[u8]) {
    debug_assert!(bytes.len() <= HALF_BYTES);
    let dst = core::ptr::addr_of_mut!(EXEC[h.0]) as *mut u32;
    for (i, c) in bytes.chunks_exact(4).enumerate() {
        dst.add(i)
            .write_volatile(u32::from_le_bytes([c[0], c[1], c[2], c[3]]));
    }
    // The one `asm!` in the JIT, and it takes no operands: there is no
    // intrinsic for `isync` and nothing else orders an instruction fetch
    // against the stores above.
    #[cfg(target_arch = "xtensa")]
    core::arch::asm!("isync", options(nostack, nomem, preserves_flags));
}

#[cfg(feature = "esp32")]
fn lease_place(_h: &Half) -> u8 {
    PLACE_RWTEXT
}

#[cfg(not(feature = "esp32"))]
fn lease_place(l: &Lease) -> u8 {
    l.place
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
/// Where the live image is ([`PLACE_PSRAM`] …); 0 = no image.
static PLACE_LIVE: AtomicU8 = AtomicU8::new(0);
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
const REASONS: [&str; 16] = [
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
    "no-memory",
];

fn reason_index(id: &str) -> u8 {
    REASONS.iter().position(|r| *r == id).unwrap_or(0) as u8
}

fn set_state(state: u8, reason: Option<&str>, bytes: usize, us: u32, place: u8) {
    REASON.store(reason.map_or(u8::MAX, reason_index), Ordering::Relaxed);
    CODE_BYTES.store(bytes as u32, Ordering::Relaxed);
    COMPILE_US.store(us, Ordering::Relaxed);
    PLACE_LIVE.store(place, Ordering::Relaxed);
    STATE.store(state, Ordering::Release);
}

/// Wire spelling of a placement (`jit.place`, and what `POST /api/jit`'s
/// `place` accepts).
pub fn place_name(place: u8) -> &'static str {
    match place {
        PLACE_PSRAM => "psram",
        PLACE_INTERNAL => "internal",
        PLACE_RWTEXT => "rwtext",
        _ => "auto",
    }
}

/// `(state, reason, code_bytes, compile_us, place)` for `/api/status`
/// (docs/api.md). `state` is the wire string, not the constant: the JSON
/// vocabulary is the one thing outside this module, so it is spelled once,
/// here. `place` is `None` while no image is live.
pub fn jit_status() -> (
    &'static str,
    Option<&'static str>,
    u32,
    u32,
    Option<&'static str>,
) {
    let r = REASON.load(Ordering::Relaxed);
    let state = match STATE.load(Ordering::Acquire) {
        STATE_NATIVE => "native",
        STATE_OFF => "off",
        _ => "interp",
    };
    let place = match PLACE_LIVE.load(Ordering::Relaxed) {
        0 => None,
        p => Some(place_name(p)),
    };
    (
        state,
        REASONS.get(r as usize).copied(),
        CODE_BYTES.load(Ordering::Relaxed),
        COMPILE_US.load(Ordering::Relaxed),
        place,
    )
}

/// Runtime switch: the next activation compiles only if this is set.
///
/// **ON by default since #665/#666** — the JIT is a shipped feature of
/// every board that builds it, and `POST /api/jit {"on":false}` is the
/// kill switch for one session. It shipped OFF through phase 3 because no
/// S3 had executed a byte of it; §7.3 has now run on the Seengreat panel
/// and the Athom (docs/boards.md "JIT on metal"): the library
/// differential over HTTP (`tools/jit-diff.mjs`), the PSRAM-vs-internal
/// microbench, the #260 table and a soak, on both chips. The one crash
/// that run found was the planner's heap, not codegen, and
/// `emit_heap_need` retires it. `JIT_OFF=1` at build time still removes
/// the feature from the image entirely.
///
/// `#[no_mangle]` so it is addressable by symbol from outside the running
/// image. That is not decoration — it is how `tools/qemu/jit-test.py`
/// flips the JIT on a board with no network, by writing one byte through
/// QEMU's gdbstub (it writes BOTH values explicitly, so the default here
/// does not change what the gate compares).
#[no_mangle]
pub static LUXEL_JIT_ENABLED: AtomicBool = AtomicBool::new(true);

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
        // The kill switch — see LUXEL_JIT_ENABLED. This line is what the
        // QEMU gate asserts to prove the switch took.
        println!("jit: interpreter (disabled)");
        set_state(STATE_INTERP, Some("disabled"), 0, 0, 0);
        return;
    }
    if let Some(r) = e.jit_ineligible() {
        println!("jit: interpreter ({r})");
        set_state(STATE_INTERP, Some(r), 0, 0, 0);
        return;
    }
    // The emitter's bookkeeping has to fit the heap NOW, beside the
    // engine that was just built — see `emit_heap_need`. Checked before
    // any exec memory is claimed so a refusal leaves nothing to undo.
    let need = emit_heap_need(e.program());
    {
        let free = esp_alloc::HEAP.free() as usize;
        if free < COMPILE_FLOOR + need {
            println!("jit: interpreter (no-memory: {need} B to compile, {free} B free)");
            set_state(STATE_INTERP, Some("no-memory"), 0, 0, 0);
            return;
        }
    }
    #[allow(unused_mut)]
    let (mut lease, cap) = match claim(PLACE_WANT.load(Ordering::Relaxed), need) {
        Ok(x) => x,
        Err(r) => {
            println!("jit: interpreter ({r}: no exec memory)");
            set_state(STATE_INTERP, Some(r), 0, 0, 0);
            return;
        }
    };
    let base = exec_addr(&lease);
    let place = lease_place(&lease);
    // Classic ESP32 only: the DRAM staging buffer (see `publish`).
    #[cfg(feature = "esp32")]
    let mut stage: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    let t0 = Instant::now();
    // The compile itself borrows the program immutably and allocates only
    // its output; it executes nothing and touches no device (§3.9).
    let built = {
        let prog = e.program();
        let Some(kinds) = prog.kinds.as_ref() else {
            println!("jit: interpreter (blob carries no kinds section)");
            set_state(STATE_INTERP, Some("untyped"), 0, 0, 0);
            return;
        };
        let env = Env {
            code_base: base as u32,
            builtins: luxel_core::jit::BUILTIN_ENTRIES.as_ptr() as u32,
            helpers: helpers(),
            max_code: cap,
        };
        #[cfg(not(feature = "esp32"))]
        // SAFETY: the lease is held and nothing executes from it until
        // `publish` below.
        let out: &mut [u8] = unsafe { out_slice(&mut lease) };
        #[cfg(feature = "esp32")]
        let out: &mut [u8] = {
            if stage.try_reserve_exact(cap).is_err() {
                println!("jit: interpreter (no heap for a {cap} B staging buffer)");
                set_state(STATE_INTERP, Some("no-memory"), 0, 0, 0);
                return;
            }
            stage.resize(cap, 0);
            &mut stage[..]
        };
        luxel_jit::compile_into(prog, kinds, &env, out)
    };
    let img = match built {
        Ok(i) => i,
        Err(r) => {
            // The full wording goes to the serial console; `/api/status`
            // carries the stable id, which is the vocabulary the editor
            // lint and the console share (§4a).
            println!("jit: interpreter ({})", r.reason());
            set_state(STATE_INTERP, Some(refusal_id(&r)), 0, 0, 0);
            return;
        }
    };
    // SAFETY: `lease` is held, and `compile_into` refused anything over
    // `cap`, so the image fits.
    #[cfg(not(feature = "esp32"))]
    unsafe { publish(&lease, img.len_bytes) };
    #[cfg(feature = "esp32")]
    unsafe { publish(&lease, &stage[..img.len_bytes]) };
    #[cfg(feature = "esp32")]
    drop(stage);
    let compile_us = t0.elapsed().as_micros().min(u32::MAX as u64) as u32;
    let bytes = img.len_bytes;
    let np = NativeProgram {
        entries: img.entries.iter().map(|o| base + *o as usize).collect(),
        abi: img.abi.iter().map(native_abi).collect(),
        code_bytes: bytes,
        compile_us,
        stack_limit: stack_limit(),
        call: alloc::boxed::Box::new(XtensaCall),
        lease: alloc::boxed::Box::new(lease),
    };
    // The stack floor is narrated because it is the one number here that
    // cannot be checked from outside the running image, and getting it
    // wrong is a reboot rather than a wrong pixel (see `stack_limit`).
    println!(
        "jit: native ({}), {} fns, {} B code ({} B pool) of {} cap, {} us, at {:#x}, sp {:#x} floor {:#x}/{:#x}",
        place_name(place),
        img.entries.len(),
        bytes,
        img.pool_len as usize * 4,
        cap,
        compile_us,
        base,
        &compile_us as *const u32 as usize,
        np.stack_limit,
        crate::core1::stack_floor().unwrap_or(0),
    );
    e.install_native(np);
    set_state(STATE_NATIVE, None, bytes, compile_us, place);
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
    set_state(STATE_INTERP, Some(reason), 0, 0, 0);
}
