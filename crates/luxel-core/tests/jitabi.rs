//! The JIT ABI as a DOWNSTREAM consumer sees it (Gitea #642).
//!
//! The differential half of phase 1 — every `generic` wrapper against the
//! interpreter's own `CallBuiltin` path, every `direct` against its
//! `generic` — lives in `src/jit/tests.rs`, because the interpreter path it
//! compares against is crate-private. What is checked HERE is that the
//! things the emitter (phase 2, a separate module) is allowed to read are
//! actually public and actually say what docs/jit-design.md §3.8 / §4 say.

use luxel_core::fixed::Fx;
use luxel_core::jit::*;
use luxel_core::vm::{builtin_sig, SigRet, Value, ValueRaw, BUILTINS, MAX_ARGS, TAG_ARR, TAG_NUM};

#[test]
fn the_emitter_only_needs_the_offset_constants() {
    // Every field the emitter encodes, reachable without naming a Rust type.
    let all = [
        OFFSET_VM,
        OFFSET_PROG,
        OFFSET_STATUS,
        OFFSET_INSN_AT,
        OFFSET_FN_IDX,
        OFFSET_FUEL,
        OFFSET_STACK_LIMIT,
        OFFSET_ARGS,
        OFFSET_ERR,
        OFFSET_FN_TABLE,
        OFFSET_BUILTINS,
    ];
    assert_eq!(all[0], 0);
    for o in all {
        assert_eq!(o % 4, 0);
        assert!(o < SIZEOF_JITCTX);
    }
    // ascending, distinct
    let mut sorted = all;
    sorted.sort_unstable();
    assert_eq!(sorted, all);
    for w in sorted.windows(2) {
        assert_ne!(w[0], w[1]);
    }
    assert_eq!(STATUS_OK, 0);
    assert_ne!(STATUS_ERR, STATUS_OK);
}

#[test]
fn value_layout_is_the_pinned_abi() {
    assert_eq!(core::mem::size_of::<Value>(), 8);
    assert_eq!(core::mem::align_of::<Value>(), 4);
    assert_eq!(core::mem::offset_of!(ValueRaw, tag), 0);
    assert_eq!(core::mem::offset_of!(ValueRaw, payload), 4);
    let v = Value::Num(Fx::from_raw(-7));
    assert_eq!(v.raw().tag, TAG_NUM);
    assert_eq!(v.raw().payload, (-7i32) as u32);
    assert_eq!(Value::from_raw(v.raw()), v);
    assert_eq!(Value::Arr(3).raw().tag, TAG_ARR);
    // …and RetDyn is the same two words
    assert_eq!(core::mem::size_of::<RetDyn>(), 8);
    assert_eq!(RetDyn::from_value(v).to_value(), v);
}

#[test]
fn the_table_is_indexed_by_builtin_id_and_never_has_a_hole() {
    assert_eq!(BUILTIN_ENTRIES.len(), BUILTINS.len());
    for id in 0..BUILTINS.len() as u16 {
        let e = &BUILTIN_ENTRIES[id as usize];
        // `generic` exists for every id, tombstones included.
        assert_ne!(e.generic as usize, 0);
        // `direct` is either a real address with a signature, or zero.
        assert_eq!(e.direct_addr() != 0, e.sig() != DirectSig::None);
        // `ret_kind` is the phase-0 signature table.
        let want = match builtin_sig(id).ret {
            SigRet::Num => RET_NUM,
            SigRet::NewArrNum => RET_NEW_ARRNUM,
            SigRet::Dyn => RET_DYN,
            SigRet::Arg(n) => RET_ARG_BASE | n,
        };
        assert_eq!(e.ret_kind, want, "id {id} (`{}`)", BUILTINS[id as usize].name);
    }
    // The boxed-args scratch the emitter reserves is sized by this.
    assert_eq!(MAX_ARGS, 16);
}

#[test]
fn the_direct_set_is_the_tier_one_numeric_set() {
    let mut names: Vec<&str> = (0..BUILTINS.len() as u16)
        .filter(|&id| BUILTIN_ENTRIES[id as usize].sig() != DirectSig::None)
        .map(|id| BUILTINS[id as usize].name)
        .collect();
    names.sort_unstable();
    // docs/jit-design.md §3.5 names seventeen pure ops and five ctx-taking
    // ones; `fract`, `lerp` and `hsv24` come along because they ARE `frac`,
    // `mix` and `hsv` (same `Builtin`, different spelling).
    let mut want = vec![
        "abs", "ceil", "clamp", "cos", "floor", "frac", "fract", "hsv", "hsv24", "lerp", "max",
        "min", "mix", "mod", "prng", "random", "round", "rgb", "sin", "sqrt", "square", "time",
        "triangle", "trunc", "wave",
    ];
    want.sort_unstable();
    assert_eq!(names, want);
}
