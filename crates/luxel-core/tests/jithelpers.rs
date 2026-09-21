//! The §3.5 helpers, against the interpreter (Gitea #651, phase 2 of #607).
//!
//! `crates/luxel-core/src/jit/helpers.rs` exists so that generated Xtensa
//! code can do what `Vm::run`'s arms do. The only interesting property is
//! therefore SAMENESS, and every case here establishes it the same way:
//! compile a tiny pattern whose init performs the operation, run it through
//! the interpreter, then perform the same operation through the helper and
//! compare — the value, the resulting arena, the error text, and the error's
//! `fn_idx`/`pc`/`line`/`col`/`is_assert`. Nothing is asserted against a
//! hand-written expectation of what the interpreter "should" do; the
//! interpreter IS the expectation. ([`VmError`] is `PartialEq`, so an error
//! case is one `assert_eq!` over the whole struct.)
//!
//! This is an INTEGRATION test on purpose: the emitter will reach the
//! helpers through the crate's public surface, so these call them exactly
//! the way it will — `luxel_core::jit::*`, a [`JitCtx`] from
//! `JitCtx::for_builtin_call` with the site fields filled in afterwards.
//!
//! Error ATTRIBUTION is what the site-filling checks. `Vm::err_at` reads
//! `self.frames.last()`, and native code has no interpreter frames; the
//! emitter stores the site into `ctx.fn_idx`/`ctx.insn_at` instead
//! (docs/jit-design.md §3.6). Each case takes the site out of the
//! interpreter's own error and hands it to the context, so a mismatch means
//! the helper's `ctx_err` disagrees with `err_at` about what that site
//! means — which is exactly the bug worth catching.

use luxel_core::compile::compile;
use luxel_core::fixed::Fx;
use luxel_core::fmath;
use luxel_core::jit::*;
use luxel_core::vm::{Program, Value, ValueRaw, Vm, VmError, TAG_ARR, TAG_BUILTIN, TAG_FUN, TAG_NUM};

/// Pixel count every harness VM reports. `assert()`'s message quotes it, so
/// both sides have to agree on it.
const PX: i32 = 10;

fn prog(src: &str) -> Program {
    compile(src).unwrap_or_else(|d| panic!("compile error: {d:?}\n--- source ---\n{src}"))
}

/// A VM in the state `Engine::new` would hand the pattern: globals at their
/// declared initializers, `pixelCount` filled in.
fn vm_for(p: &Program) -> Vm {
    let mut vm = Vm::new(p, 0x5EED_1234);
    vm.globals[p.pixel_count_g as usize] = Value::Num(Fx::from_int(PX));
    vm
}

/// Run top-level init (`fns[0]`) — the interpreter side of every case.
fn init(p: &Program, vm: &mut Vm) -> Result<Value, VmError> {
    vm.call(p, 0, &[])
}

/// Read an exported global by name.
fn g(p: &Program, vm: &Vm, name: &str) -> Value {
    let i = p
        .global_index(name)
        .unwrap_or_else(|| panic!("no global `{name}`"));
    vm.globals[i as usize]
}

/// Contents of arena entry `id`, for comparing two arrays element by
/// element.
fn arena(p: &Program, vm: &Vm, id: u32) -> Vec<Value> {
    vm.array(p, id)
        .unwrap_or_else(|| panic!("no arena entry {id}"))
        .to_vec()
}

/// A context standing exactly where the interpreter's error says the
/// faulting instruction was (docs/jit-design.md §3.6). `for_builtin_call`
/// leaves `fn_idx = u16::MAX`, which is precisely the "no frame" answer the
/// helpers must NOT give.
fn ctx_at(vm: &mut Vm, p: &Program, err: &mut Option<VmError>, at: (u16, u32)) -> JitCtx {
    let mut c = JitCtx::for_builtin_call(vm, p, err);
    c.fn_idx = at.0;
    c.insn_at = at.1;
    c
}

/// The site an interpreter error reports, for feeding back into [`ctx_at`].
fn site(e: &VmError) -> (u16, u32) {
    (e.fn_idx, e.pc)
}

/// What a helper call left behind: `None` on success (and the error slot
/// must still be empty), the error otherwise. Takes the status by value
/// because every case reads it out and drops the context before touching
/// the VM again — `JitCtx` holds raw pointers into the VM, and nothing may
/// alias them while it is alive.
fn outcome_of(status: i32, err: &mut Option<VmError>) -> Option<VmError> {
    if status == STATUS_OK {
        assert!(err.is_none(), "error slot filled on a successful helper call");
        return None;
    }
    assert_eq!(status, STATUS_ERR, "status must be OK or ERR, nothing else");
    Some(err.take().expect("STATUS_ERR without an error"))
}

// ===================================================== fx_div / fx_pow

/// Raw words spanning the interesting shapes: the `Fx` extremes, ±1, ±0.5,
/// zero, integers, and fractions with a non-zero low half (what forces the
/// i64 path in `impl Div`).
const SWEEP: &[i32] = &[
    0,
    1,
    -1,
    0x8000,      // 0.5
    -0x8000,     // -0.5
    0x1_0000,    // 1
    -0x1_0000,   // -1
    0x2_0000,    // 2
    -0x3_0000,   // -3
    0x1_8000,    // 1.5
    -0x1_8000,   // -1.5
    0x7_0000,    // 7
    0x1_2345,    // 1.1377…
    -0x1_2345,
    0x123_4567,
    i32::MIN, // Fx::MIN
    i32::MAX, // Fx::MAX
];

/// Which arm of `impl Div for Fx` a pair takes — the four paths §3.5's
/// `Div` row is about ("three exact 32-bit paths and an i64 fallback").
fn div_path(a: i32, b: i32) -> usize {
    if b == 0 {
        0 // divide by zero → 0
    } else if b & 0xFFFF == 0 {
        1 // integer divisor
    } else if a as i16 as i32 == a {
        2 // small dividend, exact in i32
    } else {
        3 // i64 fallback
    }
}

#[test]
fn fx_div_is_bit_for_bit_fx_division_on_every_path() {
    let mut hit = [0usize; 4];
    for &a in SWEEP {
        for &b in SWEEP {
            hit[div_path(a, b)] += 1;
            assert_eq!(
                fx_div(a, b),
                (Fx::from_raw(a) / Fx::from_raw(b)).raw(),
                "fx_div({a:#x}, {b:#x})"
            );
        }
    }
    // All four paths of `impl Div` were actually exercised; a sweep that
    // missed one would pass while saying nothing about it.
    for (i, &n) in hit.iter().enumerate() {
        assert!(n > 0, "div path {i} never taken");
    }
    // The zero rule the oracle pinned, called out so it cannot be a
    // coincidence of the sweep.
    assert_eq!(fx_div(Fx::from_int(5).raw(), 0), 0);
    assert_eq!(fx_div(i32::MIN, 0), 0);
}

#[test]
fn fx_pow_is_bit_for_bit_fmath_pow() {
    for &a in SWEEP {
        for &b in SWEEP {
            assert_eq!(
                fx_pow(a, b),
                fmath::pow(Fx::from_raw(a), Fx::from_raw(b)).raw(),
                "fx_pow({a:#x}, {b:#x})"
            );
        }
    }
}

/// …and the interpreter agrees, through its own `op::DIV` / `op::POW`
/// arms. `fx_div`/`fx_pow` being literally `Fx::div`/`fmath::pow` makes the
/// two tests above tautological about the OPERATION; this one is what says
/// the helper is the same operation the pattern language performs, ABI and
/// all.
#[test]
fn div_and_pow_helpers_match_the_interpreters_own_arms() {
    let p = prog(
        "export function d(x, y) { return x / y }\n\
         export function q(x, y) { return x ** y }\n",
    );
    let (d, q) = (
        p.exported_fn("d").expect("d"),
        p.exported_fn("q").expect("q"),
    );
    let mut vm = vm_for(&p);
    for &a in SWEEP {
        for &b in SWEEP {
            let args = [Value::Num(Fx::from_raw(a)), Value::Num(Fx::from_raw(b))];
            let want_d = vm.call(&p, d, &args).expect("`/` cannot fail");
            assert_eq!(
                Value::Num(Fx::from_raw(fx_div(a, b))),
                want_d,
                "x / y at ({a:#x}, {b:#x})"
            );
            let want_q = vm.call(&p, q, &args).expect("`**` cannot fail");
            assert_eq!(
                Value::Num(Fx::from_raw(fx_pow(a, b))),
                want_q,
                "x ** y at ({a:#x}, {b:#x})"
            );
        }
    }
}

// ========================================================== arr_load_*

/// A pattern whose init indexes a three-element array with `{ix}`. The
/// index goes through a global so the test never has to re-derive the
/// literal's `Fx` word itself: whatever the compiler put in `ix` is exactly
/// what the interpreter indexed with, and it is what the helper is handed.
fn load_src(ix: &str) -> String {
    format!(
        "var a = array(3)\n\
         a[0] = 10\n\
         a[1] = 20\n\
         a[2] = 30\n\
         export var ix\n\
         export var out\n\
         ix = {ix}\n\
         out = a[ix]\n"
    )
}

#[test]
fn arr_load_num_matches_the_interpreter_value_for_value() {
    // Truncation toward zero and the two out-of-range ends. `2.999` and
    // `0.5` truncate down; `-0.5` is NEGATIVE, which the interpreter
    // refuses BEFORE truncating (truncating first would make it index 0).
    let (mut ok, mut bad) = (0, 0);
    for ix in ["0", "0.5", "1", "1.75", "2", "2.999", "3", "3.9", "-0.5", "-1", "1000"] {
        let p = prog(&load_src(ix));
        let mut vm = vm_for(&p);
        let want = init(&p, &mut vm);
        assert_eq!(vm.arena_slots(), 1, "`a` is arena entry 0 (ix = {ix})");
        let idx = g(&p, &vm, "ix").num().raw();

        let mut err = None;
        let at = match &want {
            Ok(_) => (0, 0),
            Err(e) => site(e),
        };
        let mut c = ctx_at(&mut vm, &p, &mut err, at);
        let got = unsafe { arr_load_num(&mut c, TAG_ARR, 0, idx) };
        let status = c.status;
        drop(c);

        match want {
            Ok(_) => {
                ok += 1;
                assert_eq!(status, STATUS_OK, "a[{ix}] should not fail");
                assert_eq!(outcome_of(status, &mut err), None);
                // The interpreter's answer is the global it stored.
                assert_eq!(Value::Num(Fx::from_raw(got)), g(&p, &vm, "out"), "a[{ix}]");
            }
            Err(e) => {
                bad += 1;
                assert_eq!(e.message, "array index out of bounds", "a[{ix}]");
                assert_eq!(got, 0, "a failing helper returns the default (ix = {ix})");
                assert_eq!(outcome_of(status, &mut err), Some(e), "a[{ix}]");
            }
        }
    }
    // Both halves of the sweep are real: a list that happened to be all
    // in-range (or all out of range) would prove only half of this.
    assert_eq!((ok, bad), (6, 5), "in-range / out-of-range split");
}

#[test]
fn arr_load_on_a_non_array_is_the_interpreters_error() {
    // A global proven `Num` may be indexed — `kinds::elem_kind` says so
    // explicitly, because indexing one always traps at run time. This is
    // the shape a budget-refused `array(pixelCount)` leaves behind, which
    // is where the error is actually seen in the field (Gitea #420).
    let p = prog(
        "var n = 5\n\
         export var ix\n\
         export var out\n\
         ix = 1\n\
         out = n[ix]\n",
    );
    let mut vm = vm_for(&p);
    let e = init(&p, &mut vm).expect_err("indexing a number must fail");
    assert_eq!(e.message, "indexing a non-array value");
    let idx = g(&p, &vm, "ix").num().raw();

    // Both helpers, both spellings of "not an array".
    for (tag, pay) in [(TAG_NUM, 5u32 << 16), (TAG_FUN, 0), (TAG_BUILTIN, 0)] {
        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, site(&e));
        let got = unsafe { arr_load_num(&mut c, tag, pay, idx) };
        let status = c.status;
        drop(c);
        assert_eq!(got, 0);
        assert_eq!(outcome_of(status, &mut err), Some(e.clone()), "tag {tag}");

        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, site(&e));
        let got = unsafe { arr_load_dyn(&mut c, tag, pay, idx) };
        let status = c.status;
        drop(c);
        assert_eq!(got, RetDyn::ZERO);
        assert_eq!(outcome_of(status, &mut err), Some(e.clone()), "tag {tag} dyn");
    }
}

#[test]
fn arr_load_dyn_returns_the_element_and_arr_load_num_coerces_it() {
    // `a` holds an ARRAY, so the element is not a `Num`: `arr_load_dyn`
    // hands back the handle, `arr_load_num` must coerce it to 0 the way
    // `Value::num` does rather than reinterpreting the arena id as a
    // number (docs/jit-design.md §2.3, last bullet).
    let p = prog(
        "var inner = array(2)\n\
         var a = array(1)\n\
         a[0] = inner\n\
         export var ix\n\
         export var out\n\
         ix = 0\n\
         out = a[ix]\n",
    );
    let mut vm = vm_for(&p);
    init(&p, &mut vm).expect("init");
    assert_eq!(vm.arena_slots(), 2);
    let want = g(&p, &vm, "out");
    assert!(matches!(want, Value::Arr(_)), "element is an array: {want:?}");
    let idx = g(&p, &vm, "ix").num().raw();

    let mut err = None;
    let mut c = ctx_at(&mut vm, &p, &mut err, (0, 0));
    let dynv = unsafe { arr_load_dyn(&mut c, TAG_ARR, 1, idx) };
    let numv = unsafe { arr_load_num(&mut c, TAG_ARR, 1, idx) };
    let status = c.status;
    drop(c);
    assert_eq!(outcome_of(status, &mut err), None);
    assert_eq!(dynv.to_value(), want, "arr_load_dyn returns the handle");
    assert_eq!(numv, want.num().raw(), "arr_load_num coerces through Value::num");
    assert_eq!(numv, 0, "…and `Value::num` of a handle is 0");
}

// =========================================================== arr_store

/// Two IDENTICAL arrays, so the interpreter can write into one and the
/// helper into the other and the answer is "the two arenas are equal".
/// Same program, same VM, same instruction — the only difference is who
/// performed the store.
fn store_pair(decl: &str, ix: &str, val: &str) -> (Program, Vm, Result<Value, VmError>, i32) {
    let p = prog(&format!(
        "var a = {decl}\n\
         var b = {decl}\n\
         export var ix\n\
         export var out\n\
         ix = {ix}\n\
         a[ix] = {val}\n\
         out = 1\n"
    ));
    let mut vm = vm_for(&p);
    let r = init(&p, &mut vm);
    let idx = g(&p, &vm, "ix").num().raw();
    (p, vm, r, idx)
}

#[test]
fn arr_store_writes_what_the_interpreter_writes() {
    for (decl, ix) in [
        ("array(3)", "0"),
        ("array(3)", "1"),
        ("array(3)", "2"),
        ("array(3)", "2.9"),   // truncates toward zero
        ("array(3)", "0.5"),   // …and so does a fraction below 1
        ("[1, 2, 3]", "1"),    // const-array: copy-on-write promotion
        ("[1, 2, 3]", "0.25"), // …promoted AND truncated
    ] {
        let (p, mut vm, r, idx) = store_pair(decl, ix, "9");
        r.unwrap_or_else(|e| panic!("`{decl}`[{ix}] = 9 failed: {}", e.message));
        assert_eq!(vm.arena_slots(), 2);

        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, (0, 0));
        unsafe { arr_store(&mut c, TAG_ARR, 1, idx, TAG_NUM, Fx::from_int(9).raw() as u32) };
        let status = c.status;
        drop(c);
        assert_eq!(outcome_of(status, &mut err), None);
        assert_eq!(arena(&p, &vm, 1), arena(&p, &vm, 0), "`{decl}`[{ix}] = 9");
    }
}

#[test]
fn arr_store_promotes_a_const_array_without_touching_the_pool() {
    let (p, mut vm, r, idx) = store_pair("[1, 2, 3]", "1", "9");
    r.expect("init");
    assert!(!p.pool.is_empty(), "`[1, 2, 3]` should be a const-pool array");
    let pool_before: Vec<u32> = p.pool_words(0).to_vec();

    let mut err = None;
    let mut c = ctx_at(&mut vm, &p, &mut err, (0, 0));
    unsafe { arr_store(&mut c, TAG_ARR, 1, idx, TAG_NUM, Fx::from_int(9).raw() as u32) };
    let status = c.status;
    drop(c);
    assert_eq!(outcome_of(status, &mut err), None);

    let want: Vec<Value> = [1, 9, 3].iter().map(|&x| Value::Num(Fx::from_int(x))).collect();
    assert_eq!(arena(&p, &vm, 0), want, "the interpreter's promoted copy");
    assert_eq!(arena(&p, &vm, 1), want, "the helper's promoted copy");
    // The pool is the PROGRAM's, shared by every occurrence of the literal
    // and by every VM running it; copy-on-write means neither store may
    // have reached it.
    assert_eq!(p.pool_words(0), &pool_before[..], "const pool was mutated");
    assert_eq!(
        pool_before,
        vec![
            Fx::from_int(1).raw() as u32,
            Fx::from_int(2).raw() as u32,
            Fx::from_int(3).raw() as u32
        ]
    );
}

#[test]
fn arr_store_out_of_bounds_matches_the_interpreter() {
    for ix in ["3", "3.5", "-0.5", "-1", "1000"] {
        let (p, mut vm, r, idx) = store_pair("array(3)", ix, "9");
        let e = r.expect_err("out of range");
        assert_eq!(e.message, "array index out of bounds", "ix = {ix}");

        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, site(&e));
        unsafe { arr_store(&mut c, TAG_ARR, 1, idx, TAG_NUM, Fx::from_int(9).raw() as u32) };
        let status = c.status;
        drop(c);
        assert_eq!(outcome_of(status, &mut err), Some(e), "ix = {ix}");
        // The refused store left the array alone, exactly as the arm does.
        assert_eq!(arena(&p, &vm, 1), arena(&p, &vm, 0), "ix = {ix}");
    }
}

#[test]
fn arr_store_into_a_non_array_matches_the_interpreter() {
    let p = prog(
        "var n = 5\n\
         export var ix\n\
         export var out\n\
         ix = 0\n\
         n[ix] = 9\n\
         out = 1\n",
    );
    let mut vm = vm_for(&p);
    let e = init(&p, &mut vm).expect_err("storing into a number must fail");
    assert_eq!(e.message, "indexing a non-array value");
    let idx = g(&p, &vm, "ix").num().raw();

    let mut err = None;
    let mut c = ctx_at(&mut vm, &p, &mut err, site(&e));
    unsafe { arr_store(&mut c, TAG_NUM, 5 << 16, idx, TAG_NUM, Fx::from_int(9).raw() as u32) };
    let status = c.status;
    drop(c);
    assert_eq!(outcome_of(status, &mut err), Some(e));
}

#[test]
fn arr_store_propagates_the_promotion_budget_error_verbatim() {
    // Two const entries cost `CONST_ENTRY_COST` (32 B) each; promoting one
    // to an owned three-element copy costs `3*8 + 32 - 32` more. A byte
    // budget one short of that is refused BEFORE anything is reserved
    // (Gitea #132), and the message is `Vm::arr_mut`'s, not the helper's.
    let src = "var a = [1, 2, 3]\n\
               var b = [1, 2, 3]\n\
               export var ix\n\
               export var out\n\
               ix = 1\n\
               a[ix] = 9\n\
               out = 1\n";
    let p = prog(src);
    let mut vm = vm_for(&p);
    vm.array_byte_budget = 32 + 32 + (3 * 8 + 32 - 32) - 1;
    let e = init(&p, &mut vm).expect_err("promotion must be refused");
    assert!(
        e.message.starts_with("array memory budget exceeded"),
        "unexpected error: {}",
        e.message
    );
    let idx = g(&p, &vm, "ix").num().raw();

    let mut err = None;
    let mut c = ctx_at(&mut vm, &p, &mut err, site(&e));
    unsafe { arr_store(&mut c, TAG_ARR, 1, idx, TAG_NUM, Fx::from_int(9).raw() as u32) };
    let status = c.status;
    drop(c);
    assert_eq!(outcome_of(status, &mut err), Some(e));
}

// ============================================================= arr_len

#[test]
fn arr_len_matches_the_interpreter() {
    for n in [0, 1, 3, 17] {
        let p = prog(&format!(
            "var a = array({n})\n\
             export var out\n\
             out = a.length\n"
        ));
        let mut vm = vm_for(&p);
        init(&p, &mut vm).expect("init");

        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, (0, 0));
        let got = unsafe { arr_len(&mut c, TAG_ARR, 0) };
        let status = c.status;
        drop(c);
        assert_eq!(outcome_of(status, &mut err), None);
        assert_eq!(Value::Num(Fx::from_raw(got)), g(&p, &vm, "out"), "array({n}).length");
    }
}

#[test]
fn arr_len_of_a_non_array_matches_the_interpreter() {
    let p = prog(
        "var n = 5\n\
         export var out\n\
         out = n.length\n",
    );
    let mut vm = vm_for(&p);
    let e = init(&p, &mut vm).expect_err("`.length` of a number must fail");
    assert_eq!(e.message, ".length of a non-array value");

    for (tag, pay) in [(TAG_NUM, 5u32 << 16), (TAG_FUN, 0), (TAG_BUILTIN, 3)] {
        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, site(&e));
        let got = unsafe { arr_len(&mut c, tag, pay) };
        let status = c.status;
        drop(c);
        assert_eq!(got, 0);
        assert_eq!(outcome_of(status, &mut err), Some(e.clone()), "tag {tag}");
    }
}

// =========================================================== new_array

/// `[x, 2, 3]` with a non-constant element is `op::NEW_ARRAY`; an
/// all-literal `[1, 2, 3]` would be `op::CONST_ARR` instead.
const NEW_ARRAY_SRC: &str = "export var x\n\
                             export var out\n\
                             x = 7\n\
                             var a = [x, 2, 3]\n\
                             out = 1\n";

#[test]
fn new_array_builds_what_the_interpreter_builds() {
    let p = prog(NEW_ARRAY_SRC);
    let mut vm = vm_for(&p);
    init(&p, &mut vm).expect("init");
    assert_eq!(vm.arena_slots(), 1);
    let want = arena(&p, &vm, 0);
    assert_eq!(want.len(), 3);

    let vals: Vec<ValueRaw> = want.iter().map(|v| v.raw()).collect();
    let mut err = None;
    let mut c = ctx_at(&mut vm, &p, &mut err, (0, 0));
    let id = unsafe { new_array(&mut c, vals.len() as u32, vals.as_ptr()) };
    let status = c.status;
    drop(c);
    assert_eq!(outcome_of(status, &mut err), None);
    assert_eq!(id, 1, "a second arena entry");
    assert_eq!(arena(&p, &vm, 1), want);
}

#[test]
fn new_array_of_zero_elements_never_reads_the_argument_pointer() {
    let p = prog("export var out\nout = 1\n");
    let mut vm = vm_for(&p);
    init(&p, &mut vm).expect("init");
    let mut err = None;
    let mut c = ctx_at(&mut vm, &p, &mut err, (0, 0));
    let id = unsafe { new_array(&mut c, 0, core::ptr::null()) };
    let status = c.status;
    drop(c);
    assert_eq!(outcome_of(status, &mut err), None);
    assert_eq!(arena(&p, &vm, id), Vec::<Value>::new());
}

#[test]
fn new_array_refuses_on_the_element_budget_with_the_interpreters_message() {
    // Budget-first: the arm allocates before it fills, so a refusal must
    // leave the arena untouched and report `Vm::alloc_array_zeroed`'s own
    // `String` (the one `is_array_budget_error` classifies for the
    // capacity model, Gitea #420).
    let p = prog(NEW_ARRAY_SRC);
    let mut vm = vm_for(&p);
    vm.array_budget = 3; // < 3 elements + ARRAY_HEADER_UNITS
    let e = init(&p, &mut vm).expect_err("the array must be refused");
    assert!(
        e.message.starts_with("array element budget exceeded"),
        "unexpected error: {}",
        e.message
    );
    assert_eq!(vm.arena_slots(), 0, "nothing was allocated");

    let vals = [
        Value::Num(Fx::from_int(7)).raw(),
        Value::Num(Fx::from_int(2)).raw(),
        Value::Num(Fx::from_int(3)).raw(),
    ];
    let mut err = None;
    let mut c = ctx_at(&mut vm, &p, &mut err, site(&e));
    let id = unsafe { new_array(&mut c, 3, vals.as_ptr()) };
    let status = c.status;
    drop(c);
    assert_eq!(id, 0, "a failing helper returns the default");
    assert_eq!(outcome_of(status, &mut err), Some(e));
    assert_eq!(vm.arena_slots(), 0, "the helper allocated nothing either");
}

// =========================================================== const_arr

const CONST_ARR_SRC: &str = "var a = [4, 5, 6, 7]\n\
                             export var out\n\
                             out = a.length\n";

#[test]
fn const_arr_allocates_what_the_interpreter_allocates() {
    let p = prog(CONST_ARR_SRC);
    let mut vm = vm_for(&p);
    init(&p, &mut vm).expect("init");
    assert_eq!(vm.arena_slots(), 1);
    let want = arena(&p, &vm, 0);

    let mut err = None;
    let mut c = ctx_at(&mut vm, &p, &mut err, (0, 0));
    let id = unsafe { const_arr(&mut c, 0) };
    let status = c.status;
    drop(c);
    assert_eq!(outcome_of(status, &mut err), None);
    assert_eq!(id, 1);
    assert_eq!(arena(&p, &vm, 1), want);
    // A const entry SHARES the pool, so the second allocation costs the
    // entry only — the same bookkeeping the arm gets.
    assert_eq!(vm.arena_slots(), 2);
}

#[test]
fn const_arr_refuses_on_the_element_budget_with_the_interpreters_message() {
    let p = prog(CONST_ARR_SRC);
    let mut vm = vm_for(&p);
    vm.array_budget = 4; // < 4 elements + ARRAY_HEADER_UNITS
    let e = init(&p, &mut vm).expect_err("the const array must be refused");
    assert!(
        e.message.starts_with("array element budget exceeded"),
        "unexpected error: {}",
        e.message
    );

    let mut err = None;
    let mut c = ctx_at(&mut vm, &p, &mut err, site(&e));
    let id = unsafe { const_arr(&mut c, 0) };
    let status = c.status;
    drop(c);
    assert_eq!(id, 0);
    assert_eq!(outcome_of(status, &mut err), Some(e));
    assert_eq!(vm.arena_slots(), 0);
}

// ==================================================== call_value_target

#[test]
fn call_value_target_resolves_a_function_to_its_native_entry() {
    let p = prog(
        "function one() { return 1 }\n\
         function two() { return 2 }\n\
         export var out\n\
         var f = one\n\
         out = f()\n",
    );
    let mut vm = vm_for(&p);
    init(&p, &mut vm).expect("init");
    assert!(p.fns.len() >= 2);

    // Stand-in native entries; the helper only reads the word.
    let table: Vec<usize> = (0..p.fns.len()).map(|i| 0x4008_0000 + i * 0x40).collect();
    for idx in 0..p.fns.len() {
        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, (0, 0));
        c.fn_table = table.as_ptr();
        let r = unsafe { call_value_target(&mut c, TAG_FUN, idx as u32) };
        let status = c.status;
        drop(c);
        assert_eq!(status, STATUS_OK, "resolving a Fun must not touch ctx.status");
        assert_eq!(outcome_of(status, &mut err), None);
        assert_eq!(r.status, CALL_TARGET_NATIVE);
        assert_eq!(r.val, table[idx] as i32, "fn {idx}'s native entry");
    }
}

#[test]
fn call_value_target_passes_a_builtin_id_through() {
    let p = prog("export var out\nout = 1\n");
    let mut vm = vm_for(&p);
    init(&p, &mut vm).expect("init");
    let table: Vec<usize> = vec![0x4008_0000; p.fns.len()];
    for id in [0u32, 1, 42, 187] {
        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, (0, 0));
        c.fn_table = table.as_ptr();
        let r = unsafe { call_value_target(&mut c, TAG_BUILTIN, id) };
        let status = c.status;
        drop(c);
        assert_eq!(status, STATUS_OK, "resolving a Builtin must not touch ctx.status");
        assert_eq!(outcome_of(status, &mut err), None);
        assert_eq!(r.status, CALL_TARGET_BUILTIN);
        assert_eq!(r.val, id as i32, "the id the emitter indexes ctx.builtins with");
    }
}

#[test]
fn call_value_target_refuses_a_non_function_like_the_interpreter() {
    let p = prog(
        "export var out\n\
         var f = 5\n\
         out = f()\n",
    );
    let mut vm = vm_for(&p);
    let e = init(&p, &mut vm).expect_err("calling a number must fail");
    assert_eq!(e.message, "call of a non-function value");

    let table: Vec<usize> = vec![0x4008_0000; p.fns.len()];
    // Not callable at all…
    for (tag, pay) in [(TAG_NUM, 5u32 << 16), (TAG_ARR, 0)] {
        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, site(&e));
        c.fn_table = table.as_ptr();
        let r = unsafe { call_value_target(&mut c, tag, pay) };
        let status = c.status;
        drop(c);
        assert_eq!((r.val, r.status), (0, CALL_TARGET_ERR));
        assert_eq!(outcome_of(status, &mut err), Some(e.clone()), "tag {tag}");
    }

    // …and a `Fun` handle the table cannot answer for. The interpreter
    // cannot produce one (the decoder validates every `ConstFun` index),
    // but a helper handed a corrupt handle must refuse rather than read
    // past the end of `ctx.fn_table`.
    for (label, fn_table) in [
        ("overrun", table.as_ptr()),
        ("no table", core::ptr::null::<usize>()),
    ] {
        let payload = if label == "overrun" { p.fns.len() as u32 } else { 0 };
        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, site(&e));
        c.fn_table = fn_table;
        let r = unsafe { call_value_target(&mut c, TAG_FUN, payload) };
        let status = c.status;
        drop(c);
        assert_eq!((r.val, r.status), (0, CALL_TARGET_ERR), "{label}");
        assert_eq!(outcome_of(status, &mut err), Some(e.clone()), "{label}");
    }
}

#[test]
fn the_call_target_discriminants_are_distinct_and_documented() {
    // The emitter branches on these three and nothing else.
    assert_eq!(
        (CALL_TARGET_NATIVE, CALL_TARGET_BUILTIN, CALL_TARGET_ERR),
        (0, 1, 2)
    );
}

// =============================================================== bails

#[test]
fn assert_fail_reproduces_the_interpreters_assertion_error() {
    let p = prog(&format!(
        "assert(pixelCount >= {}, \"needs a bigger rig\")\n\
         export var out\n\
         out = 1\n",
        PX + 1
    ));
    let mut vm = vm_for(&p);
    let e = init(&p, &mut vm).expect_err("the assertion must fail");
    assert_eq!(e.message, format!("pattern requires: needs a bigger rig (pixelCount = {PX})"));
    assert!(e.is_assert, "an assertion is not a bug");
    assert_eq!(p.assert_msgs.len(), 1);

    // A fresh VM, so `assert_failed` re-reads `pixelCount` itself rather
    // than inheriting anything the failed run left behind.
    let mut vm2 = vm_for(&p);
    let mut err = None;
    let mut c = ctx_at(&mut vm2, &p, &mut err, site(&e));
    unsafe { assert_fail(&mut c, 0) };
    let status = c.status;
    drop(c);
    assert_eq!(outcome_of(status, &mut err), Some(e));
}

#[test]
fn assert_fail_quotes_the_live_pixel_count() {
    // The message is built from VM state, not baked at compile time: two
    // VMs of the same program must say different things.
    // 30000 and not 100000: the threshold is an `Fx`, whose range stops at
    // 32767.99998.
    let p = prog("assert(pixelCount >= 30000)\nexport var out\nout = 1\n");
    for px in [1, 64, 4096] {
        let mut vm = Vm::new(&p, 1);
        vm.globals[p.pixel_count_g as usize] = Value::Num(Fx::from_int(px));
        let e = init(&p, &mut vm).expect_err("the assertion must fail");

        let mut vm2 = Vm::new(&p, 1);
        vm2.globals[p.pixel_count_g as usize] = Value::Num(Fx::from_int(px));
        let mut err = None;
        let mut c = ctx_at(&mut vm2, &p, &mut err, site(&e));
        unsafe { assert_fail(&mut c, 0) };
        let status = c.status;
        drop(c);
        let got = outcome_of(status, &mut err).expect("assert_fail always fails");
        assert!(got.message.contains(&format!("pixelCount = {px}")), "{}", got.message);
        assert_eq!(got, e);
    }
}

#[test]
fn bail_depth_is_the_interpreters_depth_error() {
    let p = prog(
        "function f(n) { return f(n + 1) }\n\
         export var out\n\
         out = f(0)\n",
    );
    let mut vm = vm_for(&p);
    let e = init(&p, &mut vm).expect_err("unbounded recursion must fail");
    assert_eq!(e.message, "call depth exceeded");

    let mut vm2 = vm_for(&p);
    let mut err = None;
    let mut c = ctx_at(&mut vm2, &p, &mut err, site(&e));
    unsafe { bail_depth(&mut c) };
    let status = c.status;
    drop(c);
    let got = outcome_of(status, &mut err).expect("bail_depth always fails");
    assert!(!got.is_assert, "a depth bail is a bug, not an assertion");
    assert_eq!(got, e);
}

#[test]
fn bail_fuel_is_the_interpreters_execution_limit() {
    let p = prog("export var out\nout = 1\n");
    let mut vm = vm_for(&p);
    let mut err = None;
    let mut c = ctx_at(&mut vm, &p, &mut err, (0, 7));
    unsafe { bail_fuel(&mut c) };
    let status = c.status;
    drop(c);
    let got = outcome_of(status, &mut err).expect("bail_fuel always fails");
    assert_eq!(got.message, "execution limit exceeded (infinite loop?)");
    // …and it is the same CONST the interpreter's `fail!` site uses, which
    // is what keeps the engine treating it as frame-fatal rather than as an
    // ordinary pattern error (`ERR_EXEC_LIMIT` is crate-private, so this is
    // the public way to tie the two together).
    assert!(got.is_resource_guard(), "must classify as a resource guard");
    assert!(!got.is_assert);
    assert_eq!((got.fn_idx, got.pc), (0, 7), "attributed from the ctx");
}

#[test]
fn a_bail_attributes_from_the_context_not_from_a_frame() {
    // The whole point of §3.6: with no interpreter frame, `Vm::err_at`
    // would say `fn_idx: u16::MAX, pc: u32::MAX`. Every helper reports
    // through `ctx_err` instead, and a source-position run makes the
    // line/col lookup observable.
    let p = prog(
        "export var out\n\
         out = 1\n\
         out = 2\n\
         out = 3\n",
    );
    let mut vm = vm_for(&p);
    let code_len = p.fns[0].code_len;
    for pc in 0..code_len {
        let mut err = None;
        let mut c = ctx_at(&mut vm, &p, &mut err, (0, pc));
        unsafe { bail_depth(&mut c) };
        let status = c.status;
        drop(c);
        let got = outcome_of(status, &mut err).expect("always fails");
        assert_eq!((got.fn_idx, got.pc), (0, pc));
        assert_eq!((got.line, got.col), p.fns[0].pos_at(pc), "pos_at({pc})");
    }
    // A context that was never given a site still produces a well-formed
    // error rather than panicking on the out-of-range `fn_idx`.
    let mut err = None;
    let mut c = JitCtx::for_builtin_call(&mut vm, &p, &mut err);
    unsafe { bail_fuel(&mut c) };
    let status = c.status;
    drop(c);
    let got = outcome_of(status, &mut err).expect("always fails");
    assert_eq!((got.fn_idx, got.line, got.col), (u16::MAX, 0, 0));
}
