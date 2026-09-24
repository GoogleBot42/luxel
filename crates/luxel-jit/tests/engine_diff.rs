//! **The engine gate.** `library/` patterns driven through a REAL
//! `luxel_core::engine::Engine` with native code installed, compared frame
//! for frame against the same engine interpreting (Gitea #658,
//! docs/jit-design.md §6/§7.1).
//!
//! `library_diff.rs` proves the emitted CODE computes what the interpreter
//! computes, entering each function by hand. This test proves the GLUE:
//! that `Engine::render_pixels`, the `beforeRender` and `renderFrame`
//! stages, the per-pass `JitCtx`, the fuel reset, the argument handoff and
//! the error path all agree with the interpreted engine on the bytes that
//! reach a strip. Those are exactly the pieces the firmware has no host
//! test for and a device has no debugger for.
//!
//! It works because the native call is behind a trait. `NativeCall::enter`
//! is implemented twice — by `firmware/src/jit.rs`'s `XtensaCall`, which
//! transmutes the address to a typed `extern "C"` pointer, and by
//! [`ModelCall`] here, which runs the same image through the Xtensa ISA
//! model. The engine's side of the call is the same code in both, so what
//! this test exercises is what the device runs.
//!
//! ```text
//!   Engine::render_pixels
//!        │ ctx: *mut JitCtx  ──── REAL: real Vm, real Program, real err
//!        ▼
//!   ModelCall::enter
//!        ├─ ctx fields ──────► model memory mirror ──► generated code
//!        ├─ Vm::globals ─────► globals mirror ──────► l32i/s32i
//!        └─ callx8 trap ─────► the REAL helper, with the REAL ctx pointer
//! ```
//!
//! The mirrors exist because the model has its own address space and the
//! emitter compiles 32-bit code whatever the host is; they are the same
//! two mirrors `common/bridge.rs` keeps, sourced from a live `JitCtx`
//! instead of from one the harness built.

mod common;

use core::cell::RefCell;

use common::isa::{Cpu, Trap};
use common::{
    env_with_fake_addresses, helper_of, library, program_of, HelperId, BUILTINS_ADDR, DIRECT_BASE,
    GENERIC_BASE, HELPER_ORDER,
};

use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::jit::{
    dev32, DirectSig, ExecLease, JitCtx, NativeAbi, NativeCall, NativeProgram, RetDyn,
    BUILTIN_ENTRIES,
};
use luxel_core::vm::{Value, ValueRaw};

/// Where the `JitCtx` mirror lives in model memory (the real one lives on
/// the engine's stack and the model cannot reach it).
const CTX_ADDR: u32 = 0x3f00_0000;
/// Where the globals mirror lives.
const GLOBALS_ADDR: u32 = 0x3f01_0000;
/// Headroom below `a1` before the prologue's depth guard fires.
const STACK_ROOM: u32 = 0x4000;
/// Model instruction budget for one engine entry. Generous: a render body
/// is tens to low thousands of instructions and init can be millions.
const STEP_LIMIT: u64 = 40_000_000;

/// The exec "buffer" on the host: the model owns the bytes, so the lease
/// is a receipt and nothing more.
struct ModelLease;
impl ExecLease for ModelLease {}

/// [`NativeCall`] backed by the Xtensa ISA model.
struct ModelCall {
    cpu: RefCell<Cpu>,
    /// Native calls serviced, so a test can assert the thing actually ran.
    calls: RefCell<u64>,
}

impl ModelCall {
    fn new(image: &[u32]) -> ModelCall {
        let bytes: Vec<u8> = image.iter().flat_map(|w| w.to_le_bytes()).collect();
        let mut cpu = Cpu::boot(&bytes, 0);
        cpu.max_depth = common::isa::MAX_WINDOWS - 1;
        for h in HELPER_ORDER {
            cpu.add_native(common::helper_addr(h));
        }
        for id in 0..BUILTIN_ENTRIES.len() as u32 {
            cpu.add_native(GENERIC_BASE + id * 16);
            cpu.add_native(DIRECT_BASE + id * 16);
            let base = BUILTINS_ADDR + id * dev32::BUILTIN_ENTRY;
            cpu.mem
                .write32(base + dev32::ENTRY_GENERIC, GENERIC_BASE + id * 16)
                .unwrap();
            cpu.mem
                .write32(base + dev32::ENTRY_DIRECT, DIRECT_BASE + id * 16)
                .unwrap();
        }
        ModelCall {
            cpu: RefCell::new(cpu),
            calls: RefCell::new(0),
        }
    }
}

/// Read `n` boxed `Value`s out of model memory — a frame's boxed-argument
/// scratch, which a generic builtin wrapper wants as a real `*const Value`.
fn read_values(cpu: &Cpu, addr: u32, n: usize) -> Vec<Value> {
    (0..n)
        .map(|j| {
            let a = addr + j as u32 * dev32::VALUE;
            Value::from_raw(ValueRaw {
                tag: cpu.mem.read32(a + dev32::VALUE_TAG).unwrap_or(0),
                payload: cpu.mem.read32(a + dev32::VALUE_PAYLOAD).unwrap_or(0),
            })
        })
        .collect()
}

impl NativeCall for ModelCall {
    unsafe fn enter(&self, addr: usize, ctx: *mut JitCtx, abi: NativeAbi, args: &[i32]) {
        let mut cpu = self.cpu.borrow_mut();

        // --- the context mirror. Only the fields GENERATED CODE reads have
        // to be here; the pointers the helpers need are in the real one.
        for off in (0..dev32::SIZEOF).step_by(4) {
            cpu.mem.write32(CTX_ADDR + off, 0).unwrap();
        }
        cpu.mem
            .write32(CTX_ADDR + dev32::FUEL, (*ctx).fuel as u32)
            .unwrap();
        cpu.mem
            .write32(CTX_ADDR + dev32::STACK_LIMIT, common::isa::STACK_TOP - STACK_ROOM)
            .unwrap();
        cpu.mem
            .write32(CTX_ADDR + dev32::BUILTINS, BUILTINS_ADDR)
            .unwrap();
        cpu.mem
            .write32(CTX_ADDR + dev32::GLOBALS, GLOBALS_ADDR)
            .unwrap();
        cpu.mem
            .write32(CTX_ADDR + dev32::FN_IDX, (*ctx).fn_idx as u32)
            .unwrap();

        // --- the globals mirror
        let vm = &mut *(*ctx).vm;
        for (i, v) in vm.globals.iter().enumerate() {
            let r: ValueRaw = v.raw();
            let a = GLOBALS_ADDR + i as u32 * dev32::VALUE;
            cpu.mem.write32(a + dev32::VALUE_TAG, r.tag).unwrap();
            cpu.mem.write32(a + dev32::VALUE_PAYLOAD, r.payload).unwrap();
        }

        // --- arguments, in the SAME layout the device uses
        cpu.reset_for_call(addr as u32);
        cpu.set_ar(2, CTX_ADDR);
        if abi.args_in_regs {
            for j in 0..abi.params as usize {
                cpu.set_ar(3 + j as u8, args.get(j).copied().unwrap_or(0) as u32);
            }
        } else {
            let mut buf = [0i32; luxel_core::jit::CTX_ARGS];
            let n = luxel_core::jit::ctx_arg_words(abi, args, &mut buf);
            for (j, w) in buf[..n].iter().enumerate() {
                cpu.mem
                    .write32(CTX_ADDR + dev32::ARGS + j as u32 * 4, *w as u32)
                    .unwrap();
            }
        }

        // --- run, servicing every `callx8` to a synthetic address
        let start = cpu.steps;
        loop {
            let left = STEP_LIMIT.saturating_sub(cpu.steps - start);
            if left == 0 {
                panic!("native entry {addr:#x} ran past {STEP_LIMIT} model steps");
            }
            match cpu.run(left) {
                Err(Trap::NativeCall(a)) => {
                    *self.calls.borrow_mut() += 1;
                    let (r2, r3) = service(&mut cpu, ctx, a);
                    cpu.return_from_native(r2, r3);
                }
                Ok(_) => break,
                Err(e) => panic!("native entry {addr:#x} trapped: {e:?}"),
            }
        }

        // --- globals back out (generated code stores them directly, so a
        // function that called no helper would otherwise leave every write
        // stranded in model memory), then the status the engine reads.
        pull_globals(&cpu, vm);
        (*ctx).status = cpu.mem.read32(CTX_ADDR + dev32::STATUS).unwrap() as i32;
        (*ctx).fuel = cpu.mem.read32(CTX_ADDR + dev32::FUEL).unwrap() as i32;
    }
}

fn pull_globals(cpu: &Cpu, vm: &mut luxel_core::vm::Vm) {
    for i in 0..vm.globals.len() {
        let a = GLOBALS_ADDR + i as u32 * dev32::VALUE;
        let tag = cpu.mem.read32(a + dev32::VALUE_TAG).unwrap();
        let payload = cpu.mem.read32(a + dev32::VALUE_PAYLOAD).unwrap();
        vm.globals[i] = Value::from_raw(ValueRaw { tag, payload });
    }
}

/// One `callx8` to a synthetic address, marshalled into a real Rust call
/// through the engine's OWN `JitCtx` — so `vm`, `prog`, `err` and
/// `fn_table` are the live objects, not copies.
///
/// # Safety
/// `ctx` is the engine's context and must be live.
unsafe fn service(cpu: &mut Cpu, ctx: *mut JitCtx, addr: u32) -> (u32, u32) {
    let a = cpu.native_args();

    // `fx_div`/`fx_pow` take no context and cannot fail.
    if addr == common::helper_addr(HelperId::FxDiv) {
        return (luxel_core::jit::fx_div(a[0] as i32, a[1] as i32) as u32, 0);
    }
    if addr == common::helper_addr(HelperId::FxPow) {
        return (luxel_core::jit::fx_pow(a[0] as i32, a[1] as i32) as u32, 0);
    }

    // Everything else sees the VM: the two copies of the shared state have
    // to agree before the call and after it.
    pull_globals(cpu, &mut *(*ctx).vm);
    (*ctx).insn_at = cpu.mem.read32(CTX_ADDR + dev32::INSN_AT).unwrap();
    (*ctx).fn_idx = cpu.mem.read32(CTX_ADDR + dev32::FN_IDX).unwrap() as u16;
    (*ctx).fuel = cpu.mem.read32(CTX_ADDR + dev32::FUEL).unwrap() as i32;
    (*ctx).status = cpu.mem.read32(CTX_ADDR + dev32::STATUS).unwrap() as i32;

    // Boxed values have to come out of model memory before the call.
    let boxed: Vec<Value> = match helper_of(addr) {
        Some(HelperId::NewArray) => read_values(cpu, a[2], a[1] as usize),
        _ if (GENERIC_BASE..DIRECT_BASE).contains(&addr) => read_values(cpu, a[1], a[2] as usize),
        _ => Vec::new(),
    };

    let out: (u32, u32) = if let Some(h) = helper_of(addr) {
        match h {
            HelperId::ArrLoadNum => (
                luxel_core::jit::arr_load_num(ctx, a[1], a[2], a[3] as i32) as u32,
                0,
            ),
            HelperId::ArrLoadDyn => {
                let r = luxel_core::jit::arr_load_dyn(ctx, a[1], a[2], a[3] as i32);
                (r.tag, r.payload)
            }
            HelperId::ArrStore => {
                luxel_core::jit::arr_store(ctx, a[1], a[2], a[3] as i32, a[4], a[5]);
                (0, 0)
            }
            HelperId::ArrLen => (luxel_core::jit::arr_len(ctx, a[1], a[2]) as u32, 0),
            HelperId::NewArray => (
                luxel_core::jit::new_array(ctx, a[1], boxed.as_ptr().cast()),
                0,
            ),
            HelperId::ConstArr => (luxel_core::jit::const_arr(ctx, a[1]), 0),
            HelperId::CallValueTarget => {
                let r = luxel_core::jit::call_value_target(ctx, a[1], a[2]);
                (r.val as u32, r.status as u32)
            }
            HelperId::AssertFail => {
                luxel_core::jit::assert_fail(ctx, a[1]);
                (0, 0)
            }
            HelperId::BailFuel => {
                luxel_core::jit::bail_fuel(ctx);
                (0, 0)
            }
            HelperId::BailDepth => {
                luxel_core::jit::bail_depth(ctx);
                (0, 0)
            }
            HelperId::FxDiv | HelperId::FxPow => unreachable!("handled above"),
        }
    } else if (GENERIC_BASE..DIRECT_BASE).contains(&addr) {
        let id = ((addr - GENERIC_BASE) / 16) as usize;
        let r: RetDyn = (BUILTIN_ENTRIES[id].generic)(ctx, boxed.as_ptr(), a[2]);
        (r.tag, r.payload)
    } else {
        let id = ((addr - DIRECT_BASE) / 16) as usize;
        let e = &BUILTIN_ENTRIES[id];
        let d = e.direct;
        let v = match e.sig() {
            DirectSig::N1 => (d.n1)(a[0] as i32),
            DirectSig::N2 => (d.n2)(a[0] as i32, a[1] as i32),
            DirectSig::N3 => (d.n3)(a[0] as i32, a[1] as i32, a[2] as i32),
            DirectSig::N4 => (d.n4)(a[0] as i32, a[1] as i32, a[2] as i32, a[3] as i32),
            DirectSig::C0 => (d.c0)(ctx),
            DirectSig::C1 => (d.c1)(ctx, a[1] as i32),
            DirectSig::C2 => (d.c2)(ctx, a[1] as i32, a[2] as i32),
            DirectSig::C3 => (d.c3)(ctx, a[1] as i32, a[2] as i32, a[3] as i32),
            DirectSig::None => unreachable!("no direct entry for {id}"),
        };
        (v as u32, 0)
    };

    cpu.mem
        .write32(CTX_ADDR + dev32::STATUS, (*ctx).status as u32)
        .unwrap();
    cpu.mem
        .write32(CTX_ADDR + dev32::FUEL, (*ctx).fuel as u32)
        .unwrap();
    // globals back into the mirror
    let vm = &mut *(*ctx).vm;
    for (i, v) in vm.globals.iter().enumerate() {
        let r: ValueRaw = v.raw();
        let ad = GLOBALS_ADDR + i as u32 * dev32::VALUE;
        cpu.mem.write32(ad + dev32::VALUE_TAG, r.tag).unwrap();
        cpu.mem.write32(ad + dev32::VALUE_PAYLOAD, r.payload).unwrap();
    }
    out
}

/// Compile `engine`'s program and install it, the way
/// `firmware/src/jit.rs::try_compile` does — same order, same
/// preconditions, a different `NativeCall`.
fn install(engine: &mut Engine) -> Result<(), String> {
    if let Some(r) = engine.jit_ineligible() {
        return Err(format!("ineligible: {r}"));
    }
    let env = env_with_fake_addresses();
    let img = {
        let prog = engine.program();
        let kinds = prog.kinds.as_ref().ok_or("no kinds section")?;
        luxel_jit::compile(prog, kinds, &env).map_err(|r| r.reason())?
    };
    let np = NativeProgram {
        entries: img
            .entries
            .iter()
            .map(|o| (common::isa::CODE_BASE + o) as usize)
            .collect(),
        abi: img
            .abi
            .iter()
            .map(|a| NativeAbi {
                args_in_regs: a.args_in_regs,
                params: a.params,
                ret_dyn: a.ret_dyn,
                dyn_params: a.dyn_params,
            })
            .collect(),
        code_bytes: img.len_bytes(),
        compile_us: 0,
        // The model's own floor; the engine only carries the number.
        stack_limit: (common::isa::STACK_TOP - STACK_ROOM) as usize,
        call: Box::new(ModelCall::new(&img.words())),
        lease: Box::new(ModelLease),
    };
    engine.install_native(np);
    Ok(())
}

/// Pixels per pattern. Sixty is the strip length the rest of the suite uses.
const PIXELS: u32 = 60;
/// Frames to compare. More than one, because `beforeRender`'s delta
/// accumulates state and a native/interpreted disagreement in it shows up
/// on frame two, not frame one.
const FRAMES: usize = 4;
/// The per-frame delta, in ms.
const DELTA_MS: i32 = 16;

/// Render one pattern both ways and return the first frame that differs.
fn diff(name: &str, src: &str) -> Result<usize, String> {
    let mut interp = Engine::new(src, PIXELS, 7).map_err(|d| d.message.clone())?;
    let mut native = Engine::new(src, PIXELS, 7).map_err(|d| d.message.clone())?;
    install(&mut native)?;
    assert!(
        native.native_active(),
        "{name}: install() left the engine interpreting"
    );

    let delta = Fx::from_int(DELTA_MS);
    for f in 0..FRAMES {
        let a = interp.frame(delta).to_vec();
        let b = native.frame(delta).to_vec();
        if a != b {
            let at = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(0);
            return Err(format!(
                "{name}: frame {f} pixel {at} interpreted {:?} native {:?}",
                a[at], b[at]
            ));
        }
        // An error on one side and not the other is a difference even when
        // the pixels happen to match (a pattern that errors after painting).
        let ea = interp.take_error().map(|e| (e.message, e.fn_idx, e.pc));
        let eb = native.take_error().map(|e| (e.message, e.fn_idx, e.pc));
        if ea != eb {
            return Err(format!(
                "{name}: frame {f} error disagrees: interpreted {ea:?} native {eb:?}"
            ));
        }
    }
    Ok(FRAMES)
}

/// The five patterns docs/jit-design.md §7.1 names, driven through the
/// engine. Two strip patterns, a 2D one, the noise-heavy #260 benchmark,
/// and a `renderFrame` pattern — which is the one the by-hand harness in
/// `library_diff.rs` cannot cover at all, because the frame builtins need
/// the engine's lent buffer.
#[test]
fn engine_named_patterns_match_the_interpreter() {
    const NAMES: [&str; 5] = [
        "rainbow.js",
        "snake.js",
        "snake-2d.js",
        "perlin-fire-wind-tunnel.js",
        "bulk-canvas-ripples-2d.js",
    ];
    let lib = library();
    for want in NAMES {
        let (name, src) = lib
            .iter()
            .find(|(n, _)| n == want)
            .unwrap_or_else(|| panic!("library/{want} is missing"));
        program_of(src).unwrap_or_else(|e| panic!("{name}: {e}"));
        diff(name, src).unwrap_or_else(|e| panic!("{e}"));
    }
}

/// Shapes `library/` does not happen to contain, so the engine glue is
/// tested on them deliberately rather than by luck.
#[test]
fn engine_edge_cases_match_the_interpreter() {
    const CASES: [(&str, &str); 4] = [
        (
            // MORE parameters than the render kind supplies. `render2D` is
            // entered with (index, x, y) — three — so `z` must read the
            // interpreter's default of 0, NOT the 0.5 the engine's
            // coordinate array is pre-filled with for missing dimensions.
            // Get this wrong and every 2D pattern with a spare parameter
            // renders mid-space instead of black.
            "extra render parameter defaults to 0",
            "export function render2D(index, x, y, z) { hsv(z, 1, 1) }",
        ),
        (
            // FEWER: the spare arguments are simply dropped.
            "missing render parameters are dropped",
            "export function render3D(index) { hsv(index / pixelCount, 1, 1) }",
        ),
        (
            // A runtime error mid-frame: both sides must raise the same
            // one, at the same site, and keep the pre-error brush (the PB
            // blast radius the engine implements).
            "a mid-render error agrees, message and site",
            "var a = [1, 2]\nexport function render(index) { hsv(0.3, 1, 1); hsv(a[index + 5], 1, 1) }",
        ),
        (
            // `beforeRender` state carried across frames, which is what
            // FRAMES > 1 is for.
            "beforeRender state carries across frames",
            "var t = 0\nexport function beforeRender(delta) { t = t + delta / 1000 }\n\
             export function render(index) { hsv(t + index / pixelCount, 1, 1) }",
        ),
    ];
    for (what, src) in CASES {
        diff(what, src).unwrap_or_else(|e| panic!("{e}"));
    }
}

/// The whole library. Slow — the ISA model runs a few hundred thousand
/// instructions a second in a debug build and this renders 60 px × 4
/// frames × 307 patterns — so it is `#[ignore]` by default and run
/// deliberately:
///
/// ```text
/// cargo test --release -p luxel-jit --test engine_diff -- --ignored --nocapture
/// ```
#[test]
#[ignore = "whole-library sweep; run deliberately, see the doc comment"]
fn engine_library_matches_the_interpreter() {
    let mut ok = 0usize;
    let mut refused = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for (name, src) in library() {
        match diff(&name, &src) {
            Ok(_) => ok += 1,
            // A refusal is a correct outcome, not a failure: the pattern
            // runs interpreted. Only a MISMATCH is a bug.
            Err(e) if e.starts_with("ineligible") || e.contains("refused") => {
                refused += 1;
                eprintln!("{name}: {e}");
            }
            Err(e) => bad.push(e),
        }
    }
    eprintln!("engine diff: {ok} identical, {refused} not compiled, {} bad", bad.len());
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
