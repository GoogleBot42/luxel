//! Phase-1 ABI tests (Gitea #642): the layouts are pinned, and every entry
//! in [`BUILTIN_ENTRIES`] is differentially tested against the interpreter.
//!
//! These live INSIDE the crate rather than in `tests/` because the
//! interpreter's `CallBuiltin` path — `Vm::call_builtin`, and the value
//! stack it marshals out of — is private, and the whole point is to compare
//! against that path and not against a re-implementation of it.
//! `crates/luxel-core/tests/jitabi.rs` covers the public half (offsets,
//! table shape) and `tests/abi_probe.rs` the two-word return.

use alloc::vec::Vec;

use crate::fixed::Fx;
use crate::jit::ctx::{CTX_ARGS, STATUS_ERR, STATUS_OK};
use crate::jit::*;
use crate::vm::{
    builtin_sig, BKind, Program, SigRet, Value, ValueRaw, Vm, VmError, BUILTINS, MAX_ARGS,
    TAG_ARR, TAG_BUILTIN, TAG_FUN, TAG_NUM,
};

// ------------------------------------------------------------- Value layout

#[test]
fn value_is_eight_bytes_with_a_u32_tag_at_offset_zero() {
    assert_eq!(core::mem::size_of::<Value>(), 8);
    assert_eq!(core::mem::align_of::<Value>(), 4);
    assert_eq!(core::mem::size_of::<ValueRaw>(), 8);
    assert_eq!(core::mem::offset_of!(ValueRaw, tag), 0);
    assert_eq!(core::mem::offset_of!(ValueRaw, payload), 4);
    // The tag is a u32, not a u16 — the #312 finding this repr pins.
    assert_eq!(core::mem::size_of_val(&Value::default().raw().tag), 4);
}

#[test]
fn every_variant_round_trips_through_raw_bytes() {
    // (variant, expected tag)
    let cases: &[(Value, u32)] = &[
        (Value::Num(Fx::ZERO), TAG_NUM),
        (Value::Num(Fx::ONE), TAG_NUM),
        (Value::Num(Fx::MIN), TAG_NUM),
        (Value::Num(Fx::MAX), TAG_NUM),
        (Value::Num(Fx::from_raw(-1)), TAG_NUM),
        (Value::Arr(0), TAG_ARR),
        (Value::Arr(u32::MAX), TAG_ARR),
        (Value::Fun(0), TAG_FUN),
        (Value::Fun(7), TAG_FUN),
        (Value::Builtin(0), TAG_BUILTIN),
        (Value::Builtin(187), TAG_BUILTIN),
    ];
    // The discriminants of docs/jit-design.md §11 answer 2, in order.
    assert_eq!((TAG_NUM, TAG_ARR, TAG_FUN, TAG_BUILTIN), (0, 1, 2, 3));
    assert!(cfg!(target_endian = "little"), "raw byte order assumes LE");
    for &(v, tag) in cases {
        let raw = v.raw();
        assert_eq!(raw.tag, tag, "tag of {v:?}");
        // …and the payload really is the second word of the byte image.
        let bytes: [u8; 8] = unsafe { core::mem::transmute(v) };
        let mut want = [0u8; 8];
        want[..4].copy_from_slice(&raw.tag.to_le_bytes());
        want[4..].copy_from_slice(&raw.payload.to_le_bytes());
        assert_eq!(bytes, want, "byte image of {v:?}");
        // both directions
        assert_eq!(Value::from_raw(raw), v, "round trip of {v:?}");
        let back: Value = unsafe { core::mem::transmute(raw) };
        assert_eq!(back, v, "transmute back of {v:?}");
        // and through the ABI return type
        assert_eq!(RetDyn::from_value(v).to_value(), v);
    }
}

#[test]
fn retdyn_is_the_same_two_words_as_value() {
    assert_eq!(core::mem::size_of::<RetDyn>(), 8);
    assert_eq!(core::mem::align_of::<RetDyn>(), 4);
    let r = RetDyn::from_value(Value::Arr(9));
    assert_eq!((r.tag, r.payload), (TAG_ARR, 9));
    assert_eq!(RetDyn::ZERO.to_value(), Value::Num(Fx::ZERO));
    // A tag no variant uses is read as Num rather than becoming an invalid
    // discriminant — generated code must not be able to forge a Value.
    assert_eq!(
        Value::from_raw(ValueRaw { tag: 99, payload: 5 }),
        Value::Num(Fx::from_raw(5))
    );
}

// ----------------------------------------------------------- JitCtx offsets

#[test]
fn jitctx_offsets_are_word_aligned_distinct_and_ascending() {
    let offs = [
        ("vm", OFFSET_VM),
        ("prog", OFFSET_PROG),
        ("status", OFFSET_STATUS),
        ("insn_at", OFFSET_INSN_AT),
        ("fn_idx", OFFSET_FN_IDX),
        ("fuel", OFFSET_FUEL),
        ("stack_limit", OFFSET_STACK_LIMIT),
        ("args", OFFSET_ARGS),
        ("err", OFFSET_ERR),
        ("fn_table", OFFSET_FN_TABLE),
        ("builtins", OFFSET_BUILTINS),
    ];
    assert_eq!(OFFSET_VM, 0, "ctx is the struct's own address");
    for w in offs.windows(2) {
        assert!(
            w[0].1 < w[1].1,
            "{} ({}) must precede {} ({})",
            w[0].0,
            w[0].1,
            w[1].0,
            w[1].1
        );
    }
    for (name, o) in offs {
        assert_eq!(o % 4, 0, "{name} at {o} is not word aligned");
        assert!(o + 4 <= SIZEOF_JITCTX, "{name} at {o} is past the struct");
    }
    assert_eq!(CTX_ARGS, 34, "docs/jit-design.md §3.8 args[34]");
    assert_eq!(OFFSET_ERR, OFFSET_ARGS + CTX_ARGS * 4);
    assert_eq!(STATUS_OK, 0);
    assert_ne!(STATUS_ERR, 0);
}

// ------------------------------------------------------------- table shape

#[test]
fn the_table_has_one_entry_per_builtin_id() {
    assert_eq!(BUILTIN_ENTRIES.len(), BUILTINS.len());
}

#[test]
fn ret_kind_is_the_phase_zero_signature_table() {
    for id in 0..BUILTINS.len() as u16 {
        let want = match builtin_sig(id).ret {
            SigRet::Num => RET_NUM,
            SigRet::NewArrNum => RET_NEW_ARRNUM,
            SigRet::Dyn => RET_DYN,
            SigRet::Arg(n) => RET_ARG_BASE | n,
        };
        assert_eq!(
            BUILTIN_ENTRIES[id as usize].ret_kind, want,
            "ret_kind of `{}` (id {id})",
            BUILTINS[id as usize].name
        );
    }
}

#[test]
fn a_direct_word_exists_exactly_when_a_direct_sig_does() {
    let mut with_direct = Vec::new();
    for id in 0..BUILTINS.len() as u16 {
        let e = &BUILTIN_ENTRIES[id as usize];
        let has_addr = e.direct_addr() != 0;
        let has_sig = e.sig() != DirectSig::None;
        assert_eq!(
            has_addr, has_sig,
            "`{}` (id {id}): direct={:#x} sig={:?}",
            BUILTINS[id as usize].name,
            e.direct_addr(),
            e.sig()
        );
        if has_sig {
            with_direct.push(BUILTINS[id as usize].name);
        }
        // Nothing tombstoned may claim a direct form.
        if matches!(BUILTINS[id as usize].kind, BKind::Removed | BKind::Todo) {
            assert!(!has_sig, "`{}` is a tombstone", BUILTINS[id as usize].name);
        }
    }
    // The tier-1 set of docs/jit-design.md §3.5, plus the ALIASES that share
    // an implementation with one of them (`fract`=frac, `lerp`=mix,
    // `hsv24`=hsv) — the table is keyed on the Builtin, not the name.
    with_direct.sort_unstable();
    let mut want = [
        "abs", "floor", "ceil", "round", "trunc", "frac", "clamp", "min", "max", "mod", "sqrt",
        "sin", "cos", "wave", "square", "triangle", "mix", "hsv", "rgb", "time", "random", "prng",
        "fract", "lerp", "hsv24",
    ];
    want.sort_unstable();
    assert_eq!(with_direct, want);
}

// ------------------------------------------------- generic vs interpreter

/// The VM state a builtin call can be observed to change, beside its return
/// value. Compared field by field so a wrapper that dropped a side effect
/// (`hsv` writing the brush, `array` charging the arena) fails loudly.
#[derive(Debug, PartialEq)]
struct Observed {
    ret: Result<Value, (alloc::string::String, u16, u32, bool)>,
    pixel: [Fx; 3],
    pixel_written: bool,
    plot: [Fx; 3],
    plot_dims: u8,
    plot_written: bool,
    arena_elems: usize,
    globals: Vec<Value>,
}

fn flatten(r: Result<Value, VmError>) -> Result<Value, (alloc::string::String, u16, u32, bool)> {
    r.map_err(|e| (e.message, e.fn_idx, e.pc, e.is_assert))
}

fn observe(vm: &Vm, ret: Result<Value, VmError>) -> Observed {
    Observed {
        ret: flatten(ret),
        pixel: vm.pixel,
        pixel_written: vm.pixel_written,
        plot: vm.plot_coord,
        plot_dims: vm.plot_dims,
        plot_written: vm.plot_written,
        arena_elems: vm.arena_elems(),
        globals: vm.globals.clone(),
    }
}

/// A program with a couple of globals and nothing else — the builtin arms
/// only read `prog` for the constant pool and the assert messages, and the
/// arrays this harness hands them are arena-owned.
fn harness_program() -> Program {
    crate::compile::compile("export var a = 1\nexport function render(i) {}\n")
        .expect("harness program compiles")
}

/// A VM in a known state: same seed, same clock, two real arrays so that
/// `Arr(0)`/`Arr(1)` arguments reach the array arms instead of erroring out
/// on a bad handle, and a 1D map so the coordinate builtins have something.
fn harness_vm(prog: &Program) -> Vm {
    let mut vm = Vm::new(prog, 0xC0FFEE);
    vm.time_ms = 1_234_567;
    vm.pixel_count = 8;
    let mut a = crate::arena::empty();
    for i in 0..4 {
        a.push(Value::Num(Fx::from_int(i)));
    }
    vm.alloc_array(a).expect("array 0");
    let mut b = crate::arena::empty();
    for i in 0..8 {
        b.push(Value::Num(Fx::from_int(i * 2)));
    }
    vm.alloc_array(b).expect("array 1");
    vm
}

/// Arguments of mixed kinds, taken `argc` at a time from the front. Numbers
/// dominate (that is what real code passes) with array, function and builtin
/// handles sprinkled in so a wrapper that mis-marshals a boxed value fails.
const MIXED: [Value; MAX_ARGS] = [
    Value::Num(Fx::from_raw(0x1_8000)), // 1.5
    Value::Arr(0),
    Value::Num(Fx::from_raw(-0x4000)), // -0.25
    Value::Num(Fx::from_raw(0x3_0000)), // 3
    Value::Fun(0),
    Value::Num(Fx::from_raw(2)),
    Value::Arr(1),
    Value::Num(Fx::from_raw(0)),
    Value::Builtin(0),
    Value::Num(Fx::from_raw(0x10000)),
    Value::Num(Fx::from_raw(-1)),
    Value::Num(Fx::from_raw(0x2_0000)),
    Value::Num(Fx::from_raw(1)),
    Value::Num(Fx::from_raw(0x8000)),
    Value::Num(Fx::from_raw(0x4_0000)),
    Value::Num(Fx::from_raw(-0x10000)),
];

/// Run one builtin through `BUILTIN_ENTRIES[id].generic`.
fn through_the_table(vm: &mut Vm, prog: &Program, id: u16, args: &[Value]) -> Result<Value, VmError> {
    let mut err: Option<VmError> = None;
    let entry = &BUILTIN_ENTRIES[id as usize];
    let mut ctx = JitCtx::for_builtin_call(vm, prog, &mut err);
    let ret = unsafe { (entry.generic)(&mut ctx, args.as_ptr(), args.len() as u32) };
    if ctx.status != STATUS_OK {
        assert_eq!(ctx.status, STATUS_ERR);
        return Err(err.expect("status set without an error"));
    }
    assert!(err.is_none(), "error slot filled on a successful call");
    Ok(ret.to_value())
}

/// Every builtin id, at every arity from 0 to `MAX_ARGS`, compared against
/// the interpreter's own `CallBuiltin` path — return value, error text and
/// site, and every piece of VM state a builtin can touch.
///
/// Nothing is skipped. The stateful builtins (`random`, `prng`, `time`, the
/// clock, the event queue, the sequencer) are deterministic functions of VM
/// state, and both sides start from the same freshly seeded [`harness_vm`],
/// so equality is meaningful for them too — that is the point of building a
/// new VM per call rather than reusing one.
///
/// Under `--features dispatch-table` the interpreter side IS the table, so
/// this degrades to a self-consistency check; the gate that matters runs in
/// the default build.
#[test]
fn every_generic_wrapper_matches_the_interpreter_at_every_arity() {
    let prog = harness_program();
    for id in 0..BUILTINS.len() as u16 {
        for argc in 0..=MAX_ARGS {
            let args = &MIXED[..argc];
            let mut a = harness_vm(&prog);
            let ra = a.call_builtin_from_stack(&prog, id, args);
            let want = observe(&a, ra);

            let mut b = harness_vm(&prog);
            let rb = through_the_table(&mut b, &prog, id, args);
            let got = observe(&b, rb);

            assert_eq!(
                got, want,
                "`{}` (id {id}) with {argc} args",
                BUILTINS[id as usize].name
            );
        }
    }
}

/// A call with MORE arguments than `MAX_ARGS` drops the extras, exactly as
/// `Vm::pop_args_into` caps at `MAX_ARGS`. (The decoder never emits one —
/// `MAX_ARGC` is 16 — but the wrapper's contract says so, so it is tested.)
#[test]
fn the_wrapper_caps_argc_at_max_args() {
    let prog = harness_program();
    let mut over = MIXED.to_vec();
    over.extend_from_slice(&[Value::Num(Fx::ONE); 4]);
    for &id in &[0u16, 7, 56, 57] {
        let mut a = harness_vm(&prog);
        let want = flatten(through_the_table(&mut a, &prog, id, &MIXED));
        let mut b = harness_vm(&prog);
        let got = flatten(through_the_table(&mut b, &prog, id, &over));
        assert_eq!(got, want, "id {id} with {} args", over.len());
    }
}

#[test]
fn tombstoned_and_unimplemented_ids_raise_the_interpreters_error() {
    let prog = harness_program();
    let mut any_removed = false;
    let mut any_todo = false;
    for id in 0..BUILTINS.len() as u16 {
        let def = &BUILTINS[id as usize];
        match def.kind {
            BKind::Removed => any_removed = true,
            BKind::Todo => any_todo = true,
            BKind::Impl(_) => continue,
        }
        let mut vm = harness_vm(&prog);
        let e = through_the_table(&mut vm, &prog, id, &MIXED[..2])
            .expect_err("a tombstone must not succeed");
        assert_eq!(
            e.message,
            alloc::format!("builtin `{}` is not implemented yet", def.name)
        );
        // site-less: the caller attributes it (`call_builtin_slow`)
        assert_eq!((e.fn_idx, e.pc, e.is_assert), (u16::MAX, u32::MAX, false));
    }
    // The six #626 callback builtins are the tombstones; if that ever stops
    // being true this test is silently passing on nothing.
    assert!(any_removed, "no BKind::Removed id left to test");
    let _ = any_todo;
}

// ----------------------------------------------------- direct vs generic

/// Raw 16.16 words worth sweeping: the extremes, the units, the halves and
/// a couple of ordinary values, each in both signs.
const SWEEP: [i32; 15] = [
    i32::MIN,
    i32::MIN + 1,
    -0x7FFF_0000,
    -0x2_0000,
    -0x1_0000,
    -0x8000,
    -1,
    0,
    1,
    0x8000,
    0x1_0000,
    0x2_0000,
    0x1_8000,
    0x7FFF_0000,
    i32::MAX,
];

#[test]
fn every_direct_entry_matches_its_generic_wrapper() {
    let prog = harness_program();
    let mut checked = 0usize;
    for id in 0..BUILTINS.len() as u16 {
        let e = &BUILTIN_ENTRIES[id as usize];
        let sig = e.sig();
        if sig == DirectSig::None {
            continue;
        }
        let argc = match sig {
            DirectSig::N1 | DirectSig::C1 => 1,
            DirectSig::N2 | DirectSig::C2 => 2,
            DirectSig::N3 | DirectSig::C3 => 3,
            DirectSig::N4 => 4,
            DirectSig::C0 => 0,
            DirectSig::None => unreachable!(),
        };
        // A full cartesian sweep at arity 3 is 3 375 calls per builtin,
        // which is fine; the inputs rotate so every argument sees every
        // word without the product exploding at higher arities.
        for (i, &x) in SWEEP.iter().enumerate() {
            for (j, &y) in SWEEP.iter().enumerate() {
                let raw = [x, y, SWEEP[(i + j) % SWEEP.len()], SWEEP[(i + 1) % SWEEP.len()]];
                let args: Vec<Value> = raw[..argc]
                    .iter()
                    .map(|&w| Value::Num(Fx::from_raw(w)))
                    .collect();

                let mut vg = harness_vm(&prog);
                let want = through_the_table(&mut vg, &prog, id, &args)
                    .unwrap_or_else(|e| panic!("tier-1 builtin cannot fail: {}", e.message));
                let want_pixel = (vg.pixel, vg.pixel_written);

                let mut vd = harness_vm(&prog);
                let mut err: Option<VmError> = None;
                let mut ctx = JitCtx::for_builtin_call(&mut vd, &prog, &mut err);
                let got = unsafe {
                    match sig {
                        DirectSig::N1 => (e.direct.n1)(raw[0]),
                        DirectSig::N2 => (e.direct.n2)(raw[0], raw[1]),
                        DirectSig::N3 => (e.direct.n3)(raw[0], raw[1], raw[2]),
                        DirectSig::N4 => (e.direct.n4)(raw[0], raw[1], raw[2], raw[3]),
                        DirectSig::C0 => (e.direct.c0)(&mut ctx),
                        DirectSig::C1 => (e.direct.c1)(&mut ctx, raw[0]),
                        DirectSig::C2 => (e.direct.c2)(&mut ctx, raw[0], raw[1]),
                        DirectSig::C3 => (e.direct.c3)(&mut ctx, raw[0], raw[1], raw[2]),
                        DirectSig::None => unreachable!(),
                    }
                };
                drop(ctx);
                assert_eq!(
                    Value::Num(Fx::from_raw(got)),
                    want,
                    "`{}` (id {id}, {sig:?}) on {:?}",
                    BUILTINS[id as usize].name,
                    &raw[..argc]
                );
                assert_eq!(
                    (vd.pixel, vd.pixel_written),
                    want_pixel,
                    "`{}` brush side effect",
                    BUILTINS[id as usize].name
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 5_000, "sweep did almost nothing ({checked})");
}

// ---------------------------------------------------------------- Ret2 ABI

#[test]
fn the_two_word_return_round_trips_on_the_host() {
    for (a, b) in [(0, 0), (1, -1), (i32::MIN, i32::MAX), (7, 9)] {
        let r = lx_abi_probe_ret2(a, b);
        assert_eq!(r.val, a.wrapping_add(b));
        assert_eq!(r.status, a ^ b);
    }
    assert_eq!(core::mem::size_of::<Ret2>(), 8);
    assert_eq!(core::mem::offset_of!(Ret2, val), 0);
    assert_eq!(core::mem::offset_of!(Ret2, status), 4);
}
