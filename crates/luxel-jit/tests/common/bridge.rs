//! The bridge that makes the whole backend testable on x86: an Xtensa ISA
//! model running the emitter's real output, with a REAL `Vm` behind the
//! context and real Rust helpers behind every `callx8` to a synthetic
//! address (docs/jit-design.md §7.1).
//!
//! ```text
//!   native code (isa::Cpu)                      Rust (luxel-core)
//!   ──────────────────────                      ─────────────────
//!   l32i gb, ctx, GLOBALS ──► globals MIRROR ◄──► Vm::globals
//!   callx8 <synthetic> ─────► Trap::NativeCall ─► helper / BUILTIN_ENTRIES
//!   ctx.status / fuel ──────► ctx MIRROR     ◄──► JitCtx
//! ```
//!
//! The emitter compiles for a 32-bit device wherever it runs, so it cannot
//! be handed x86-64 function pointers. Every helper and every builtin
//! entry point therefore gets a made-up 32-bit address; the model traps a
//! call to one and this file marshals the register state into a real Rust
//! call and the result back.
//!
//! Two pieces of state live in two places and are synchronised around
//! every such call:
//!
//! - **The context.** Generated code writes `status`, `insn_at`, `fn_idx`
//!   and `fuel` into the mirror; the helpers read and write the real
//!   `JitCtx`.
//! - **The globals.** Generated code loads and stores them directly
//!   (§3.8); builtins reach them through `Vm::globals`. Both are the same
//!   `[Value]` image, so the sync is a straight copy either way.

#![allow(dead_code)]

use luxel_core::fixed::Fx;
use luxel_core::jit::{dev32, DirectSig, JitCtx, RetDyn, BUILTIN_ENTRIES};
use luxel_core::vm::{Program, Value, ValueRaw, Vm, VmError};
use luxel_jit::{FnAbi, NativeImage};

use super::isa::{Cpu, Halted, Trap};
use super::{helper_of, HelperId, BUILTINS_ADDR, DIRECT_BASE, GENERIC_BASE};

/// Where the `JitCtx` mirror lives in model memory.
pub const CTX_ADDR: u32 = 0x3f00_0000;
/// Where the globals mirror lives.
pub const GLOBALS_ADDR: u32 = 0x3f01_0000;
/// `Vm::FUEL`, which is crate-private; the number is in docs/jit-design.md
/// §3.6 and is only a budget here.
pub const FUEL: i32 = 8_000_000;
/// Headroom below `a1` before the prologue's depth guard fires.
pub const STACK_ROOM: u32 = 0x4000;

/// Everything one compiled program needs in order to run.
pub struct Bridge<'p> {
    pub cpu: Cpu,
    pub vm: Vm,
    pub prog: &'p Program,
    pub err: Option<VmError>,
    /// Native entry address per bytecode function — what
    /// `call_value_target` resolves a `Fun` handle through.
    pub fn_table: Vec<usize>,
    pub abi: Vec<FnAbi>,
    /// Native calls serviced, for a "did it actually exercise anything"
    /// sanity check.
    pub native_calls: u64,
    last_steps: u64,
}

impl<'p> Bridge<'p> {
    pub fn new(prog: &'p Program, img: &NativeImage, vm: Vm) -> Bridge<'p> {
        let bytes: Vec<u8> = img.bytes.clone();
        let mut cpu = Cpu::boot(&bytes, 0);
        // The model's register file is four windows deep by construction;
        // a pattern that nests deeper than that is reported rather than
        // mismodelled.
        cpu.max_depth = super::isa::MAX_WINDOWS - 1;

        // Synthetic addresses the model must trap rather than decode.
        for h in super::HELPER_ORDER {
            cpu.add_native(super::helper_addr(h));
        }
        for id in 0..BUILTIN_ENTRIES.len() as u32 {
            cpu.add_native(GENERIC_BASE + id * 16);
            cpu.add_native(DIRECT_BASE + id * 16);
        }

        // The fake `BUILTIN_ENTRIES`: generated code indexes it by id and
        // loads the `generic` or `direct` word, so only those two words
        // have to be right — and they hold the synthetic addresses above.
        for id in 0..BUILTIN_ENTRIES.len() as u32 {
            let base = BUILTINS_ADDR + id * dev32::BUILTIN_ENTRY;
            cpu.mem
                .write32(base + dev32::ENTRY_GENERIC, GENERIC_BASE + id * 16)
                .unwrap();
            cpu.mem
                .write32(base + dev32::ENTRY_DIRECT, DIRECT_BASE + id * 16)
                .unwrap();
        }

        let fn_table: Vec<usize> = img
            .entries
            .iter()
            .map(|e| (super::isa::CODE_BASE + e) as usize)
            .collect();

        let mut b = Bridge {
            cpu,
            vm,
            prog,
            err: None,
            fn_table,
            abi: img.abi.clone(),
            native_calls: 0,
            last_steps: 0,
        };
        b.write_ctx();
        b.push_globals();
        b
    }

    /// Initialise the context mirror. Only the fields generated code reads
    /// have to be right; the pointers the HELPERS need live in the real
    /// `JitCtx` this file builds per call.
    fn write_ctx(&mut self) {
        let m = &mut self.cpu.mem;
        for off in (0..dev32::SIZEOF).step_by(4) {
            m.write32(CTX_ADDR + off, 0).unwrap();
        }
        m.write32(CTX_ADDR + dev32::FUEL, FUEL as u32).unwrap();
        m.write32(
            CTX_ADDR + dev32::STACK_LIMIT,
            super::isa::STACK_TOP - STACK_ROOM,
        )
        .unwrap();
        m.write32(CTX_ADDR + dev32::BUILTINS, BUILTINS_ADDR).unwrap();
        m.write32(CTX_ADDR + dev32::GLOBALS, GLOBALS_ADDR).unwrap();
    }

    /// `Vm::globals` → the mirror.
    fn push_globals(&mut self) {
        for (i, v) in self.vm.globals.iter().enumerate() {
            let r: ValueRaw = v.raw();
            let a = GLOBALS_ADDR + i as u32 * dev32::VALUE;
            self.cpu.mem.write32(a + dev32::VALUE_TAG, r.tag).unwrap();
            self.cpu
                .mem
                .write32(a + dev32::VALUE_PAYLOAD, r.payload)
                .unwrap();
        }
    }

    /// The mirror → `Vm::globals`.
    fn pull_globals(&mut self) {
        for i in 0..self.vm.globals.len() {
            let a = GLOBALS_ADDR + i as u32 * dev32::VALUE;
            let tag = self.cpu.mem.read32(a + dev32::VALUE_TAG).unwrap();
            let payload = self.cpu.mem.read32(a + dev32::VALUE_PAYLOAD).unwrap();
            self.vm.globals[i] = Value::from_raw(ValueRaw { tag, payload });
        }
    }

    fn ctx_word(&self, off: u32) -> u32 {
        self.cpu.mem.read32(CTX_ADDR + off).unwrap()
    }

    /// Read `n` boxed `Value`s out of model memory — the frame's
    /// boxed-argument scratch (§3.3), which a generic builtin wrapper
    /// wants as a real `*const Value`.
    fn read_values(&self, addr: u32, n: usize) -> Vec<Value> {
        (0..n)
            .map(|j| {
                let a = addr + j as u32 * dev32::VALUE;
                Value::from_raw(ValueRaw {
                    tag: self.cpu.mem.read32(a + dev32::VALUE_TAG).unwrap_or(0),
                    payload: self.cpu.mem.read32(a + dev32::VALUE_PAYLOAD).unwrap_or(0),
                })
            })
            .collect()
    }

    /// Run until the entered function returns, servicing native calls.
    ///
    /// Globals are pulled back out of the mirror on the way out: generated
    /// code stores them directly (§3.8), and a function that never called
    /// a helper would otherwise leave every one of its writes stranded in
    /// model memory.
    /// `limit` is the budget for the WHOLE entry, not for each stretch
    /// between native calls: `Cpu::run`'s own budget restarts on every
    /// call, so a loop that calls a builtin every iteration would never
    /// reach it.
    pub fn run(&mut self, limit: u64) -> Result<Halted, Trap> {
        let start = self.cpu.steps;
        let r = loop {
            let left = limit.saturating_sub(self.cpu.steps - start);
            if left == 0 {
                break Err(Trap::StepLimit);
            }
            match self.cpu.run(left) {
                Err(Trap::NativeCall(addr)) => self.service(addr),
                other => break other,
            }
        };
        self.pull_globals();
        self.last_steps = self.cpu.steps - start;
        r
    }

    /// Instructions the last [`Bridge::run`] executed, native calls
    /// excluded. `Halted::steps` counts only the stretch after the final
    /// native call, which is not what a "did this really run" check wants.
    pub fn last_steps(&self) -> u64 {
        self.last_steps
    }

    /// Enter a compiled function. `args` are the effective arguments in
    /// parameter order, already coerced to raw words (this harness only
    /// enters functions whose parameters are numbers, which is every
    /// engine entry point).
    pub fn enter(&mut self, fn_idx: usize, args: &[i32]) {
        self.cpu.reset_for_call(self.fn_table[fn_idx] as u32);
        self.cpu.set_ar(2, CTX_ADDR);
        let abi = self.abi[fn_idx];
        if abi.args_in_regs {
            for (j, a) in args.iter().enumerate().take(abi.params as usize) {
                self.cpu.set_ar(3 + j as u8, *a as u32);
            }
        } else {
            // `ParamConv::CtxArgs`: one word per non-`Dyn` parameter, two
            // for a `Dyn` one, in parameter order (§3.2). The engine's
            // entries are all-`Num`, so this is the one-word layout.
            let k = &self.prog.kinds.as_ref().unwrap().fns[fn_idx];
            let mut off = dev32::ARGS;
            for j in 0..abi.params as usize {
                let dyn_slot = k.slots[j] == luxel_core::kinds::Kind::Dyn;
                let v = args.get(j).copied().unwrap_or(0) as u32;
                if dyn_slot {
                    self.cpu.mem.write32(CTX_ADDR + off, 0).unwrap();
                    self.cpu.mem.write32(CTX_ADDR + off + 4, v).unwrap();
                    off += 8;
                } else {
                    self.cpu.mem.write32(CTX_ADDR + off, v).unwrap();
                    off += 4;
                }
            }
        }
        // Every host entry resets the budget, exactly as `Vm::render_pixel`
        // and `Vm::call` do.
        self.cpu
            .mem
            .write32(CTX_ADDR + dev32::FUEL, FUEL as u32)
            .unwrap();
        self.cpu
            .mem
            .write32(CTX_ADDR + dev32::STATUS, 0)
            .unwrap();
        self.err = None;
    }

    /// Shorten the fuel budget. The real one is eight million units and
    /// the model runs at a few hundred thousand instructions a second in a
    /// debug build, so a test that wants to SEE the budget run out asks
    /// for a small one — the mechanism under test is the back-edge check,
    /// not the size of the number (§3.6).
    pub fn set_fuel(&mut self, n: i32) {
        self.cpu
            .mem
            .write32(CTX_ADDR + dev32::FUEL, n as u32)
            .unwrap();
    }

    /// Did the last run end in a runtime error?
    pub fn failed(&self) -> bool {
        self.ctx_word(dev32::STATUS) != 0
    }

    // ------------------------------------------------- the native bridge

    fn service(&mut self, addr: u32) {
        self.native_calls += 1;
        let a = self.cpu.native_args();
        let (r2, r3) = self.dispatch(addr, a);
        self.cpu.return_from_native(r2, r3);
    }

    /// Run one Rust call with the model's register state marshalled in and
    /// the result marshalled back.
    fn dispatch(&mut self, addr: u32, a: [u32; 6]) -> (u32, u32) {
        // `fx_div`/`fx_pow` take no context and cannot fail, so they need
        // no synchronisation at all.
        if addr == super::helper_addr(HelperId::FxDiv) {
            return (luxel_core::jit::fx_div(a[0] as i32, a[1] as i32) as u32, 0);
        }
        if addr == super::helper_addr(HelperId::FxPow) {
            return (luxel_core::jit::fx_pow(a[0] as i32, a[1] as i32) as u32, 0);
        }

        // Everything else sees the VM, so the two copies of the shared
        // state have to agree before and after.
        self.pull_globals();
        let insn_at = self.ctx_word(dev32::INSN_AT);
        let fn_idx = self.ctx_word(dev32::FN_IDX) as u16;
        let fuel = self.ctx_word(dev32::FUEL) as i32;

        // Values that have to be read out of model memory BEFORE the
        // borrow of `self.vm` starts.
        let boxed: Vec<Value> = match helper_of(addr) {
            Some(HelperId::NewArray) => self.read_values(a[2], a[1] as usize),
            _ if addr >= GENERIC_BASE && addr < DIRECT_BASE => {
                self.read_values(a[1], a[2] as usize)
            }
            _ => Vec::new(),
        };

        let Bridge { vm, prog, err, fn_table, .. } = self;
        let mut ctx = JitCtx::for_builtin_call(vm, prog, err);
        ctx.insn_at = insn_at;
        ctx.fn_idx = fn_idx;
        ctx.fuel = fuel;
        ctx.fn_table = fn_table.as_ptr();
        let p = &mut ctx as *mut JitCtx;

        // SAFETY: `ctx` borrows live objects for the length of this block,
        // nothing else touches them while a helper runs, and every pointer
        // handed over below addresses a live local.
        let out: (u32, u32) = unsafe {
            if let Some(h) = helper_of(addr) {
                match h {
                    HelperId::ArrLoadNum => (
                        luxel_core::jit::arr_load_num(p, a[1], a[2], a[3] as i32) as u32,
                        0,
                    ),
                    HelperId::ArrLoadDyn => {
                        let r = luxel_core::jit::arr_load_dyn(p, a[1], a[2], a[3] as i32);
                        (r.tag, r.payload)
                    }
                    HelperId::ArrStore => {
                        luxel_core::jit::arr_store(p, a[1], a[2], a[3] as i32, a[4], a[5]);
                        (0, 0)
                    }
                    HelperId::ArrLen => (luxel_core::jit::arr_len(p, a[1], a[2]) as u32, 0),
                    HelperId::NewArray => {
                        (luxel_core::jit::new_array(p, a[1], boxed.as_ptr().cast()), 0)
                    }
                    HelperId::ConstArr => (luxel_core::jit::const_arr(p, a[1]), 0),
                    HelperId::CallValueTarget => {
                        let r = luxel_core::jit::call_value_target(p, a[1], a[2]);
                        (r.val as u32, r.status as u32)
                    }
                    HelperId::AssertFail => {
                        luxel_core::jit::assert_fail(p, a[1]);
                        (0, 0)
                    }
                    HelperId::BailFuel => {
                        luxel_core::jit::bail_fuel(p);
                        (0, 0)
                    }
                    HelperId::BailDepth => {
                        luxel_core::jit::bail_depth(p);
                        (0, 0)
                    }
                    HelperId::FxDiv | HelperId::FxPow => unreachable!("handled above"),
                }
            } else if addr >= GENERIC_BASE && addr < DIRECT_BASE {
                let id = ((addr - GENERIC_BASE) / 16) as usize;
                let r: RetDyn = (BUILTIN_ENTRIES[id].generic)(p, boxed.as_ptr(), a[2]);
                (r.tag, r.payload)
            } else {
                let id = ((addr - DIRECT_BASE) / 16) as usize;
                let e = &BUILTIN_ENTRIES[id];
                let d = e.direct;
                let v = match e.sig() {
                    DirectSig::N1 => (d.n1)(a[0] as i32),
                    DirectSig::N2 => (d.n2)(a[0] as i32, a[1] as i32),
                    DirectSig::N3 => (d.n3)(a[0] as i32, a[1] as i32, a[2] as i32),
                    DirectSig::N4 => {
                        (d.n4)(a[0] as i32, a[1] as i32, a[2] as i32, a[3] as i32)
                    }
                    DirectSig::C0 => (d.c0)(p),
                    DirectSig::C1 => (d.c1)(p, a[1] as i32),
                    DirectSig::C2 => (d.c2)(p, a[1] as i32, a[2] as i32),
                    DirectSig::C3 => (d.c3)(p, a[1] as i32, a[2] as i32, a[3] as i32),
                    DirectSig::None => unreachable!("no direct entry for {id}"),
                };
                (v as u32, 0)
            }
        };

        let status = ctx.status as u32;
        let fuel_out = ctx.fuel as u32;
        self.cpu
            .mem
            .write32(CTX_ADDR + dev32::STATUS, status)
            .unwrap();
        self.cpu
            .mem
            .write32(CTX_ADDR + dev32::FUEL, fuel_out)
            .unwrap();
        self.push_globals();
        out
    }
}

/// The brush a render call left behind, plus whether it wrote one — the
/// whole observable result of `render` (`Vm::pixel`, `Vm::pixel_written`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brush {
    pub pixel: [i32; 3],
    pub written: bool,
}

pub fn brush(vm: &Vm) -> Brush {
    Brush {
        pixel: [vm.pixel[0].raw(), vm.pixel[1].raw(), vm.pixel[2].raw()],
        written: vm.pixel_written,
    }
}

/// A `Vm` set up the way the engine sets one up, minus everything the
/// differential test does not vary.
pub fn fresh_vm(prog: &Program, pixel_count: u32, seed: u64) -> Vm {
    let mut vm = Vm::new(prog, seed);
    vm.pixel_count = pixel_count;
    vm.globals[prog.pixel_count_g as usize] = Value::Num(Fx::from_int(pixel_count as i32));
    vm.time_ms = 1_234;
    vm
}
