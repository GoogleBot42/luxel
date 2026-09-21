//! [`BUILTIN_ENTRIES`] — one [`BuiltinEntry`] per builtin id, in
//! [`BUILTINS`] order (docs/jit-design.md §4).
//!
//! Two entry points per builtin:
//!
//! - **`generic`** — `extern "C" fn(*mut JitCtx, *const Value, u32) ->
//!   RetDyn`. Works for any builtin at any arity. The emitter boxes the
//!   call's arguments into the frame's scratch `[Value; n]` (§3.3), passes
//!   the pointer and the count, and reads the two-word result. The wrapper
//!   marshals exactly as the interpreter does — missing arguments read
//!   `Num(0)`, extras are dropped, `argc` is capped at [`MAX_ARGS`] — and
//!   then runs the interpreter's OWN ladder (`Vm::builtin_ladder`:
//!   `builtin_fast`, else `builtin_hot`/`builtin_cold`). There is no second
//!   implementation of any builtin anywhere in the JIT.
//! - **`direct`** — the tier-1 numeric set of §3.5, as raw 16.16 words in
//!   registers with no boxing at all. `direct_sig` says which signature the
//!   `direct` word has; `DirectSig::None` (and a zero word) means this
//!   builtin has no direct form and the emitter must use `generic`.
//!
//! Errors never come back in the return value: a failing call sets
//! `ctx.status` to [`STATUS_ERR`] and fills `*ctx.err` (§3.2). That is what
//! lets every `generic` have one shape, tombstones included — a `Removed`
//! or `Todo` id's wrapper raises the same error the interpreter raises.

use crate::fixed::Fx;
use crate::fmath;
use crate::jit::ctx::JitCtx;
use crate::vm::{builtin_sig, BKind, Builtin, SigRet, Value, ValueRaw, Vm, BUILTINS, MAX_ARGS};

// ------------------------------------------------------------- return ABI

/// A boxed [`Value`] returned by value in two words — `a2:a3` on Xtensa
/// (docs/jit-design.md §3.2). Byte-identical to [`ValueRaw`], which is
/// [`Value`]'s pinned image; the separate type exists so a function
/// signature says "this is the ABI return", not "this is a value in
/// memory".
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetDyn {
    pub tag: u32,
    pub payload: u32,
}

const _: () = assert!(core::mem::size_of::<RetDyn>() == 8);
const _: () = assert!(core::mem::align_of::<RetDyn>() == 4);
const _: () = assert!(core::mem::offset_of!(RetDyn, tag) == 0);
const _: () = assert!(core::mem::offset_of!(RetDyn, payload) == 4);

impl RetDyn {
    /// `Num(0)` — what a failing call returns beside a non-zero status.
    pub const ZERO: RetDyn = RetDyn { tag: 0, payload: 0 };

    #[inline]
    pub const fn from_value(v: Value) -> RetDyn {
        let r = v.raw();
        RetDyn { tag: r.tag, payload: r.payload }
    }

    #[inline]
    pub const fn to_value(self) -> Value {
        Value::from_raw(ValueRaw { tag: self.tag, payload: self.payload })
    }
}

// ----------------------------------------------------------- return kinds

/// `ret_kind`: `Dyn`. Matches the LXBC kind byte (docs/jit-design.md §2.2).
pub const RET_DYN: u8 = 0;
/// `ret_kind`: a number. The LXBC `Num` kind byte.
pub const RET_NUM: u8 = 1;
/// `ret_kind`: a freshly allocated all-numeric array (`array(n)`). The
/// LXBC `ArrNum` kind byte.
pub const RET_NEW_ARRNUM: u8 = 3;
/// `ret_kind`: argument `n` verbatim — `RET_ARG_BASE | n`. Above every kind
/// byte, so the low values stay interchangeable with the LXBC encoding.
pub const RET_ARG_BASE: u8 = 0x80;

/// [`BuiltinEntry::ret_kind`] for builtin `id`, derived at COMPILE TIME
/// from [`builtin_sig`] — the phase-0 signature table stays the single
/// source of truth, and `kinds`/`jitlint` keep reading it unchanged.
const fn ret_kind_of(id: u16) -> u8 {
    match builtin_sig(id).ret {
        SigRet::Num => RET_NUM,
        SigRet::NewArrNum => RET_NEW_ARRNUM,
        SigRet::Dyn => RET_DYN,
        SigRet::Arg(n) => RET_ARG_BASE | n,
    }
}

// ------------------------------------------------------- direct signature

/// Which raw-word signature a [`BuiltinEntry::direct`] word has
/// (docs/jit-design.md §3.5). `N<n>` = `n` numeric arguments and no
/// context; `C<n>` = a `*mut JitCtx` first, then `n` numeric arguments.
/// Every one returns a single raw 16.16 word and cannot fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DirectSig {
    /// No direct entry point — use `generic`.
    None = 0,
    N1 = 1,
    N2 = 2,
    N3 = 3,
    N4 = 4,
    C0 = 5,
    C1 = 6,
    C2 = 7,
    C3 = 8,
}

/// The `direct` word: a function pointer of the shape [`DirectSig`] names,
/// or `0`.
///
/// **Deviation from docs/jit-design.md §4**, which spells this field
/// `direct: usize`: a function pointer cannot be cast to an integer during
/// const evaluation, so a `usize` field could not be initialised in a
/// `static`. A one-word `#[repr(C)]` union is the same word with the same
/// meaning (`none == 0` ⇔ no direct entry) and stays const-initialisable.
/// The emitter wants the address, which [`BuiltinEntry::direct_addr`]
/// hands over.
#[repr(C)]
#[derive(Clone, Copy)]
pub union Direct {
    /// The "no direct form" encoding, and how the emitter reads the word.
    pub none: usize,
    pub n1: unsafe extern "C" fn(i32) -> i32,
    pub n2: unsafe extern "C" fn(i32, i32) -> i32,
    pub n3: unsafe extern "C" fn(i32, i32, i32) -> i32,
    pub n4: unsafe extern "C" fn(i32, i32, i32, i32) -> i32,
    pub c0: unsafe extern "C" fn(*mut JitCtx) -> i32,
    pub c1: unsafe extern "C" fn(*mut JitCtx, i32) -> i32,
    pub c2: unsafe extern "C" fn(*mut JitCtx, i32, i32) -> i32,
    pub c3: unsafe extern "C" fn(*mut JitCtx, i32, i32, i32) -> i32,
}

const _: () = assert!(core::mem::size_of::<Direct>() == core::mem::size_of::<usize>());

// ------------------------------------------------------------- the entry

/// One builtin's native entry points (docs/jit-design.md §4).
#[repr(C)]
pub struct BuiltinEntry {
    /// The any-arity wrapper. Never null, for every id including
    /// tombstones.
    pub generic: unsafe extern "C" fn(*mut JitCtx, *const Value, u32) -> RetDyn,
    /// The raw-word entry point, or the zero word.
    pub direct: Direct,
    /// [`DirectSig`] as a `u8`.
    pub direct_sig: u8,
    /// `RET_*` — what a call pushes, from [`builtin_sig`].
    pub ret_kind: u8,
}

// A `*const BuiltinEntry` crosses into generated code (`JitCtx::builtins`),
// so it has to be sharable. Nothing in it is ever written.
unsafe impl Sync for BuiltinEntry {}

impl BuiltinEntry {
    /// The address the emitter puts in a literal, or 0 when there is no
    /// direct form. Not `const`: reading a function pointer as an integer
    /// is a run-time-only operation.
    #[inline]
    pub fn direct_addr(&self) -> usize {
        // SAFETY: every variant of `Direct` is one pointer-sized word and
        // `none` is the integer reading of it.
        unsafe { self.direct.none }
    }

    /// [`DirectSig`] of this entry.
    #[inline]
    pub fn sig(&self) -> DirectSig {
        match self.direct_sig {
            1 => DirectSig::N1,
            2 => DirectSig::N2,
            3 => DirectSig::N3,
            4 => DirectSig::N4,
            5 => DirectSig::C0,
            6 => DirectSig::C1,
            7 => DirectSig::C2,
            8 => DirectSig::C3,
            _ => DirectSig::None,
        }
    }
}

// ----------------------------------------------------- generic wrappers

/// The body every `generic` wrapper shares. Out of line so the 188
/// wrappers are 188 thunks (`movi` the id, jump) rather than 188 copies of
/// the marshalling — the #328 I-cache lesson applies to the table as much
/// as to the interpreter.
///
/// # Safety
/// `ctx` must be a live [`JitCtx`] whose `vm`, `prog` and `err` point at
/// live objects; `args` must address at least `min(argc, MAX_ARGS)`
/// [`Value`]s.
#[inline(never)]
unsafe fn generic_call(ctx: *mut JitCtx, id: u16, args: *const Value, argc: u32) -> RetDyn {
    let c = &mut *ctx;
    // The interpreter's own marshalling (`Vm::pop_args_into` + the
    // `[Value; MAX_ARGS]` buffer): cap at MAX_ARGS, missing slots read
    // `Num(0)`, extras are dropped.
    let n = (argc as usize).min(MAX_ARGS);
    let mut buf = [Value::default(); MAX_ARGS];
    let mut i = 0;
    while i < n {
        buf[i] = *args.add(i);
        i += 1;
    }
    let def = &BUILTINS[id as usize];
    let r = match def.kind {
        BKind::Impl(b) => {
            let vm = &mut *c.vm;
            let prog = &*c.prog;
            vm.builtin_ladder(prog, id, b, &buf, n)
        }
        // A tombstoned or unimplemented id. Unreachable through a loaded
        // blob (the decoder rejects the import, the compiler never emits
        // one), and it raises exactly the interpreter's error.
        BKind::Todo | BKind::Removed => Err(Vm::builtin_unimplemented(def.name)),
    };
    match r {
        Ok(v) => {
            c.status = crate::jit::STATUS_OK;
            RetDyn::from_value(v)
        }
        Err(e) => {
            c.fail(e);
            RetDyn::ZERO
        }
    }
}

unsafe extern "C" fn generic<const ID: u16>(
    ctx: *mut JitCtx,
    args: *const Value,
    argc: u32,
) -> RetDyn {
    generic_call(ctx, ID, args, argc)
}

// ------------------------------------------------------- direct wrappers

/// The pure tier-1 arms, on raw 16.16 words. Each is the body of the
/// matching arm of `Vm::builtin_fast` with the boxing removed;
/// `tests/jitabi.rs` sweeps every one of them against `generic` (which runs
/// that arm) over negatives, zero, ±1 and the `Fx` extremes.
macro_rules! direct_pure {
    ($($name:ident ($($a:ident),*) = $body:expr;)*) => {$(
        unsafe extern "C" fn $name($($a: i32),*) -> i32 {
            $(let $a = Fx::from_raw($a);)*
            let r: Fx = $body;
            r.raw()
        }
    )*};
}

direct_pure! {
    d_abs(a) = a.abs();
    d_floor(a) = a.floor();
    d_ceil(a) = a.ceil();
    d_round(a) = a.round();
    d_trunc(a) = a.trunc();
    d_frac(a) = a.frac();
    d_sqrt(a) = fmath::sqrt(a);
    d_sin(a) = fmath::sin(a);
    d_cos(a) = fmath::cos(a);
    d_min(a, b) = a.min(b);
    d_max(a, b) = a.max(b);
    d_mod(a, b) = a.mod_floor(b);
    d_clamp(a, lo, hi) = a.clamp(lo, hi);
    d_mix(a, b, t) = a + (b - a) * t;
    d_wave(a) = Fx::from_raw((fmath::sin_turns(a).raw() + Fx::ONE.raw()) >> 1);
    d_triangle(a) = {
        let t = a.wrap_unit();
        let half = Fx::from_raw(1 << 15);
        if t < half { t + t } else { (Fx::ONE - t) + (Fx::ONE - t) }
    };
    // `square(t)` defaults its duty to 0.5 (`builtin_fast`'s `argc >= 2`
    // test). The direct form takes the EFFECTIVE arguments, so a call site
    // with one argument materialises `Fx::from_raw(1 << 15)` itself — the
    // emitter knows `argc` statically.
    d_square(t, duty) = {
        let t = t.wrap_unit();
        if t < duty { Fx::ONE } else { Fx::ZERO }
    };
}

/// The context-taking tier-1 arms. These reach the VM (the RNG, the clock,
/// the pixel brush), so rather than restate the arm they CALL it:
/// `Vm::builtin_fast` is `#[inline(always)]` and `b` is a constant at each
/// site, so the match folds to the one arm and nothing is duplicated.
///
/// # Safety
/// `ctx.vm` must be live; these are only ever reached from generated code.
#[inline(always)]
unsafe fn ctx_fast(ctx: *mut JitCtx, b: Builtin, a: [i32; 3], argc: usize) -> i32 {
    let vm = &mut *(*ctx).vm;
    let args = [
        Value::Num(Fx::from_raw(a[0])),
        Value::Num(Fx::from_raw(a[1])),
        Value::Num(Fx::from_raw(a[2])),
        Value::Num(Fx::ZERO),
    ];
    match vm.builtin_fast(b, args, argc) {
        Some(v) => v.num().raw(),
        // unreachable: every `b` passed here is a tier-1 arm
        None => 0,
    }
}

unsafe extern "C" fn d_random(ctx: *mut JitCtx, max: i32) -> i32 {
    ctx_fast(ctx, Builtin::Random, [max, 0, 0], 1)
}
unsafe extern "C" fn d_prng(ctx: *mut JitCtx, max: i32) -> i32 {
    ctx_fast(ctx, Builtin::Prng, [max, 0, 0], 1)
}
unsafe extern "C" fn d_time(ctx: *mut JitCtx, period: i32) -> i32 {
    ctx_fast(ctx, Builtin::Time, [period, 0, 0], 1)
}
unsafe extern "C" fn d_hsv(ctx: *mut JitCtx, h: i32, s: i32, v: i32) -> i32 {
    ctx_fast(ctx, Builtin::Hsv, [h, s, v], 3)
}
unsafe extern "C" fn d_rgb(ctx: *mut JitCtx, r: i32, g: i32, b: i32) -> i32 {
    ctx_fast(ctx, Builtin::Rgb, [r, g, b], 3)
}

/// The tier-1 set of docs/jit-design.md §3.5, keyed on the BUILTIN rather
/// than on the name so the aliases come along — `fract` is `Frac`, `lerp`
/// is `Mix`, `hsv24` is `Hsv`.
const fn direct_of(id: u16) -> (Direct, DirectSig) {
    let i = id as usize;
    let b = match BUILTINS[i].kind {
        BKind::Impl(b) => b,
        _ => return (Direct { none: 0 }, DirectSig::None),
    };
    match b {
        Builtin::Abs => (Direct { n1: d_abs }, DirectSig::N1),
        Builtin::Floor => (Direct { n1: d_floor }, DirectSig::N1),
        Builtin::Ceil => (Direct { n1: d_ceil }, DirectSig::N1),
        Builtin::Round => (Direct { n1: d_round }, DirectSig::N1),
        Builtin::Trunc => (Direct { n1: d_trunc }, DirectSig::N1),
        Builtin::Frac => (Direct { n1: d_frac }, DirectSig::N1),
        Builtin::Sqrt => (Direct { n1: d_sqrt }, DirectSig::N1),
        Builtin::Sin => (Direct { n1: d_sin }, DirectSig::N1),
        Builtin::Cos => (Direct { n1: d_cos }, DirectSig::N1),
        Builtin::Wave => (Direct { n1: d_wave }, DirectSig::N1),
        Builtin::Triangle => (Direct { n1: d_triangle }, DirectSig::N1),
        Builtin::Min => (Direct { n2: d_min }, DirectSig::N2),
        Builtin::Max => (Direct { n2: d_max }, DirectSig::N2),
        Builtin::Mod => (Direct { n2: d_mod }, DirectSig::N2),
        Builtin::Square => (Direct { n2: d_square }, DirectSig::N2),
        Builtin::Clamp => (Direct { n3: d_clamp }, DirectSig::N3),
        Builtin::Mix => (Direct { n3: d_mix }, DirectSig::N3),
        Builtin::Random => (Direct { c1: d_random }, DirectSig::C1),
        Builtin::Prng => (Direct { c1: d_prng }, DirectSig::C1),
        Builtin::Time => (Direct { c1: d_time }, DirectSig::C1),
        Builtin::Hsv => (Direct { c3: d_hsv }, DirectSig::C3),
        Builtin::Rgb => (Direct { c3: d_rgb }, DirectSig::C3),
        _ => (Direct { none: 0 }, DirectSig::None),
    }
}

// --------------------------------------------------------------- the table

const fn entry<const ID: u16>() -> BuiltinEntry {
    let (direct, sig) = direct_of(ID);
    BuiltinEntry {
        generic: generic::<ID>,
        direct,
        direct_sig: sig as u8,
        ret_kind: ret_kind_of(ID),
    }
}

/// `entry::<0>(), entry::<1>(), …` — a const-generic instantiation per id,
/// because a function pointer is a distinct symbol per id and const
/// evaluation cannot build one from a loop counter.
///
/// The list is APPEND-ONLY, exactly like [`BUILTINS`]: a new builtin adds
/// its id at the end. The length assertion below is the guard — a builtin
/// appended without a line here fails the build.
macro_rules! entries {
    ($($i:literal),* $(,)?) => { [ $( entry::<$i>() ),* ] };
}

/// One entry per builtin id, indexed by id ([`BUILTINS`] order).
#[rustfmt::skip]
pub static BUILTIN_ENTRIES: [BuiltinEntry; 188] = entries![
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
    23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43,
    44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64,
    65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85,
    86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104,
    105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120,
    121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136,
    137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152,
    153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168,
    169, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184,
    185, 186, 187,
];

/// The table and the name table are the same length, and the same ids.
const _: () = assert!(BUILTIN_ENTRIES.len() == BUILTINS.len());
