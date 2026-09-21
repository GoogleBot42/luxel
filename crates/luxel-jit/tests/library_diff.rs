//! **The gate.** Every `library/*.js` is compiled to Xtensa, run through
//! an ISA model with a real `Vm` behind the context, and compared with the
//! interpreter bit for bit (docs/jit-design.md §7.1).
//!
//! This is what stands in for a device in phase 2. A mismatch is a bug in
//! the emitter or in the ISA model — never an acceptable difference: the
//! interpreter is the semantics, full stop.
//!
//! What is compared, per pattern:
//!
//! 1. **`beforeRender(delta)` once** — the globals it leaves behind.
//! 2. **`render*` at a spread of pixel indices** — `Vm::pixel` and
//!    `Vm::pixel_written`, the whole observable result of a render call.
//! 3. **Failure agreement** — a pattern that errors in the interpreter
//!    must error natively, and vice versa.
//!
//! Top-level init runs NATIVELY on the native side and interpreted on the
//! other, so the array allocations, the global initialisation and the
//! `ConstArr` promotions are all under test too.

mod common;

use common::bridge::{brush, fresh_vm, Bridge, Brush};
use common::isa::Trap;
use common::{env_with_fake_addresses, library, program_of};

use luxel_core::fixed::Fx;
use luxel_core::vm::{Program, Value, Vm};

/// Pixels per pattern. Sixty is the strip length the rest of the test
/// suite uses.
const PIXELS: u32 = 60;
/// Which pixel indices to compare. Both ends, some primes, and the one
/// past a natural half.
const PROBES: [u32; 8] = [0, 1, 2, 7, 29, 30, 31, 59];
/// Model instruction budget per entry. A render body is tens to low
/// thousands of instructions; this is loose enough not to matter and tight
/// enough that a runaway is a failure rather than a hang.
const STEP_LIMIT: u64 = 400_000;
/// Init and `beforeRender` do the bulk work — array fills, palette
/// builds — so they get a much larger budget than a per-pixel call.
const SETUP_LIMIT: u64 = 40_000_000;

/// How one pattern came out.
#[derive(Debug, Default)]
struct Verdict {
    /// Everything matched.
    ok: bool,
    /// Why not, if not.
    why: Option<String>,
}

fn diff_one(name: &str, src: &str) -> Verdict {
    let Ok((prog, kinds)) = program_of(src) else {
        return Verdict {
            ok: false,
            why: Some("does not compile".into()),
        };
    };
    let env = env_with_fake_addresses();
    let img = match luxel_jit::compile(&prog, &kinds, &env) {
        Ok(i) => i,
        Err(r) => {
            return Verdict {
                ok: false,
                why: Some(format!("refused: {}", r.reason())),
            }
        }
    };

    // --- the interpreter side
    let mut vi = fresh_vm(&prog, PIXELS, 7);
    let init_err_i = vi.call(&prog, 0, &[]).err();

    // --- the native side
    let vn = fresh_vm(&prog, PIXELS, 7);
    let mut b = Bridge::new(&prog, &img, vn);
    b.enter(0, &[]);
    match b.run(SETUP_LIMIT) {
        Ok(_) => {}
        Err(e) => {
            return Verdict {
                ok: false,
                why: Some(format!("init trapped: {e:?}")),
            }
        }
    }
    let init_err_n = b.failed();

    if init_err_i.is_some() != init_err_n {
        return Verdict {
            ok: false,
            why: Some(format!(
                "init disagrees: interpreter {:?}, native err={init_err_n}",
                init_err_i.as_ref().map(|e| &e.message)
            )),
        };
    }
    if let Some(d) = globals_differ(&vi, &b.vm) {
        return Verdict {
            ok: false,
            why: Some(format!("after init, {d}")),
        };
    }

    // --- beforeRender
    if let Some(f) = prog.exported_fn("beforeRender") {
        let delta = Fx::from_int(16).raw();
        let ei = vi.call(&prog, f, &[Value::Num(Fx::from_raw(delta))]).err();
        b.enter(f as usize, &[delta]);
        match b.run(SETUP_LIMIT) {
            Ok(_) => {}
            Err(e) => {
                return Verdict {
                    ok: false,
                    why: Some(format!("beforeRender trapped: {e:?}")),
                }
            }
        }
        if ei.is_some() != b.failed() {
            return Verdict {
                ok: false,
                why: Some(format!(
                    "beforeRender disagrees: interpreter {:?}, native err={}",
                    ei.as_ref().map(|e| &e.message),
                    b.failed()
                )),
            };
        }
        if let Some(d) = globals_differ(&vi, &b.vm) {
            return Verdict {
                ok: false,
                why: Some(format!("after beforeRender, {d}")),
            };
        }
    }

    // --- the render path
    let Some((f, argc)) = render_entry(&prog) else {
        // A renderFrame-only pattern: the frame builtins need the engine's
        // lent buffer, which this harness does not build. Init and
        // beforeRender still had to agree.
        return Verdict {
            ok: true,
            why: None,
        };
    };

    for i in PROBES {
        if i >= PIXELS {
            continue;
        }
        let args = render_args(i);
        let vals: Vec<Value> = args[..argc]
            .iter()
            .map(|r| Value::Num(Fx::from_raw(*r)))
            .collect();

        vi.pixel_written = false;
        let ei = vi.call(&prog, f, &vals).err();
        let bi = brush(&vi);

        b.vm.pixel_written = false;
        b.enter(f as usize, &args[..argc]);
        let r = b.run(STEP_LIMIT);
        if let Err(e) = r {
            return Verdict {
                ok: false,
                why: Some(format!("pixel {i} trapped: {e:?}")),
            };
        }
        let en = b.failed();
        let bn = brush(&b.vm);

        if ei.is_some() != en {
            return Verdict {
                ok: false,
                why: Some(format!(
                    "pixel {i}: interpreter err {:?}, native err={en}",
                    ei.as_ref().map(|e| &e.message)
                )),
            };
        }
        if ei.is_none() && bi != bn {
            return Verdict {
                ok: false,
                why: Some(format!("pixel {i}: {}", brush_diff(bi, bn))),
            };
        }
        if let Some(d) = globals_differ(&vi, &b.vm) {
            return Verdict {
                ok: false,
                why: Some(format!("after pixel {i}, {d}")),
            };
        }
    }
    let _ = name;
    Verdict {
        ok: true,
        why: None,
    }
}

/// The four render arguments as raw 16.16 words: `index`, then a
/// deterministic coordinate triple. The values only have to be the SAME on
/// both sides; the engine's own projection is not under test here.
fn render_args(i: u32) -> [i32; 4] {
    let x = Fx::from_int(i as i32) / Fx::from_int(PIXELS as i32);
    [
        Fx::from_int(i as i32).raw(),
        x.raw(),
        (x * Fx::from_raw(1 << 15)).raw(),
        Fx::from_raw(1 << 15).raw(),
    ]
}

/// The render entry and how many of the four arguments it takes.
fn render_entry(prog: &Program) -> Option<(u16, usize)> {
    for (name, argc) in [("render3D", 4), ("render2D", 3), ("render", 2)] {
        if let Some(f) = prog.exported_fn(name) {
            return Some((f, argc));
        }
    }
    None
}

fn globals_differ(a: &Vm, b: &Vm) -> Option<String> {
    for (i, (x, y)) in a.globals.iter().zip(b.globals.iter()).enumerate() {
        if x.raw() != y.raw() {
            return Some(format!(
                "global {i} is {:?} interpreted and {:?} native",
                x, y
            ));
        }
    }
    None
}

fn brush_diff(a: Brush, b: Brush) -> String {
    format!(
        "brush {:?} (written {}) interpreted vs {:?} (written {}) native",
        a.pixel, a.written, b.pixel, b.written
    )
}

/// The whole library, native against interpreted.
#[test]
fn library_renders_identically_native_and_interpreted() {
    let lib = library();
    assert!(lib.len() > 300, "expected the whole library, got {}", lib.len());
    let mut bad: Vec<(String, String)> = Vec::new();
    let mut pass = 0usize;
    for (k, (name, src)) in lib.iter().enumerate() {
        if k % 25 == 0 {
            eprintln!("... {k}/{}", lib.len());
        }
        let v = diff_one(name, src);
        if v.ok {
            pass += 1;
        } else {
            bad.push((name.clone(), v.why.unwrap_or_default()));
        }
    }
    eprintln!("native == interpreted for {pass}/{} patterns", lib.len());
    assert!(
        bad.is_empty(),
        "{} of {} patterns differ:\n{}",
        bad.len(),
        lib.len(),
        bad.iter()
            .map(|(n, w)| format!("  {n}: {w}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The model must actually be executing the generated code, not returning
/// straight away — a guard against a harness that passes by doing nothing.
#[test]
fn the_model_really_runs_the_code() {
    let (prog, kinds) = program_of(
        "export function render(i) { hsv(i * 3 + 0.25, 1, sqrt(i)) }\n",
    )
    .unwrap();
    let env = env_with_fake_addresses();
    let img = luxel_jit::compile(&prog, &kinds, &env).unwrap();
    let mut b = Bridge::new(&prog, &img, fresh_vm(&prog, PIXELS, 7));
    b.enter(0, &[]);
    b.run(STEP_LIMIT).unwrap();
    let f = prog.exported_fn("render").unwrap();
    b.enter(f as usize, &render_args(7)[..2]);
    b.run(STEP_LIMIT).unwrap();
    assert!(
        b.last_steps() > 20,
        "only {} instructions executed",
        b.last_steps()
    );
    assert!(b.native_calls >= 2, "hsv and sqrt should both be calls");
    assert!(b.vm.pixel_written, "render must have set the brush");
}

/// A runtime error must stop native execution the same way it stops the
/// interpreter, with the same message and the same attribution.
#[test]
fn a_runtime_error_bails_with_the_interpreters_message() {
    let (prog, kinds) = program_of(
        "var a = array(4)\n\
         export function render(i) { hsv(a[99], 1, 1) }\n",
    )
    .unwrap();
    let env = env_with_fake_addresses();
    let img = luxel_jit::compile(&prog, &kinds, &env).unwrap();

    let mut vi = fresh_vm(&prog, PIXELS, 7);
    vi.call(&prog, 0, &[]).unwrap();
    let f = prog.exported_fn("render").unwrap();
    let ei = vi
        .call(&prog, f, &[Value::Num(Fx::ZERO), Value::Num(Fx::ZERO)])
        .unwrap_err();

    let mut b = Bridge::new(&prog, &img, fresh_vm(&prog, PIXELS, 7));
    b.enter(0, &[]);
    b.run(STEP_LIMIT).unwrap();
    b.enter(f as usize, &[0, 0]);
    b.run(STEP_LIMIT).unwrap();
    assert!(b.failed(), "the native side must report the error");
    let en = b.err.take().expect("the helper must have filled ctx.err");
    assert_eq!(en.message, ei.message);
    assert_eq!(en.fn_idx, ei.fn_idx, "error attributed to the wrong fn");
    assert_eq!(en.pc, ei.pc, "error attributed to the wrong word");
    assert_eq!((en.line, en.col), (ei.line, ei.col));
}

/// Fuel is charged at back edges, so a runaway loop stops natively too
/// (§3.6) rather than spinning until the model's step limit.
#[test]
fn a_runaway_loop_runs_out_of_fuel() {
    let (prog, kinds) = program_of(
        "export function render(i) { var n = 0; while (1) { n = n + 1 } hsv(n, 1, 1) }\n",
    )
    .unwrap();
    let env = env_with_fake_addresses();
    let img = luxel_jit::compile(&prog, &kinds, &env).unwrap();
    let mut b = Bridge::new(&prog, &img, fresh_vm(&prog, PIXELS, 7));
    b.enter(0, &[]);
    b.run(STEP_LIMIT).unwrap();
    let f = prog.exported_fn("render").unwrap();
    b.enter(f as usize, &[0, 0]);
    b.set_fuel(500);
    match b.run(STEP_LIMIT) {
        Ok(_) => {
            assert!(b.failed(), "the loop must have bailed");
            let e = b.err.take().unwrap();
            assert!(
                e.message.contains("execution limit"),
                "unexpected error: {}",
                e.message
            );
        }
        Err(Trap::StepLimit) => panic!("fuel never ran out"),
        Err(e) => panic!("trapped: {e:?}"),
    }
}
