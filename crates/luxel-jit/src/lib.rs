//! Luxel's Xtensa LX7 code generator: LXBC v6 words in, native code out
//! (Gitea #651, phase 2 of #607; docs/jit-design.md §3 is the spec).
//!
//! `no_std` + `alloc`, host-testable, and **a pure function**: [`compile`]
//! reads a [`Program`], its [`Kinds`] and an [`Env`] of absolute addresses
//! and returns a [`NativeImage`] or a [`Refusal`]. It executes nothing,
//! allocates nothing executable and touches no device. That is what lets
//! the whole backend be verified on x86 — the encoder against the vendor
//! `objdump` (`tests/objdump.rs`) and the generated code against the
//! interpreter through a small Xtensa ISA model (`tests/isa/`).
//!
//! The crate is NOT linked into the firmware or the wasm playground yet;
//! phase 3 does that.
//!
//! ```text
//!  Program + Kinds ──► emit::compile ──► NativeImage { words, entries, … }
//!                          │                            │
//!                          │ xtensa::Asm                │ tests/isa
//!                          ▼                            ▼
//!                     24/16-bit forms              differential vs Vm
//! ```

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub mod xtensa;

mod emit;
mod plan;

pub use emit::compile;
pub use plan::{FnPlan, ParamConv, SlotHome};

/// Absolute addresses of every Rust helper generated code can call
/// (docs/jit-design.md §3.5, `luxel_core::jit::helpers`).
///
/// The emitter puts each one in the literal pool and reaches it with
/// `l32r` + `callx8`; it never needs to know what any of them does beyond
/// the signature the design names.
///
/// Addresses, not function pointers: the emitter compiles for a 32-bit
/// device however wide the machine it is running on is, so the test
/// harness hands it synthetic addresses and intercepts the calls
/// (`tests/isa/`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Helpers {
    /// `extern "C" fn(i32, i32) -> i32` — `Fx / Fx`, cannot fail.
    pub fx_div: u32,
    /// `extern "C" fn(i32, i32) -> i32` — `pow`, cannot fail.
    pub fx_pow: u32,
    /// `extern "C" fn(*mut JitCtx, u32 arr, i32 idx) -> i32`, fallible.
    pub arr_load_num: u32,
    /// `extern "C" fn(*mut JitCtx, u32 arr, i32 idx) -> RetDyn`, fallible.
    pub arr_load_dyn: u32,
    /// `extern "C" fn(*mut JitCtx, u32 arr, i32 idx, u32 tag, u32 payload)`,
    /// fallible.
    pub arr_store: u32,
    /// `extern "C" fn(*mut JitCtx, u32 tag, u32 payload) -> i32`, fallible.
    pub arr_len: u32,
    /// `extern "C" fn(*mut JitCtx, u32 n, *const ValueRaw) -> u32`, fallible.
    pub new_array: u32,
    /// `extern "C" fn(*mut JitCtx, u32 d) -> u32`, fallible.
    pub const_arr: u32,
    /// `luxel_core::jit::call_value_target`:
    /// `extern "C" fn(*mut JitCtx, u32 tag, u32 payload) -> Ret2`, which
    /// RESOLVES a `CallValue` callee and does not call it — see the
    /// deviation note on `emit::Emitter::call_value`.
    pub call_value: u32,
    /// `extern "C" fn(*mut JitCtx, u32 msg)` — always fails.
    pub assert_fail: u32,
    /// `extern "C" fn(*mut JitCtx)` — fills `ERR_EXEC_LIMIT`.
    pub bail_fuel: u32,
    /// `extern "C" fn(*mut JitCtx)` — fills "call depth exceeded".
    pub bail_depth: u32,
}

/// Where the compiled image will live and what it may call.
///
/// Addresses are EXECUTION addresses: on the S3 the JIT writes through the
/// DBUS alias and runs through the IBUS one (docs/jit-design.md §5), so
/// `code_base` is the IBUS address, not the pointer the emitter's output
/// was memcpy'd to.
#[derive(Clone, Copy, Debug)]
pub struct Env {
    /// Execution address of `NativeImage::words[0]` — the first word of
    /// the LITERAL POOL, which is the start of the image (§3.7). Must be
    /// 4-aligned.
    pub code_base: u32,
    /// Address of `luxel_core::jit::BUILTIN_ENTRIES`. Unused today (the
    /// emitter reads `JitCtx::builtins` at run time so one image is
    /// independent of where the table landed) and kept because §3.8 names
    /// it and phase 3 may want the direct form.
    pub builtins: u32,
    /// Helper addresses.
    pub helpers: Helpers,
    /// Hard cap on the image, in bytes — `JIT_MAX_CODE` (§5). Exceeding it
    /// is [`Refusal::TooLarge`].
    pub max_code: usize,
}

/// A compiled program: the whole buffer, plus where to enter it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeImage {
    /// `[literal pool][fn 0][fn 1]…`, 4-aligned, as little-endian words
    /// (Xtensa instructions are byte-packed 24-bit and 16-bit forms; this
    /// is the byte image viewed as words, and its length is always a
    /// multiple of 4 bytes).
    pub words: Vec<u32>,
    /// Words of literal pool at the front of `words`.
    pub pool_len: u32,
    /// Per bytecode function index: BYTE offset of its `entry` instruction
    /// from the start of `words`. Add `Env::code_base` for the address.
    pub entries: Vec<u32>,
    /// The engine's entry points by name (`Program::exported_fns`), as
    /// byte offsets into `words`.
    pub exports: Vec<(String, u32)>,
    /// How to call each function, in the same index order as `entries`.
    pub abi: Vec<FnAbi>,
}

/// The calling convention one compiled function ended up with — what phase
/// 3's engine glue, and the ISA-model harness that stands in for it today,
/// need in order to enter it (docs/jit-design.md §3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FnAbi {
    /// `entry`'s frame size in bytes.
    pub frame: u32,
    /// Parameters in `a11…a15` (caller view). When false they are handed
    /// over through `JitCtx::args`, one word for a non-`Dyn` parameter and
    /// two for a `Dyn` one, in parameter order.
    pub args_in_regs: bool,
    pub params: u8,
    /// The result comes back in `a10:a11` (tag, payload) rather than in
    /// `a10` alone.
    pub ret_dyn: bool,
}

impl NativeImage {
    /// Total size of the image in bytes.
    #[inline]
    pub fn len_bytes(&self) -> usize {
        self.words.len() * 4
    }
    /// Bytes of native code, excluding the literal pool.
    #[inline]
    pub fn code_bytes(&self) -> usize {
        self.len_bytes() - self.pool_len as usize * 4
    }
}

/// Why the whole program could not be compiled (docs/jit-design.md §4a).
///
/// A refusal is whole-program and never a panic: the caller runs the
/// interpreter. Every variant carries the site it was decided at so the
/// reason can be reported the way `/api/status` and the editor lint do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// An opcode the emitter has no row for. None should remain for a blob
    /// this compiler produced; a future opcode lands here instead of
    /// panicking.
    UnsupportedOpcode { op: u8, fn_idx: u16, word: u32 },
    /// The image is larger than [`Env::max_code`].
    TooLarge { bytes: usize, max: usize },
    /// A literal is further than `l32r`'s backwards reach (§3.7).
    L32rReach { fn_idx: u16, word: u32, distance: usize },
    /// `entry`'s frame field tops out at 32 760 bytes (§3.3).
    FrameTooLarge { fn_idx: u16, bytes: usize },
    /// The `kinds` section does not verify. A compiler bug, reported
    /// loudly — the interpreter still runs the pattern.
    Verifier { detail: String },
    /// A parameter list the frame plan cannot express: more argument words
    /// than [`luxel_core::jit::CTX_ARGS`] handoff slots.
    ParamOverflow { fn_idx: u16, words: usize },
    /// A frame or globals-array offset past what one `addmi` reaches.
    OffsetReach { fn_idx: u16, off: u32 },
    /// One instruction wanted more scratch registers than the v1 register
    /// plan has. An emitter bug rather than a property of the program —
    /// but reported as a refusal, because a panic in the compiler would
    /// take the render task down on a board with no serial port.
    ScratchExhausted { fn_idx: u16, word: u32, op: u8 },
    /// A helper sits in a different gigabyte from the generated code.
    /// `retw` restores only the low 30 bits of the return address
    /// (`PC ← PC[31:30] || a0[29:0]`), so such a call would return into
    /// the wrong gigabyte — a wild jump, not a wrong value.
    AddressRegion { name: &'static str, addr: u32 },
    /// A `j` target further than `j`'s ±128 KB.
    JumpReach { fn_idx: u16, word: u32, distance: usize },
    /// The program has no `kinds` section at all (an untyped v6 blob).
    Untyped,
}

impl Refusal {
    /// Stable machine id, the vocabulary `/api/status`'s `jit.reason` and
    /// `luxel_core::jitlint::JitRefusal::id` share.
    pub fn id(&self) -> &'static str {
        match self {
            Refusal::UnsupportedOpcode { .. } => "unsupported",
            Refusal::TooLarge { .. } => "too-large",
            Refusal::L32rReach { .. } => "l32r-reach",
            Refusal::FrameTooLarge { .. } => "frame-size",
            Refusal::Verifier { .. } => "kinds",
            Refusal::ParamOverflow { .. } => "param-overflow",
            Refusal::JumpReach { .. } => "jump-reach",
            Refusal::OffsetReach { .. } => "offset-reach",
            Refusal::ScratchExhausted { .. } => "scratch",
            Refusal::AddressRegion { .. } => "address-region",
            Refusal::Untyped => "untyped",
        }
    }

    /// `(fn_idx, fn-relative word index)` the refusal was decided at, or
    /// `(u16::MAX, 0)` for a whole-program one.
    pub fn site(&self) -> (u16, u32) {
        match *self {
            Refusal::UnsupportedOpcode { fn_idx, word, .. }
            | Refusal::L32rReach { fn_idx, word, .. }
            | Refusal::JumpReach { fn_idx, word, .. }
            | Refusal::ScratchExhausted { fn_idx, word, .. } => (fn_idx, word),
            Refusal::FrameTooLarge { fn_idx, .. } | Refusal::ParamOverflow { fn_idx, .. } => {
                (fn_idx, 0)
            }
            _ => (u16::MAX, 0),
        }
    }

    /// Human wording for the reason string the design asks a `Refusal` to
    /// carry.
    pub fn reason(&self) -> String {
        use alloc::format;
        match self {
            Refusal::UnsupportedOpcode { op, fn_idx, word } => {
                format!("unsupported opcode {op:#04x} at fn {fn_idx} word {word}")
            }
            Refusal::TooLarge { bytes, max } => {
                format!("native image is {bytes} B, over the {max} B cap")
            }
            Refusal::L32rReach { distance, .. } => {
                format!("literal pool is {distance} B away, past l32r's reach")
            }
            Refusal::FrameTooLarge { fn_idx, bytes } => {
                format!("fn {fn_idx} needs a {bytes} B frame, over entry's 32760 B")
            }
            Refusal::Verifier { detail } => format!("kinds verification failed: {detail}"),
            Refusal::ParamOverflow { fn_idx, words } => {
                format!("fn {fn_idx} takes {words} argument words, over the handoff area")
            }
            Refusal::OffsetReach { fn_idx, off } => {
                format!("fn {fn_idx} addresses {off} B from a base, past addmi's reach")
            }
            Refusal::ScratchExhausted { fn_idx, word, op } => {
                format!("fn {fn_idx} word {word} (op {op:#04x}) ran out of scratch registers")
            }
            Refusal::JumpReach { distance, .. } => {
                format!("jump target is {distance} B away, past j's reach")
            }
            Refusal::AddressRegion { name, addr } => {
                format!("helper {name} at {addr:#010x} is outside the code's gigabyte")
            }
            Refusal::Untyped => String::from("the blob carries no kinds section"),
        }
    }
}
