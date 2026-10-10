//! The inline builtins of Gitea #942, differentially.
//!
//! `abs floor ceil round trunc frac min max clamp saturate mix mod sign
//! step square triangle` (and the `fract`/`lerp` aliases) are emitted as
//! instructions rather than a `callx8` to their `direct` entry. The
//! definition of correct is that entry — the Rust kernel in
//! `luxel_core::jit::table` — so every call site shape below is compiled,
//! run through the ISA model over the `i32` edges and pseudo-random words,
//! and compared word for word with the kernel AND with the interpreter.
//! The shapes put the operands everywhere the emitter can find them:
//! register homes, frame homes (a deep stack), `Dyn` slots, trailing
//! `CallBuiltinC`/`CC` immediates, one register read three times, and a
//! nested call whose operand is the previous result.

mod common;

use common::bridge::{fresh_vm, Bridge};
use common::{env_with_fake_addresses, program_of};

use luxel_core::fixed::Fx;
use luxel_core::jit::{direct_default, DirectSig, BUILTIN_ENTRIES};
use luxel_core::vm::{Program, Value, BUILTINS};

const STEP_LIMIT: u64 = 100_000;
/// Random input tuples per shape. The ISA model is ~20x slower unoptimised,
/// and the gate runs `cargo test` in debug — the release run
/// (`--release`, docs/jit-design.md §7.1) gets ten times as many.
const RANDOMS: usize = if cfg!(debug_assertions) { 300 } else { 3000 };

/// Every raw word a 16.16 kernel tends to get wrong: the i32 extremes,
/// the zero crossings, the half and unit boundaries either side of zero.
const EDGES: [i32; 27] = [
    0,
    1,
    -1,
    2,
    -2,
    i32::MIN,
    i32::MIN + 1,
    i32::MAX,
    i32::MAX - 1,
    0x7fff,
    0x8000,
    0x8001,
    0xffff,
    0x1_0000,
    0x1_0001,
    -0x8000,
    -0x8001,
    -0xffff,
    -0x1_0000,
    -0x1_0001,
    0x1_8000,
    -0x1_8000,
    0x1234_5678,
    -0x1234_5678,
    3 << 16,
    -(3 << 16),
    0x4000,
];

/// A small xorshift — deterministic, so a failure reproduces.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> i32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        let w = (self.0 >> 16) as u32 as i32;
        // Half full-range words, half within ±8.0 where patterns live.
        if self.0 & 1 == 0 {
            w
        } else {
            w >> 12
        }
    }
}

fn id_of(name: &str) -> u16 {
    BUILTINS
        .iter()
        .position(|d| d.name == name)
        .unwrap_or_else(|| panic!("no builtin {name}")) as u16
}

/// The Rust kernel: `BUILTIN_ENTRIES[id].direct`, at the effective
/// arguments (a short `square` gets its default duty, as the direct path
/// materialises it).
fn kernel(id: u16, args: &[i32]) -> i32 {
    let e = &BUILTIN_ENTRIES[id as usize];
    let d = e.direct;
    let mut a = args.to_vec();
    let n = match e.sig() {
        DirectSig::N1 => 1,
        DirectSig::N2 => 2,
        DirectSig::N3 => 3,
        s => panic!("builtin {id} has direct signature {s:?}"),
    };
    if a.len() + 1 == n {
        a.push(direct_default(id).expect("a short call has a default"));
    }
    assert_eq!(a.len(), n);
    // SAFETY: pure numeric kernels, called at their own signature.
    unsafe {
        match n {
            1 => (d.n1)(a[0]),
            2 => (d.n2)(a[0], a[1]),
            _ => (d.n3)(a[0], a[1], a[2]),
        }
    }
}

/// One compiled call-site shape.
struct Site {
    src: String,
    prog: Program,
    img: luxel_jit::NativeImage,
    f: usize,
    out: usize,
    /// What the kernel should be fed for parameters `(a, b, c)`, or `None`
    /// when only the interpreter is the reference (the nested shape).
    feed: fn(&[i32; 3], usize) -> Option<Vec<i32>>,
}

fn site(prelude: &str, body: &str, feed: fn(&[i32; 3], usize) -> Option<Vec<i32>>) -> Site {
    let src = format!(
        "var out = 0\n{prelude}\nexport function f(a, b, c) {{ {body} }}\nexport function render(i) {{ f(i, i, i) }}\n"
    );
    let (prog, kinds) = program_of(&src).unwrap_or_else(|e| panic!("{src}\n{e}"));
    let img = luxel_jit::compile(&prog, &kinds, &luxel_env())
        .unwrap_or_else(|e| panic!("{src}\nrefused: {}", e.reason()));
    let f = prog.exported_fn("f").unwrap() as usize;
    let out = prog.globals.iter().position(|g| g.name == "out").unwrap();
    Site {
        src,
        prog,
        img,
        f,
        out,
        feed,
    }
}

fn luxel_env() -> luxel_jit::Env {
    env_with_fake_addresses()
}

/// Run every input through one site, natively and interpreted.
fn drive(site: &Site, id: u16, argc: usize, inputs: &[[i32; 3]], inline: bool) {
    let mut vi = fresh_vm(&site.prog, 60, 7);
    let _ = vi.call(&site.prog, 0, &[]);
    let mut b = Bridge::new(&site.prog, &site.img, fresh_vm(&site.prog, 60, 7));
    b.enter(0, &[]);
    b.run(STEP_LIMIT).expect("init");
    // init's own calls (`array(2)`) are not the site under test
    b.generic_calls = 0;
    b.direct_calls = 0;
    let mut native_steps = 0;
    for p in inputs {
        let args: Vec<Value> = p.iter().map(|&w| Value::Num(Fx::from_raw(w))).collect();
        let ei = vi.call(&site.prog, site.f as u16, &args).err();
        assert!(ei.is_none(), "{}\ninterpreter failed on {p:?}", site.src);
        b.enter(site.f, p);
        b.run(STEP_LIMIT)
            .unwrap_or_else(|e| panic!("{}\n{p:?} trapped: {e:?}", site.src));
        assert!(!b.failed(), "{}\nnative failed on {p:?}", site.src);
        native_steps += b.last_steps();
        let got = num_of(b.vm.globals[site.out]);
        let interp = num_of(vi.globals[site.out]);
        assert_eq!(
            got, interp,
            "{}\n{p:?}: native {got:#x}, interpreter {interp:#x}",
            site.src
        );
        if let Some(k) = (site.feed)(p, argc) {
            let want = kernel(id, &k);
            assert_eq!(
                got, want,
                "{}\n{p:?} (kernel fed {k:?}): native {got:#x}, kernel {want:#x}",
                site.src
            );
        }
    }
    assert!(native_steps > 0, "{}: nothing ran", site.src);
    if inline {
        assert_eq!(
            (b.direct_calls, b.generic_calls),
            (0, 0),
            "{}: an inline builtin still called out",
            site.src
        );
    }
}

fn shapes(name: &str, argc: usize) -> Vec<Site> {
    let ps = ["a", "b", "c"];
    let call = format!("{name}({})", ps[..argc].join(", "));
    let mut v = Vec::new();
    // Operands in register homes (depths 0, 1) and the frame (depth 2).
    v.push(site("", &format!("out = {call}"), |p, n| {
        Some(p[..n].to_vec())
    }));
    // A deep stack: every operand and the result in a frame home. `x * 0`
    // is 0 for every word, so the kernel's answer is unchanged.
    v.push(site(
        "",
        &format!("out = c * 0 + (b * 0 + (a * 0 + {call}))"),
        |p, n| Some(p[..n].to_vec()),
    ));
    // A `Dyn` first operand: an array half the time, which `Value::num`
    // reads as 0.
    v.push(site(
        "var pal = array(2)\n",
        &format!(
            "var d = b > 3000 ? pal : a; out = {name}({})",
            ["d", "b", "c"][..argc].join(", ")
        ),
        |p, n| {
            let mut k = p[..n].to_vec();
            if p[1] > 3000 << 16 {
                k[0] = 0;
            }
            Some(k)
        },
    ));
    // One register read for every operand.
    v.push(site(
        "",
        &format!("out = {name}({})", ["a"; 3][..argc].join(", ")),
        |p, n| Some(vec![p[0]; n]),
    ));
    // The result fed back in: operand 0 is the previous call's register.
    v.push(site(
        "",
        &format!(
            "out = {name}({})",
            std::iter::once(call.as_str())
                .chain(ps[1..argc].iter().copied())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        |_, _| None,
    ));
    // Immediates: `CallBuiltinC` (last argument) and `CallBuiltinCC`.
    if argc >= 2 {
        let tail = if argc == 2 { "-0.75" } else { "0.25, -1.5" };
        v.push(site("", &format!("out = {name}(a, {tail})"), |p, n| {
            Some(if n == 2 {
                vec![p[0], Fx::from_f64_lit(-0.75).raw()]
            } else {
                vec![
                    p[0],
                    Fx::from_f64_lit(0.25).raw(),
                    Fx::from_f64_lit(-1.5).raw(),
                ]
            })
        }));
        // A constant FIRST operand is pushed, not folded into the call.
        v.push(site(
            "",
            &format!("out = {name}(0.5, {})", ps[1..argc].join(", ")),
            |p, n| {
                let mut k = vec![Fx::from_f64_lit(0.5).raw()];
                k.extend_from_slice(&p[1..n]);
                Some(k)
            },
        ));
    }
    v
}

/// `(name, argc)` for every inlined call.
const INLINED: [(&str, usize); 19] = [
    ("abs", 1),
    ("floor", 1),
    ("ceil", 1),
    ("round", 1),
    ("trunc", 1),
    ("frac", 1),
    ("fract", 1),
    ("min", 2),
    ("max", 2),
    ("clamp", 3),
    ("saturate", 1),
    ("mix", 3),
    ("lerp", 3),
    ("mod", 2),
    ("sign", 1),
    ("step", 2),
    ("square", 1),
    ("square", 2),
    ("triangle", 1),
];

fn inputs(argc: usize, exhaustive_edges: bool, randoms: usize, seed: u64) -> Vec<[i32; 3]> {
    let mut v = Vec::new();
    let mut rng = Rng(seed | 1);
    if exhaustive_edges {
        match argc {
            1 => v.extend(EDGES.iter().map(|&x| [x, 0, 0])),
            2 => {
                for &x in &EDGES {
                    for &y in &EDGES {
                        v.push([x, y, 0]);
                    }
                }
            }
            _ => {
                for &x in &EDGES {
                    for &y in &EDGES {
                        for &z in &EDGES {
                            v.push([x, y, z]);
                        }
                    }
                }
            }
        }
    } else {
        // a sample of edge tuples
        for _ in 0..200 {
            let mut t = [0; 3];
            for w in t.iter_mut() {
                *w = EDGES[(rng.next() as u32 as usize) % EDGES.len()];
            }
            v.push(t);
        }
    }
    for _ in 0..randoms {
        let t = [rng.next(), rng.next(), rng.next()];
        v.push(t);
        // and the same words with an edge mixed in
        let e = EDGES[(rng.next() as u32 as usize) % EDGES.len()];
        v.push([t[0], e, t[2]]);
    }
    v
}

#[test]
fn inline_builtins_match_their_kernel_bit_for_bit() {
    for (k, &(name, argc)) in INLINED.iter().enumerate() {
        let id = id_of(name);
        for (s, site) in shapes(name, argc).iter().enumerate() {
            // The plain shape gets every edge tuple; the rest a sample.
            let inp = inputs(
                argc,
                s == 0,
                RANDOMS,
                0x9e37_79b9 ^ (k as u64) << 8 ^ s as u64,
            );
            drive(site, id, argc, &inp, true);
        }
    }
}

/// The emitter's sequences are written against the kernels as they stand.
/// If a kernel's meaning changes, its sequence is wrong — so pin the few
/// facts the sequences assume, where a reader of `emit.rs` will look.
#[test]
fn the_kernel_facts_the_sequences_assume() {
    // `square(t)` defaults its duty to exactly 0.5 (the bit-15 test).
    assert_eq!(direct_default(id_of("square")), Some(1 << 15));
    // clamp is max-then-min: an inverted range yields `hi`.
    assert_eq!(kernel(id_of("clamp"), &[5, 10, 0]), 0);
    // floor/ceil/frac/mod at the i32 edges.
    assert_eq!(kernel(id_of("floor"), &[i32::MIN]), i32::MIN);
    assert_eq!(kernel(id_of("ceil"), &[i32::MAX]), i32::MIN);
    assert_eq!(kernel(id_of("frac"), &[i32::MIN]), 0);
    assert_eq!(kernel(id_of("mod"), &[i32::MIN, -1]), 0);
    assert_eq!(kernel(id_of("mod"), &[7, 0]), 0);
}

/// Arities the inline set does not cover keep the path they had: a short
/// `clamp` is still the generic wrapper, an overlong `abs` likewise, and
/// a builtin outside the set is still a direct call.
#[test]
fn other_arities_and_builtins_still_call() {
    let s = site("", "out = clamp(a, b)", |_, _| None);
    let mut b = Bridge::new(&s.prog, &s.img, fresh_vm(&s.prog, 60, 7));
    b.enter(s.f, &[1, 2, 3]);
    b.run(STEP_LIMIT).unwrap();
    assert_eq!(b.generic_calls, 1);

    let s = site("", "out = sqrt(a)", |_, _| None);
    let mut b = Bridge::new(&s.prog, &s.img, fresh_vm(&s.prog, 60, 7));
    b.enter(s.f, &[4 << 16, 0, 0]);
    b.run(STEP_LIMIT).unwrap();
    assert_eq!(b.direct_calls, 1);
}

/// `out` always holds a number here; anything else is a failure in itself.
fn num_of(v: Value) -> i32 {
    match v {
        Value::Num(x) => x.raw(),
        other => panic!("`out` holds {other:?}"),
    }
}
