//! A small Xtensa LX7 interpreter: EXACTLY the instruction forms
//! `luxel_jit::xtensa::Asm` can emit, and nothing else.
//!
//! This is the other half of the host verification plan
//! (docs/jit-design.md §7.1): `tests/objdump.rs` proves the encoder's
//! BYTES are the instructions it claims, and this model proves the
//! SEQUENCES the emitter builds out of them compute what the interpreter
//! computes — on x86, with no device and no QEMU.
//!
//! Two rules keep it honest:
//!
//! 1. **`xtensa.rs` is the contract.** Every decode arm below cites the
//!    `Asm` method it undoes; if the two ever disagree, the objdump-pinned
//!    encoder is right and this file is wrong.
//! 2. **It never panics.** A malformed or unsupported word is a [`Trap`],
//!    not an abort: a panic here would be indistinguishable from a bug in
//!    the emitter this model exists to judge. The one exception is the
//!    register-index mask (`i & 0xf`), which cannot fail.
//!
//! # The window
//!
//! Real windowed registers are modelled as a FLAT file of 64 physical
//! registers plus a window base: `ar(i)` is `phys[(wb * 4 + i) % 64]`, so
//! a `callx8`'s rotation of eight registers is `wb += 2`. 64 physical
//! registers is four windows of sixteen; nesting deep enough to wrap is
//! refused ([`Trap::WindowOverflow`]) rather than modelled as the overflow
//! exception, because the emitted code never nests that deep and a loud
//! refusal beats a wrong spill.
//!
//! The split between `callx8` and `entry` follows the hardware
//! (docs/jit-design.md §3.2): `callx8` writes the return address into the
//! CALLER's `a8` (with the call increment in its top two bits) and records
//! `PS.CALLINC`; the callee's `entry as, frame` computes `ar(s) - frame`
//! in the caller's window, rotates, and deposits the result as the
//! callee's `ar(s)`. That is what makes the callee's `a0` the return
//! address, its `a2..a7` the caller's `a10..a15`, and its `a1` the
//! caller's `a1 - frame`, all without copying anything.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

/// Physical registers in the file: four windows of sixteen.
/// **Wider than the hardware's 64 on purpose.** Real Xtensa spills the
/// oldest window to the caller's stack on a window-overflow exception and
/// restores it on underflow, and both are TRANSPARENT to the running code
/// — a deep call chain behaves exactly as if the register file were
/// unbounded. A 128-window file reproduces that behaviour for any call
/// chain shallower than 128 without implementing the exception handlers,
/// and the interpreter's own `MAX_DEPTH` is 48, so nothing a pattern can
/// do reaches the wrap. What is NOT modelled is the cost of the spill,
/// which is cycles, not semantics.
pub const NPHYS: usize = 1024;
/// The window base counts in units of four registers, as `WindowBase` does.
const NUNITS: usize = NPHYS / 4;
/// How many nested `callx8`s fit before the window wraps onto itself.
pub const MAX_WINDOWS: usize = NPHYS / 8;

/// Where [`Cpu::boot`] puts the code image. Arbitrary, but 4-aligned and
/// high, like the S3's IBUS window (docs/jit-design.md §5).
pub const CODE_BASE: u32 = 0x4200_0000;
/// Where [`Cpu::boot`] points `a1`. The stack grows DOWN from here.
pub const STACK_TOP: u32 = 0x3fca_0000;

/// Bytes at the top of every frame that the window mechanism owns — the
/// model's copy of `luxel_jit::plan::WINDOW_SAVE`, spelled here because
/// the two are the same fact seen from either side: the planner reserves
/// them, and this model refuses to let generated code write them.
pub const SPILL_BYTES: u32 = 32;

// --------------------------------------------------------------- traps

/// Why execution stopped. Every one of these is a normal `Err` — the model
/// never panics (see the module comment).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trap {
    /// A word this model has no decode arm for. Either the emitter emitted
    /// something outside `Asm`, or execution ran off the end of the image
    /// (unmapped memory reads as zero, which decodes to nothing).
    UnknownInstruction { pc: u32, word: u32 },
    /// A 32-bit access whose address was not 4-aligned. The emitter
    /// guarantees aligned offsets, so this means a bad base register.
    Unaligned { pc: u32, addr: u32 },
    /// One `callx8` deeper than [`Cpu::max_depth`] windows.
    WindowOverflow { pc: u32, depth: usize },
    /// `quos`/`rems` with a zero divisor — the emitter is expected to
    /// guard these (§3.5, the `Rem` row).
    DivideByZero { pc: u32 },
    /// [`Cpu::run`]'s budget ran out: the code is looping.
    StepLimit,
    /// `callx8` reached an address registered with [`Cpu::add_native`] —
    /// a Rust helper or builtin wrapper this model cannot execute. The
    /// window is already rotated as the callee's `entry` would have left
    /// it; see [`Cpu::native_args`] and [`Cpu::return_from_native`].
    NativeCall(u32),
    /// Generated code stored into a frame's WINDOW SAVE AREA — the 32
    /// bytes below a caller's stack pointer that the window overflow
    /// handler owns (`plan::WINDOW_SAVE`).
    ///
    /// The model has a flat 64-register file and never spills, so it would
    /// otherwise execute such a store harmlessly forever — which is
    /// exactly what happened: `WINDOW_SAVE` sat at `a1+0` instead of the
    /// top of the frame, every host gate passed, and the real overflow
    /// handler ate the generated function's data on a device (Gitea #658).
    /// This trap is the model learning that lesson.
    SpillAreaWrite { pc: u32, addr: u32 },
    /// `retw.n` with no outstanding call: the function the harness entered
    /// has returned. [`Cpu::run`] turns this into `Ok(Halted)`.
    Halt,
}

/// How [`Cpu::run`] finished: the two return words, in the callee's view
/// (`a2`, `a3` — kind `Num` in `a2`, a `Dyn` pair in `a2:a3`, §3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Halted {
    pub a2: u32,
    pub a3: u32,
    /// Instructions executed by this `run` call.
    pub steps: u64,
}

/// A 32-bit access that was not 4-aligned. [`Cpu`] turns it into
/// [`Trap::Unaligned`] once it knows the pc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Misaligned(pub u32);

// -------------------------------------------------------------- memory

/// Byte-addressed sparse memory: a code image at a high address, a stack
/// somewhere else entirely, and arbitrary host pointers for the `JitCtx`,
/// all in one map with no notion of segments.
///
/// Unmapped bytes read as zero. That is deliberate: it makes a jump into
/// nowhere decode to [`Trap::UnknownInstruction`] instead of executing
/// whatever happened to be next in a dense array.
#[derive(Clone, Debug, Default)]
pub struct Mem {
    bytes: BTreeMap<u32, u8>,
    /// Ranges a test wants to prove nothing writes to (the 16-byte window
    /// save area, typically). Writes landing in one are recorded below
    /// rather than refused.
    pub watch: Vec<Range<u32>>,
    /// Addresses of writes that landed in a [`Mem::watch`] range.
    pub watch_hits: Vec<u32>,
}

impl Mem {
    pub fn new() -> Mem {
        Mem::default()
    }

    /// Copy a byte image in at `addr` — the code buffer, a literal pool, a
    /// prebuilt `JitCtx`.
    pub fn blit(&mut self, addr: u32, image: &[u8]) {
        for (i, b) in image.iter().enumerate() {
            self.bytes.insert(addr.wrapping_add(i as u32), *b);
        }
    }

    /// Guard a range: any write into it is recorded in
    /// [`Mem::watch_hits`].
    pub fn watch(&mut self, lo: u32, hi: u32) {
        self.watch.push(lo..hi);
    }

    pub fn read8(&self, addr: u32) -> u8 {
        self.bytes.get(&addr).copied().unwrap_or(0)
    }

    pub fn write8(&mut self, addr: u32, v: u8) {
        if self.watch.iter().any(|r| r.contains(&addr)) {
            self.watch_hits.push(addr);
        }
        self.bytes.insert(addr, v);
    }

    /// Little-endian 32-bit read. The emitter only ever produces 4-aligned
    /// addresses, so a misaligned one is a bug worth reporting, not a
    /// slow path to emulate.
    pub fn read32(&self, addr: u32) -> Result<u32, Misaligned> {
        if addr % 4 != 0 {
            return Err(Misaligned(addr));
        }
        Ok(u32::from_le_bytes([
            self.read8(addr),
            self.read8(addr.wrapping_add(1)),
            self.read8(addr.wrapping_add(2)),
            self.read8(addr.wrapping_add(3)),
        ]))
    }

    pub fn write32(&mut self, addr: u32, v: u32) -> Result<(), Misaligned> {
        if addr % 4 != 0 {
            return Err(Misaligned(addr));
        }
        for (i, b) in v.to_le_bytes().iter().enumerate() {
            self.write8(addr.wrapping_add(i as u32), *b);
        }
        Ok(())
    }
}

// ----------------------------------------------------------------- cpu

/// The machine.
pub struct Cpu {
    /// A FLAT register file with a window base rather than real windowed
    /// registers: `ar(i)` is `phys[(wb * 4 + i) % NPHYS]`.
    phys: [u32; NPHYS],
    /// Window base, in units of four physical registers.
    wb: usize,
    /// `PS.CALLINC`, in the same units: set by `callx8`, consumed by the
    /// callee's `entry`. Zero means "no pending rotation", which is how
    /// the function the harness enters directly behaves.
    pending_rotate: usize,
    /// Outstanding calls, for the [`Trap::WindowOverflow`] guard and to
    /// know when a `retw.n` means "the harness's function returned".
    depth: usize,
    /// Addresses that are Rust, not Xtensa (see [`Cpu::add_native`]).
    natives: BTreeSet<u32>,
    /// The native call awaiting [`Cpu::return_from_native`], if any.
    pending: Option<u32>,
    /// Set by a `retw.n` at depth 0; every later `step` re-reports
    /// [`Trap::Halt`] instead of decoding whatever is at the pc.
    halted: bool,
    pub sar: u32,
    pub pc: u32,
    pub mem: Mem,
    /// Executed instruction count, for a runaway guard.
    pub steps: u64,
    /// How many windows deep a call chain may go before
    /// [`Trap::WindowOverflow`].
    pub max_depth: usize,
    /// One `[lo, hi)` per live frame: the window save area its `entry`
    /// carved out. Pushed by `entry`, popped by `retw.n`. Generated code
    /// storing into any of them is [`Trap::SpillAreaWrite`]. The third
    /// field is the call depth the frame was opened at, so a `retw` from a
    /// NATIVE call — which increments depth without ever running `entry`
    /// — does not pop its caller's area.
    spill_areas: Vec<(u32, u32, usize)>,
}

impl Default for Cpu {
    fn default() -> Cpu {
        Cpu::new()
    }
}

impl Cpu {
    pub fn new() -> Cpu {
        Cpu {
            phys: [0; NPHYS],
            wb: 0,
            pending_rotate: 0,
            depth: 0,
            natives: BTreeSet::new(),
            pending: None,
            halted: false,
            sar: 0,
            pc: 0,
            mem: Mem::new(),
            steps: 0,
            max_depth: 4,
            spill_areas: Vec::new(),
        }
    }

    /// Blit `image` at [`CODE_BASE`], point the pc at `entry_off` bytes
    /// into it and `a1` at [`STACK_TOP`]. `entry_off` is an IMAGE offset,
    /// which is exactly what `Asm::here` hands out, so a branch target the
    /// encoder computed and an address this model fetches from differ only
    /// by `CODE_BASE` (the §3.7 buffer layout).
    pub fn boot(image: &[u8], entry_off: usize) -> Cpu {
        let mut c = Cpu::new();
        c.mem.blit(CODE_BASE, image);
        c.pc = CODE_BASE + entry_off as u32;
        c.set_ar(1, STACK_TOP);
        c
    }

    /// Re-enter at `addr` with a fresh window, an empty call stack and the
    /// stack pointer back at the top — what the ENGINE does per pixel, and
    /// what a differential harness needs between two `render` calls.
    /// Memory (the context mirror, the globals mirror, the code) is left
    /// alone; only the machine is reset.
    pub fn reset_for_call(&mut self, addr: u32) {
        self.phys = [0; NPHYS];
        self.wb = 0;
        self.pending_rotate = 0;
        self.depth = 0;
        self.pending = None;
        self.halted = false;
        self.sar = 0;
        self.pc = addr;
        self.set_ar(1, STACK_TOP);
        self.spill_areas.clear();
    }

    // ------------------------------------------------------- registers

    /// Register `i` of the CURRENT window.
    pub fn ar(&self, i: u8) -> u32 {
        self.phys[self.phys_idx(i)]
    }

    pub fn set_ar(&mut self, i: u8, v: u32) {
        let p = self.phys_idx(i);
        self.phys[p] = v;
    }

    fn phys_idx(&self, i: u8) -> usize {
        (self.wb * 4 + (i & 0xf) as usize) % NPHYS
    }

    /// Window base, in units of four registers — for assertions about the
    /// rotation, not for the code under test.
    pub fn window_base(&self) -> usize {
        self.wb
    }

    /// Outstanding `callx8`s.
    pub fn depth(&self) -> usize {
        self.depth
    }

    fn rotate(&mut self, units: usize) {
        self.wb = (self.wb + units) % NUNITS;
    }

    fn unrotate(&mut self, units: usize) {
        self.wb = (self.wb + NUNITS - units % NUNITS) % NUNITS;
    }

    // ---------------------------------------------------- native calls

    /// Register an address that is a Rust function — a `Helpers` entry or
    /// a builtin wrapper (docs/jit-design.md §3.5). A `callx8` there stops
    /// with [`Trap::NativeCall`] instead of trying to decode Rust.
    pub fn add_native(&mut self, addr: u32) {
        self.natives.insert(addr);
    }

    /// The pending native call's address, if one is waiting.
    pub fn pending_native(&self) -> Option<u32> {
        self.pending
    }

    /// Arguments of the pending native call, callee view: `a2..a7`. The
    /// window is already rotated, so these are the caller's `a10..a15`.
    pub fn native_args(&self) -> [u32; 6] {
        [
            self.ar(2),
            self.ar(3),
            self.ar(4),
            self.ar(5),
            self.ar(6),
            self.ar(7),
        ]
    }

    /// Return from the pending native call with one or two result words
    /// (`a2`, `a3` in the callee's view — a `Num` or a `Ret2`/`RetDyn`
    /// pair) and continue. This is a `retw.n`: the caller resumes with the
    /// results visible as its `a10`/`a11`.
    ///
    /// Only meaningful straight after a [`Trap::NativeCall`]; with no call
    /// pending it still performs the return, which at depth 0 parks
    /// [`Trap::Halt`].
    pub fn return_from_native(&mut self, a2: u32, a3: u32) {
        self.set_ar(2, a2);
        self.set_ar(3, a3);
        self.pending = None;
        // The Halt at depth 0 is recorded in `halted` for the next step.
        let _ = self.do_retw();
    }

    // -------------------------------------------------------- stepping

    /// Decode and execute one instruction at `self.pc`.
    pub fn step(&mut self) -> Result<(), Trap> {
        if self.halted {
            return Err(Trap::Halt);
        }
        // Re-report a pending native call rather than executing the Rust
        // at the pc: a harness that forgets to service one must not get
        // garbage.
        if let Some(a) = self.pending {
            return Err(Trap::NativeCall(a));
        }

        let pc = self.pc;
        let b0 = self.mem.read8(pc);
        let op0 = (b0 & 0xf) as u32;
        // Instruction length is a function of op0 alone: 0x8..=0xd are the
        // 16-bit density forms (`Asm::w16`), everything else is 24-bit
        // (`Asm::w24`). 0xe/0xf are reserved on this configuration and
        // fall through to UnknownInstruction below.
        let len: u32 = if (0x8..=0xd).contains(&op0) { 2 } else { 3 };
        let word = if len == 2 {
            b0 as u32 | (self.mem.read8(pc.wrapping_add(1)) as u32) << 8
        } else {
            b0 as u32
                | (self.mem.read8(pc.wrapping_add(1)) as u32) << 8
                | (self.mem.read8(pc.wrapping_add(2)) as u32) << 16
        };
        self.steps += 1;

        // The field layout of xtensa.rs's module header. `imm8` is the
        // RRI8 immediate byte, `op1`/`op2` the RRR sub-opcodes; they are
        // meaningless for the 16-bit forms and unused there.
        let t = ((word >> 4) & 0xf) as u8;
        let s = ((word >> 8) & 0xf) as u8;
        let r = ((word >> 12) & 0xf) as u8;
        let op1 = (word >> 16) & 0xf;
        let op2 = (word >> 20) & 0xf;
        let imm8 = (word >> 16) & 0xff;

        let unknown = Err(Trap::UnknownInstruction { pc, word });
        // Every arm that does not branch leaves this in place.
        self.pc = pc.wrapping_add(len);
        // Branch and jump targets are `pc + 4 + imm`, NOT `pc + 3 + imm`:
        // the ISA's offset is relative to the instruction's address plus
        // four regardless of the form's length (`Asm::branch` computes
        // `target - (at + 4)`).
        let rel = |imm: i32| (pc as i32).wrapping_add(4).wrapping_add(imm) as u32;

        match op0 {
            // ------------------------------------------- op0 0: QRST
            0x0 => match (op1, op2) {
                // RST0/op2 0 is the CALLX/"ST0" corner. `t` is `m:n`:
                // m = 3 selects the CALLX family and n its increment, so
                // t = 0b1110 is CALLX8 (`Asm::callx8`). r = 2, s = 0,
                // t = 0xf is `nop` (`Asm::nop`) — the same word with
                // r = 0 would be `callx12 a0`, which is why the encoder
                // is careful about that nibble.
                (0x0, 0x0) => {
                    if r == 0 && t == 0b1110 {
                        self.do_callx8(pc, s, pc.wrapping_add(len))
                    } else if r == 2 && s == 0 && t == 0xf {
                        Ok(())
                    } else {
                        unknown
                    }
                }
                // `Asm::and` / `or` / `xor`.
                (0x0, 0x1) => {
                    self.set_ar(r, self.ar(s) & self.ar(t));
                    Ok(())
                }
                (0x0, 0x2) => {
                    self.set_ar(r, self.ar(s) | self.ar(t));
                    Ok(())
                }
                (0x0, 0x3) => {
                    self.set_ar(r, self.ar(s) ^ self.ar(t));
                    Ok(())
                }
                // ST1: the shift-amount setters, `r` selecting which.
                (0x0, 0x4) => match r {
                    // `Asm::ssr`: SAR := as & 31.
                    0x0 => {
                        self.sar = self.ar(s) & 31;
                        Ok(())
                    }
                    // `Asm::ssl`: SAR := 32 - (as & 31), so SAR can be 32.
                    0x1 => {
                        self.sar = 32 - (self.ar(s) & 31);
                        Ok(())
                    }
                    // `Asm::ssai`: five-bit constant, low nibble in `s`
                    // and bit 4 in `t`.
                    0x4 => {
                        self.sar = ((t as u32 & 1) << 4) | s as u32;
                        Ok(())
                    }
                    _ => unknown,
                },
                // `Asm::neg` (s = 0) and `Asm::abs` (s = 1); both wrap, so
                // abs(i32::MIN) is i32::MIN.
                (0x0, 0x6) => match s {
                    0 => {
                        self.set_ar(r, (self.ar(t) as i32).wrapping_neg() as u32);
                        Ok(())
                    }
                    1 => {
                        self.set_ar(r, (self.ar(t) as i32).wrapping_abs() as u32);
                        Ok(())
                    }
                    _ => unknown,
                },
                // `Asm::add` / `Asm::sub`, wrapping.
                (0x0, 0x8) => {
                    self.set_ar(r, self.ar(s).wrapping_add(self.ar(t)));
                    Ok(())
                }
                (0x0, 0xc) => {
                    self.set_ar(r, self.ar(s).wrapping_sub(self.ar(t)));
                    Ok(())
                }

                // RST1: the shifts. `Asm::slli` encodes `32 - sa` split
                // across op2's low bit and `t`, so op2 is 0 for sa 17..31
                // and 1 for sa 1..16 — the trap the encoder's comment
                // warns about.
                (0x1, 0x0) | (0x1, 0x1) => {
                    let f = ((op2 & 1) << 4) | t as u32;
                    if f == 0 {
                        // `32 - sa == 0` is not a shift the encoder can
                        // emit (sa is 1..=31); refuse rather than invent.
                        return unknown;
                    }
                    let sa = 32 - f;
                    self.set_ar(r, self.ar(s) << sa);
                    Ok(())
                }
                // `Asm::srai`: five-bit sa, bit 4 in op2's low bit and
                // bits 3..0 in `s`; the source is `t`.
                (0x1, 0x2) | (0x1, 0x3) => {
                    let sa = ((op2 & 1) << 4) | s as u32;
                    self.set_ar(r, ((self.ar(t) as i32) >> sa) as u32);
                    Ok(())
                }
                // `Asm::srli`: four-bit sa in `s`.
                (0x1, 0x4) => {
                    self.set_ar(r, self.ar(t) >> (s as u32));
                    Ok(())
                }
                // `Asm::src`: the funnel shift. `as:at` is a 64-bit value
                // shifted RIGHT by SAR; SAR 0 yields `at` and SAR 32
                // yields `as`, so this has to be done in 64 bits.
                (0x1, 0x8) => {
                    let f = ((self.ar(s) as u64) << 32) | self.ar(t) as u64;
                    self.set_ar(r, (f >> (self.sar & 63)) as u32);
                    Ok(())
                }
                // `Asm::sll`: `as << (32 - SAR)`. With SAR = 32 (from an
                // `ssl` of a multiple of 32) that is a shift by zero, and
                // with SAR = 0 a shift by 32 — both are done in 64 bits
                // because a 32-bit `<< 32` is not a shift in Rust.
                (0x1, 0xa) => {
                    let sh = 32u32.saturating_sub(self.sar.min(32));
                    self.set_ar(r, ((self.ar(s) as u64) << sh) as u32);
                    Ok(())
                }
                // `Asm::sra`: arithmetic `at >> SAR`, saturating the count
                // at 63 so the sign fill is well defined.
                (0x1, 0xb) => {
                    let sh = self.sar.min(63);
                    self.set_ar(r, ((self.ar(t) as i32 as i64) >> sh) as u32);
                    Ok(())
                }

                // RST2: the multiply/divide group.
                // `Asm::mull` — low 32 bits of the product.
                (0x2, 0x8) => {
                    let p = (self.ar(s) as i32 as i64).wrapping_mul(self.ar(t) as i32 as i64);
                    self.set_ar(r, p as u32);
                    Ok(())
                }
                // `Asm::mulsh` — high 32 bits of the SIGNED product.
                (0x2, 0xb) => {
                    let p = (self.ar(s) as i32 as i64).wrapping_mul(self.ar(t) as i32 as i64);
                    self.set_ar(r, ((p as u64) >> 32) as u32);
                    Ok(())
                }
                // `Asm::quos` / `Asm::rems`: signed, truncating toward
                // zero, trapping on a zero divisor. i32::MIN / -1 wraps to
                // i32::MIN (hence wrapping_div/wrapping_rem).
                (0x2, 0xd) => {
                    let (a, b) = (self.ar(s) as i32, self.ar(t) as i32);
                    if b == 0 {
                        return Err(Trap::DivideByZero { pc });
                    }
                    self.set_ar(r, a.wrapping_div(b) as u32);
                    Ok(())
                }
                (0x2, 0xf) => {
                    let (a, b) = (self.ar(s) as i32, self.ar(t) as i32);
                    if b == 0 {
                        return Err(Trap::DivideByZero { pc });
                    }
                    self.set_ar(r, a.wrapping_rem(b) as u32);
                    Ok(())
                }
                _ => unknown,
            },

            // ------------------------------------------- op0 1: l32r
            //
            // `Asm::l32r`. The literal's address is `(pc + 3) & !3` plus a
            // ONE-EXTENDED imm16 scaled by four: the field is always a
            // negative word offset (no LITBASE on these cores, §3.7), so
            // the sign extension is of the 18-bit quantity, not of imm16
            // as an i16. Encoding `(dist / 4) & 0xffff` and decoding
            // `(imm16 | 0xffff0000) << 2` are exact inverses over the
            // whole -256 KB reach; reading imm16 as an i16 would get the
            // far half of that range wrong.
            0x1 => {
                let imm16 = (word >> 8) & 0xffff;
                // One-extend, THEN scale: `(0xffff0000 | imm16) << 2` with
                // the shift folded into the constant so nothing overflows.
                let off = (imm16 << 2) | 0xfffc_0000;
                let base = pc.wrapping_add(3) & !3;
                let addr = base.wrapping_add(off);
                match self.mem.read32(addr) {
                    Ok(v) => {
                        self.set_ar(t, v);
                        Ok(())
                    }
                    Err(Misaligned(a)) => Err(Trap::Unaligned { pc, addr: a }),
                }
            }

            // ------------------------------------------- op0 2: LSAI
            //
            // The RRI8 group, `r` as the sub-opcode.
            0x2 => match r {
                // `Asm::l32i` / `Asm::s32i`: the byte offset is imm8 * 4.
                0x2 => {
                    let addr = self.ar(s).wrapping_add(imm8 * 4);
                    match self.mem.read32(addr) {
                        Ok(v) => {
                            self.set_ar(t, v);
                            Ok(())
                        }
                        Err(Misaligned(a)) => Err(Trap::Unaligned { pc, addr: a }),
                    }
                }
                0x6 => {
                    let addr = self.ar(s).wrapping_add(imm8 * 4);
                    if self.in_spill_area(addr) {
                        return Err(Trap::SpillAreaWrite { pc, addr });
                    }
                    let v = self.ar(t);
                    match self.mem.write32(addr, v) {
                        Ok(()) => Ok(()),
                        Err(Misaligned(a)) => Err(Trap::Unaligned { pc, addr: a }),
                    }
                }
                // `Asm::movi`: a 12-bit signed immediate whose HIGH nibble
                // lives in `s` and low byte in the RRI8 field.
                0xa => {
                    let u = ((s as u32) << 8) | imm8;
                    self.set_ar(t, sign_extend(u, 12) as u32);
                    Ok(())
                }
                // `Asm::addi`: 8-bit signed, wrapping.
                0xc => {
                    let v = self.ar(s).wrapping_add(sign_extend(imm8, 8) as u32);
                    self.set_ar(t, v);
                    Ok(())
                }
                // `Asm::addmi`: the same immediate scaled by 256.
                0xd => {
                    let v = self
                        .ar(s)
                        .wrapping_add((sign_extend(imm8, 8) * 256) as u32);
                    self.set_ar(t, v);
                    Ok(())
                }
                _ => unknown,
            },

            // ----------------------------- op0 6: J / BZ / BI0 / ENTRY
            //
            // One op0 for four formats, told apart by `n` = the low two
            // bits of the `t` nibble (`m` is the high two). `Asm` builds
            // exactly these four.
            0x6 => {
                let n = t & 3;
                let m = t >> 2;
                match n {
                    // `Asm::j`: an 18-bit signed offset in bits 23..6.
                    0 => {
                        let imm18 = (word >> 6) & 0x3_ffff;
                        self.pc = rel(sign_extend(imm18, 18));
                        Ok(())
                    }
                    // BZ: `Asm::branch_z`, 12-bit offset. m 0 = beqz,
                    // 1 = bnez (the ISA's bltz/bgez are m 2/3 and the
                    // encoder never emits them).
                    1 => {
                        let imm12 = (word >> 12) & 0xfff;
                        let target = rel(sign_extend(imm12, 12));
                        let v = self.ar(s);
                        let take = match m {
                            0 => v == 0,
                            1 => v != 0,
                            _ => return unknown,
                        };
                        if take {
                            self.pc = target;
                        }
                        Ok(())
                    }
                    // BI0: `Asm::branch_i` — beqi (m 0) / bnei (m 1)
                    // against B4CONST[r]. blti/bgei are m 2/3, unemitted.
                    2 => {
                        let target = rel(sign_extend(imm8, 8));
                        let k = luxel_jit::xtensa::B4CONST[(r & 0xf) as usize];
                        let v = self.ar(s) as i32;
                        let take = match m {
                            0 => v == k,
                            1 => v != k,
                            _ => return unknown,
                        };
                        if take {
                            self.pc = target;
                        }
                        Ok(())
                    }
                    // BI1 with m = 0 is `Asm::entry`; the frame field is
                    // scaled by eight.
                    3 => {
                        if m != 0 {
                            return unknown;
                        }
                        let frame = ((word >> 12) & 0xfff) * 8;
                        self.do_entry(s, frame);
                        Ok(())
                    }
                    _ => unknown,
                }
            }

            // ------------------------------------------- op0 7: BRI8
            //
            // `Asm::branch`: `r` IS the condition code (the `Cond`
            // discriminants), `s` and `t` the two registers, and the
            // 8-bit offset is relative to pc + 4.
            0x7 => {
                let target = rel(sign_extend(imm8, 8));
                let (a, b) = (self.ar(s), self.ar(t));
                let take = match r {
                    0x1 => a == b,                    // beq
                    0x9 => a != b,                    // bne
                    0x2 => (a as i32) < (b as i32),   // blt
                    0xa => (a as i32) >= (b as i32),  // bge
                    0x3 => a < b,                     // bltu
                    0xb => a >= b,                    // bgeu
                    _ => return unknown,
                };
                if take {
                    self.pc = target;
                }
                Ok(())
            }

            // ------------------------------- op0 8/9: l32i.n / s32i.n
            //
            // `Asm::l32i_n` / `Asm::s32i_n`: the offset/4 rides in `r`.
            0x8 => {
                let addr = self.ar(s).wrapping_add(r as u32 * 4);
                match self.mem.read32(addr) {
                    Ok(v) => {
                        self.set_ar(t, v);
                        Ok(())
                    }
                    Err(Misaligned(a)) => Err(Trap::Unaligned { pc, addr: a }),
                }
            }
            0x9 => {
                let addr = self.ar(s).wrapping_add(r as u32 * 4);
                if self.in_spill_area(addr) {
                    return Err(Trap::SpillAreaWrite { pc, addr });
                }
                let v = self.ar(t);
                match self.mem.write32(addr, v) {
                    Ok(()) => Ok(()),
                    Err(Misaligned(a)) => Err(Trap::Unaligned { pc, addr: a }),
                }
            }

            // `Asm::add_n`.
            0xa => {
                self.set_ar(r, self.ar(s).wrapping_add(self.ar(t)));
                Ok(())
            }

            // `Asm::addi_n`: `t` is the immediate, with 0 meaning -1 (so
            // the form cannot encode a zero addend).
            0xb => {
                let imm = if t == 0 { -1 } else { t as i32 };
                self.set_ar(r, self.ar(s).wrapping_add(imm as u32));
                Ok(())
            }

            // ST2 (`Asm::movi_n`): a 7-bit field split `t`(high 3) :
            // `r`(low 4) whose values 0x60..=0x7f mean -32..=-1, giving
            // the -32..=95 range the encoder checks. `t` with bit 3 set
            // would be beqz.n/bnez.n, which the encoder never emits.
            0xc => {
                if t >> 3 != 0 {
                    return unknown;
                }
                let f = ((t as u32) << 4) | r as u32;
                let imm = if f >= 0x60 { f as i32 - 128 } else { f as i32 };
                self.set_ar(s, imm as u32);
                Ok(())
            }

            // ST3: `r` = 0 is `Asm::mov_n` (destination in `t`, source in
            // `s`); r = 0xf is the S3 group, where t = 1 is `Asm::retw_n`
            // and t = 3 is `Asm::nop_n`.
            0xd => match r {
                0x0 => {
                    self.set_ar(t, self.ar(s));
                    Ok(())
                }
                0xf => match t {
                    1 => self.do_retw(),
                    3 => Ok(()),
                    _ => unknown,
                },
                _ => unknown,
            },

            _ => unknown,
        }
    }

    /// Step until something stops it. `limit` is the step budget for THIS
    /// call, so a harness servicing native calls can keep using the same
    /// number.
    pub fn run(&mut self, limit: u64) -> Result<Halted, Trap> {
        let start = self.steps;
        loop {
            if self.steps - start >= limit {
                return Err(Trap::StepLimit);
            }
            match self.step() {
                Ok(()) => {}
                Err(Trap::Halt) => {
                    return Ok(Halted {
                        a2: self.ar(2),
                        a3: self.ar(3),
                        steps: self.steps - start,
                    })
                }
                Err(e) => return Err(e),
            }
        }
    }

    // ------------------------------------------------ window mechanics

    /// `callx8 as`. Per the ISA the return address goes into the CALLER's
    /// `a8` — which the callee's `entry` rotation turns into its `a0` —
    /// with the call increment in bits 31..30 so `retw` knows how far to
    /// rotate back. `ar(s)` is read BEFORE `a8` is written, which is what
    /// makes the emitter's idiomatic `l32r a8, lit; callx8 a8` legal.
    fn do_callx8(&mut self, pc: u32, s: u8, ret_pc: u32) -> Result<(), Trap> {
        let target = self.ar(s);
        // Refuse rather than model the window-overflow exception: the
        // emitted code is not expected to nest this deep, and a wrong
        // spill would look like an emitter bug.
        if self.depth + 1 > self.max_depth || self.depth + 1 >= MAX_WINDOWS {
            return Err(Trap::WindowOverflow {
                pc,
                depth: self.depth + 1,
            });
        }
        self.set_ar(8, (2 << 30) | (ret_pc & 0x3fff_ffff));
        self.depth += 1;
        if self.natives.contains(&target) {
            // A Rust callee never executes `entry`, so do what `entry`
            // would have done: rotate, and hand it the caller's stack
            // pointer (Rust uses its own frame, but a helper reading a1
            // must not see a stale register).
            let sp = self.ar(1);
            self.rotate(2);
            self.set_ar(1, sp);
            self.pending = Some(target);
            self.pc = target;
            return Err(Trap::NativeCall(target));
        }
        self.pending_rotate = 2;
        self.pc = target;
        Ok(())
    }

    /// `entry as, frame`: `AR[s + 4*CALLINC] <- AR[s] - frame`, then
    /// `WindowBase += CALLINC`. Modelled in that order — read the caller's
    /// `ar(s)`, rotate, write the callee's — which is why the callee's
    /// `a1` is the caller's `a1 - frame` while the caller's own `a1`
    /// (a different physical register) is untouched. The 16 bytes of
    /// window save area at the new `a1 + 0` (§3.3) are ORDINARY MEMORY:
    /// `entry` does not write them.
    fn do_entry(&mut self, s: u8, frame: u32) {
        let caller_sp = self.ar(s);
        let sp = caller_sp.wrapping_sub(frame);
        // The window save areas live just below the CALLER's sp, i.e. at
        // the TOP of the frame this `entry` just opened
        // (`plan::WINDOW_SAVE`). Nothing generated may write there.
        self.spill_areas.push((
            caller_sp.wrapping_sub(SPILL_BYTES),
            caller_sp,
            self.depth,
        ));
        let rot = self.pending_rotate;
        self.pending_rotate = 0;
        self.rotate(rot);
        self.set_ar(s, sp);
    }

    /// Is `addr` inside any live frame's window save area? A store there
    /// by generated code is a bug in the frame plan.
    fn in_spill_area(&self, addr: u32) -> bool {
        self.spill_areas
            .iter()
            .any(|&(lo, hi, _)| addr >= lo && addr < hi)
    }

    /// `retw.n`: the rotation to undo and the return pc both come out of
    /// `a0`, as they do in hardware. Only 30 bits of return address fit
    /// there (the top two carry the call increment), so the pc's high two
    /// bits come from the CURRENT pc — `PC <- PC[31:30] || a0[29:0]`,
    /// which is why a callee must live in the same 1 GB region as its
    /// caller. With no outstanding call this is the harness's own function
    /// returning, which parks [`Trap::Halt`] WITHOUT rotating, so the
    /// caller can read `a2`/`a3`.
    fn do_retw(&mut self) -> Result<(), Trap> {
        if self.depth == 0 {
            self.halted = true;
            return Err(Trap::Halt);
        }
        let link = self.ar(0);
        let rot = ((link >> 30) & 3) as usize;
        self.pc = (self.pc & 0xc000_0000) | (link & 0x3fff_ffff);
        self.unrotate(rot);
        if self.spill_areas.last().is_some_and(|a| a.2 == self.depth) {
            self.spill_areas.pop();
        }
        self.depth -= 1;
        Ok(())
    }
}

/// Sign-extend the low `bits` of `v`.
fn sign_extend(v: u32, bits: u32) -> i32 {
    let sh = 32 - bits;
    ((v << sh) as i32) >> sh
}
