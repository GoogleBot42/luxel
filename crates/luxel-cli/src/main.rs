//! `luxel` — desktop harness for the Luxel pattern language.
//!
//! Subcommands:
//!   luxel parse <file>                     dump the AST (or errors, editor-style)
//!   luxel run   <file> [opts]              render headlessly to a PPM frame-strip
//!   luxel bench <file> [opts]              measure VM throughput (pixels/sec)
//!
//! run/bench options:
//!   --pixels N     virtual strip length      (default 60 / 1000)
//!   --frames N     frames to render          (default 300 / 500)
//!   --fps F        simulated frame rate      (default 30)
//!   --out PATH     PPM output path           (default out.ppm; "-" = none)
//!   --seed S       RNG seed                  (default 1)
//!   --control NAME=V[,V,V]   invoke a UI control before rendering
//!   --no-fuse      compile without the superinstruction peephole (#261 A/B)
//!   --no-storefwd  compile without store forwarding (#320 A/B)
//!                  (also on `luxel compile`, so a device can be handed an
//!                  unfused blob against unchanged firmware)
//!
//! bench-only options:
//!   --profile      dump dynamic opcode/pair/triple/builtin counts for the run
//!   --json         emit that profile as one JSON line (tools/profile-library.mjs)
//!
//! compile-only options:
//!   --stats        one JSON line of the blob's STATIC shape (per-function
//!                  instruction counts); writes no .lxbc unless --out is given
//!                  (tools/oracle/opcount.mjs, Gitea #312)
//!
//! The PPM is one row per frame (like PB's preview strips): width = pixels,
//! height = frames.

use std::io::Write;
use std::process::ExitCode;
use std::time::Instant;

mod serve;

use luxel_core::diag::line_col;
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::parse::parse_program;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        return usage();
    };
    match cmd.as_str() {
        "parse" if args.len() == 2 => parse_cmd(&args[1]),
        "run" if args.len() >= 2 => run_cmd(&args[1], &args[2..], false),
        "bench" if args.len() >= 2 => run_cmd(&args[1], &args[2..], true),
        "vars" if args.len() >= 2 => vars_cmd(&args[1], &args[2..]),
        "pixels" if args.len() >= 2 => pixels_cmd(&args[1], &args[2..]),
        "check" if args.len() >= 2 => check_cmd(&args[1], &args[2..]),
        "compile" if args.len() >= 2 => compile_cmd(&args[1], &args[2..]),
        "serve" => serve::serve_cmd(&args[1..]),
        _ => usage(),
    }
}

/// Host wall clock for engine construction (UTC; no tz handling yet) so
/// top-level clock builtins see real time during init (Gitea #104).
fn now_unix() -> Option<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

/// Compile + smoke-run a pattern (.js source or .epe export) and report one
/// JSON line: {"file", "stage": "ok"|"epe"|"compile"|"init"|"frame", "error"?}.
/// The corpus report tooling drives this. Optional: --grid WxH (default
/// 10x10; sets pixel count to W·H and installs a 2D grid map) or --strip N
/// (N pixels and NO map — the mapless-strip rig most real devices run,
/// where a 2D pattern falls back to its own `render`; Gitea #193).
fn check_cmd(path: &str, rest: &[String]) -> ExitCode {
    let rig = match rest {
        [flag, v] if flag == "--grid" => match v.split_once('x') {
            Some((a, b)) => match (num(a), num(b)) {
                (Ok(a), Ok(b)) => Rig::Grid(a.max(1), b.max(1)),
                _ => return usage(),
            },
            None => return usage(),
        },
        [flag, v] if flag == "--strip" => match num(v) {
            Ok(n) => Rig::Strip(n.max(1)),
            Err(c) => return c,
        },
        [] => Rig::Grid(10, 10),
        _ => return usage(),
    };
    check_at(path, rig)
}

/// The rig a `check` run stands the pattern up on: a 2D grid map, or a
/// mapless strip of N pixels.
#[derive(Clone, Copy)]
enum Rig {
    Grid(u32, u32),
    Strip(u32),
}

fn check_at(path: &str, rig: Rig) -> ExitCode {
    let report = |stage: &str, error: Option<String>| {
        let mut obj = serde_json::json!({ "file": path, "stage": stage });
        if let Some(e) = error {
            obj["error"] = serde_json::Value::String(e);
        }
        println!("{obj}");
        if stage == "ok" {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    };
    let raw = match read(path) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let src = if path.ends_with(".epe") {
        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(v) => match v["sources"]["main"].as_str() {
                Some(s) => s.to_string(),
                None => return report("epe", Some("no sources.main in .epe".into())),
            },
            Err(e) => return report("epe", Some(format!("bad .epe JSON: {e}"))),
        }
    } else {
        raw.clone()
    };
    // grid sizes matter — patterns hardcoding rig shapes (width = 16) or
    // doing pixelCount/10 are genuinely OOB (on PB too) at other counts
    let pixels = match rig {
        Rig::Grid(w, h) => w * h,
        Rig::Strip(n) => n,
    };
    let prog = match luxel_core::compile::compile(&src) {
        Ok(p) => p,
        Err(d) => {
            let (line, col) = line_col(&src, d.span.start);
            return report("compile", Some(format!("{line}:{col}: {}", d.message)));
        }
    };
    // LXBC round-trip: encode must decode to a byte-identical re-encode, and
    // the decoded program must render exactly like the fresh compile — this
    // is the device's execution path, so the corpus report exercises it.
    let blob = match luxel_core::bytecode::serialize(&prog) {
        Ok(b) => b,
        Err(e) => return report("bytecode", Some(e.to_string())),
    };
    let prog_bc = match luxel_core::bytecode::deserialize(&blob) {
        Ok(p) => p,
        Err(e) => return report("bytecode", Some(e.to_string())),
    };
    match luxel_core::bytecode::serialize(&prog_bc) {
        Ok(b) if b == blob => {}
        Ok(_) => return report("bytecode", Some("re-encode not byte-identical".into())),
        Err(e) => return report("bytecode", Some(e.to_string())),
    }
    let mut engine = Engine::from_program_budgeted_at(prog, pixels, 1, usize::MAX, now_unix());
    let mut engine_bc = Engine::from_program_budgeted_at(prog_bc, pixels, 1, usize::MAX, now_unix());
    if let Some(e) = engine.take_error() {
        return report("init", Some(e.message));
    }
    engine_bc.take_error();
    // a W×H grid map so render2D patterns exercise real coordinates. A strip
    // rig installs NO map on purpose: that is the mapless device, where the
    // engine picks `render` and a 2D pattern's own 1D fallback runs — code
    // no grid rig ever executes (Gitea #193).
    if let Rig::Grid(w, _) = rig {
        let coords: Vec<[Fx; 3]> = (0..pixels)
            .map(|i| {
                [
                    Fx::from_int((i % w) as i32),
                    Fx::from_int((i / w) as i32),
                    Fx::ZERO,
                ]
            })
            .collect();
        engine.set_map(2, &coords);
        engine_bc.set_map(2, &coords);
    }
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        engine.set_wall_clock(now.as_secs() as i64);
        engine_bc.set_wall_clock(now.as_secs() as i64);
    }
    for _ in 0..3 {
        let px = engine.frame(Fx::from_f64(16.7)).to_vec();
        let px_bc = engine_bc.frame(Fx::from_f64(16.7));
        if px != px_bc {
            return report("bytecode", Some("frame differs from source path".into()));
        }
        if let Some(e) = engine.take_error() {
            return report("frame", Some(e.message));
        }
        engine_bc.take_error();
    }
    report("ok", None)
}

/// Render a pattern and dump the final frame's RGB bytes as a JSON array —
/// the local half of the PIXEL-level differential-oracle harness (the PB
/// side is a previewFrame capture; tools/oracle/pixels.mjs compares).
/// Frames run with delta 0, so only time-independent patterns make sense.
fn pixels_cmd(path: &str, rest: &[String]) -> ExitCode {
    let src = match read(path) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let pixels = match rest {
        [flag, n] if flag == "--pixels" => match num(n) {
            Ok(v) => v,
            Err(c) => return c,
        },
        [] => 60,
        _ => return usage(),
    };
    let mut engine = match Engine::new_at(&src, pixels, 1, now_unix()) {
        Ok(e) => e,
        Err(d) => {
            let (line, col) = line_col(&src, d.span.start);
            eprintln!("{path}:{line}:{col}: error: {}", d.message);
            return ExitCode::FAILURE;
        }
    };
    let mut last: Vec<u8> = Vec::new();
    for _ in 0..3 {
        last = engine.frame(Fx::ZERO).iter().flatten().copied().collect();
    }
    if let Some(e) = engine.take_error() {
        eprintln!(
            "warning: runtime error: line {}:{}: {}",
            e.line, e.col, e.message
        );
    }
    let items: Vec<String> = last.iter().map(|b| b.to_string()).collect();
    println!("[{}]", items.join(","));
    ExitCode::SUCCESS
}

/// Run a pattern's init (plus one frame) and dump exported vars as JSON with
/// raw 16.16 values — the local half of the differential-oracle harness.
fn vars_cmd(path: &str, rest: &[String]) -> ExitCode {
    let src = match read(path) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let pixels = match rest {
        [flag, n] if flag == "--pixels" => match num(n) {
            Ok(v) => v,
            Err(c) => return c,
        },
        [] => 60,
        _ => return usage(),
    };
    let mut engine = match Engine::new_at(&src, pixels, 1, now_unix()) {
        Ok(e) => e,
        Err(d) => {
            let (line, col) = line_col(&src, d.span.start);
            eprintln!("{path}:{line}:{col}: error: {}", d.message);
            return ExitCode::FAILURE;
        }
    };
    // three zero-delta frames so multi-frame oracle probes settle
    for _ in 0..3 {
        engine.frame(Fx::ZERO);
    }
    if let Some(e) = engine.take_error() {
        eprintln!(
            "warning: runtime error: line {}:{}: {}",
            e.line, e.col, e.message
        );
    }
    let names: Vec<String> = engine.exported_vars().map(String::from).collect();
    let mut out = String::from("{");
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{name}\":"));
        match engine.var(name) {
            Some(luxel_core::vm::Value::Num(v)) => out.push_str(&v.raw().to_string()),
            Some(luxel_core::vm::Value::Arr(_)) => {
                let vals: Vec<String> = engine
                    .var_array(name)
                    .into_iter()
                    .flat_map(|a| a.iter())
                    .map(|v| v.num().raw().to_string())
                    .collect();
                out.push_str(&format!("[{}]", vals.join(",")));
            }
            _ => out.push_str("null"),
        }
    }
    out.push('}');
    println!("{out}");
    ExitCode::SUCCESS
}

pub(crate) fn usage() -> ExitCode {
    eprintln!(
        "usage: luxel parse <pattern.js>\n       luxel run   <pattern.js> [--pixels N] [--frames N] [--fps F] [--out PATH] [--seed S] [--control NAME=V]\n       luxel bench <pattern.js> [--pixels N] [--frames N]\n       luxel check <pattern.js|.epe> [--grid WxH | --strip N]\n       luxel compile <pattern.js|.epe> [--out PATH.lxbc] [--no-fuse] [--no-storefwd] [--stats]\n       luxel serve [--pixels N] [--port P] [--heap-free BYTES] [--engine-heap BYTES]"
    );
    ExitCode::from(2)
}

fn read(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read {path}: {e}");
        ExitCode::FAILURE
    })
}

/// Compile a pattern to LXBC bytecode (what devices execute — they carry no
/// compiler). Default output: the input path with an .lxbc extension.
fn compile_cmd(path: &str, rest: &[String]) -> ExitCode {
    // `--no-fuse` here (not just on run/bench) is what makes the #261 A/B
    // runnable on a DEVICE: same firmware, two blobs, one fused and one not.
    let mut rest: Vec<String> = rest.to_vec();
    let no_fuse = rest.iter().any(|a| a == "--no-fuse");
    rest.retain(|a| a != "--no-fuse");
    let no_fold = rest.iter().any(|a| a == "--no-fold");
    rest.retain(|a| a != "--no-fold");
    let no_storefwd = rest.iter().any(|a| a == "--no-storefwd");
    rest.retain(|a| a != "--no-storefwd");
    // `--stats`: report the STATIC shape of the blob (per-function
    // instruction counts) as one JSON line — the Luxel half of the
    // Pixelblaze op-count comparison (tools/oracle/opcount.mjs, Gitea #312).
    // On its own it writes no file, so it can be pointed at library/ without
    // littering .lxbc next to the sources.
    let stats = rest.iter().any(|a| a == "--stats");
    rest.retain(|a| a != "--stats");
    let rest = &rest[..];
    let out_path = match rest {
        [flag, p] if flag == "--out" => Some(p.clone()),
        [] if stats => None,
        [] => {
            let stem = path.rsplit_once('.').map(|(s, _)| s).unwrap_or(path);
            Some(format!("{stem}.lxbc"))
        }
        _ => return usage(),
    };
    let raw = match read(path) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let src = if path.ends_with(".epe") {
        match serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|v| v["sources"]["main"].as_str().map(String::from))
        {
            Some(s) => s,
            None => {
                eprintln!("error: no sources.main in {path}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        raw
    };
    let prog = match luxel_core::compile::compile_with(
        &src,
        luxel_core::compile::CompileOpts {
            superinstructions: !no_fuse,
            const_folding: !no_fold,
            store_forwarding: !no_storefwd,
        },
    ) {
        Ok(p) => p,
        Err(d) => {
            let (line, col) = line_col(&src, d.span.start);
            eprintln!("error: {path}:{line}:{col}: {}", d.message);
            return ExitCode::FAILURE;
        }
    };
    let blob = match luxel_core::bytecode::serialize(&prog) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if stats {
        let mut fns = Vec::new();
        let mut total = 0u32;
        for (i, f) in prog.fns.iter().enumerate() {
            let s = f.code_start as usize;
            let code = &prog.words[s..s + f.code_len as usize];
            let insns = match luxel_core::bytecode::insn_count(code) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("error: {path}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            total += insns;
            fns.push(serde_json::json!({
                // fns[0] is top-level init code and has no source name.
                "name": if i == 0 { "(init)" } else { f.name.as_str() },
                "params": f.params,
                "locals": f.locals,
                "words": f.code_len,
                "insns": insns,
            }));
        }
        println!(
            "{}",
            serde_json::json!({
                "file": path,
                "fused": !no_fuse,
                "bytes": blob.len(),
                "words": prog.words.len(),
                "insns": total,
                "globals": prog.globals.len(),
                "fns": fns,
                "exported": prog.exported_fns.iter()
                    .map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            })
        );
    }

    let Some(out_path) = out_path else {
        return ExitCode::SUCCESS;
    };
    match std::fs::write(&out_path, &blob) {
        Ok(()) => {
            eprintln!("{out_path}: {} bytes", blob.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: cannot write {out_path}: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_cmd(path: &str) -> ExitCode {
    let src = match read(path) {
        Ok(s) => s,
        Err(c) => return c,
    };
    match parse_program(&src) {
        Ok(stmts) => {
            println!("{stmts:#?}");
            ExitCode::SUCCESS
        }
        Err(d) => {
            let (line, col) = line_col(&src, d.span.start);
            eprintln!("{path}:{line}:{col}: error: {}", d.message);
            ExitCode::FAILURE
        }
    }
}

struct Opts {
    pixels: u32,
    frames: u32,
    fps: u32,
    out: String,
    seed: u64,
    controls: Vec<(String, Vec<Fx>)>,
    /// 2D grid map dimensions (cols, rows); overrides --pixels.
    grid: Option<(u32, u32)>,
    /// `bench --profile`: dump dynamic opcode/bigram/builtin counts for
    /// the render pass (Gitea #261).
    profile: bool,
    /// `--json`: machine-readable profile on stdout (tools/profile-library.mjs).
    json: bool,
    /// `--no-fuse`: compile WITHOUT the superinstruction peephole — the
    /// A/B lever for Gitea #261 (and the way to prove a fused stream
    /// renders identically to the unfused one).
    no_fuse: bool,
    /// `--no-storefwd`: compile WITHOUT the Gitea #320 store-forwarding
    /// pass, so `x = …` followed by a read of `x` pops and reloads it.
    no_storefwd: bool,
    no_fold: bool,
}

fn parse_opts(args: &[String], bench: bool) -> Result<Opts, ExitCode> {
    let mut o = Opts {
        pixels: if bench { 1000 } else { 60 },
        frames: if bench { 500 } else { 300 },
        fps: 30,
        out: if bench { "-".into() } else { "out.ppm".into() },
        seed: 1,
        controls: Vec::new(),
        grid: None,
        profile: false,
        json: false,
        no_fuse: false,
        no_storefwd: false,
        no_fold: false,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || {
            it.next().cloned().ok_or_else(|| {
                eprintln!("error: {a} needs a value");
                ExitCode::from(2)
            })
        };
        match a.as_str() {
            "--pixels" => o.pixels = num(&val()?)?,
            "--frames" => o.frames = num(&val()?)?,
            "--fps" => o.fps = num(&val()?)?.max(1),
            "--out" => o.out = val()?,
            "--seed" => o.seed = num(&val()?)? as u64,
            "--map-grid" => {
                let v = val()?;
                let Some((w, h)) = v.split_once('x') else {
                    eprintln!("error: --map-grid expects WxH (e.g. 16x16)");
                    return Err(ExitCode::from(2));
                };
                o.grid = Some((num(w)?.max(1), num(h)?.max(1)));
            }
            "--profile" if bench => o.profile = true,
            "--json" if bench => o.json = true,
            "--no-fuse" => o.no_fuse = true,
            "--no-fold" => o.no_fold = true,
            "--no-storefwd" => o.no_storefwd = true,
            "--control" => {
                let v = val()?;
                let Some((name, vals)) = v.split_once('=') else {
                    eprintln!("error: --control expects NAME=V[,V,V]");
                    return Err(ExitCode::from(2));
                };
                let vals: Result<Vec<Fx>, _> = vals
                    .split(',')
                    .map(|s| s.trim().parse::<f64>().map(Fx::from_f64))
                    .collect();
                match vals {
                    Ok(vs) => o.controls.push((name.to_string(), vs)),
                    Err(_) => {
                        eprintln!("error: bad control value in `{v}`");
                        return Err(ExitCode::from(2));
                    }
                }
            }
            _ => {
                eprintln!("error: unknown option {a}");
                return Err(ExitCode::from(2));
            }
        }
    }
    Ok(o)
}

fn num(s: &str) -> Result<u32, ExitCode> {
    s.parse().map_err(|_| {
        eprintln!("error: bad number `{s}`");
        ExitCode::from(2)
    })
}

fn run_cmd(path: &str, rest: &[String], bench: bool) -> ExitCode {
    let src = match read(path) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let mut o = match parse_opts(rest, bench) {
        Ok(o) => o,
        Err(c) => return c,
    };
    if let Some((w, h)) = o.grid {
        o.pixels = w * h;
    }
    let o = o;

    let compiled = luxel_core::compile::compile_with(
        &src,
        luxel_core::compile::CompileOpts {
            superinstructions: !o.no_fuse,
            const_folding: !o.no_fold,
            store_forwarding: !o.no_storefwd,
        },
    );
    let mut engine = match compiled {
        Ok(p) => Engine::from_program_budgeted_at(p, o.pixels, o.seed, usize::MAX, now_unix()),
        Err(d) => {
            let (line, col) = line_col(&src, d.span.start);
            eprintln!("{path}:{line}:{col}: error: {}", d.message);
            return ExitCode::FAILURE;
        }
    };
    if let Some(e) = engine.take_error() {
        eprintln!(
            "warning: runtime error during init: line {}:{}: {}",
            e.line, e.col, e.message
        );
    }
    if let Some((w, h)) = o.grid {
        let coords: Vec<[Fx; 3]> = (0..o.pixels)
            .map(|i| {
                [
                    Fx::from_int((i % w) as i32),
                    Fx::from_int((i / w) as i32),
                    Fx::ZERO,
                ]
            })
            .collect();
        engine.set_map(2, &coords);
        let _ = h;
    }
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        engine.set_wall_clock(now.as_secs() as i64); // UTC; no tz handling yet
    }
    for (name, vals) in &o.controls {
        if engine.set_control(name, vals).is_none() && engine.last_error.is_none() {
            eprintln!("warning: no control named `{name}`");
        }
    }

    let delta = Fx::from_f64(1000.0 / o.fps as f64);
    let mut strip: Vec<u8> = Vec::with_capacity((o.pixels * o.frames * 3) as usize);
    let mut first_err = None;

    // Counters describe the RENDER pass: init (and its top-level loops,
    // which can dwarf a frame in table-building patterns) is excluded.
    #[cfg(feature = "profile")]
    if o.profile {
        engine.profile_reset();
    }
    let t0 = Instant::now();
    for _ in 0..o.frames {
        let frame = engine.frame(delta);
        if !bench && o.out != "-" {
            for px in frame {
                strip.extend_from_slice(px);
            }
        }
        if first_err.is_none() {
            first_err = engine.take_error();
        }
    }
    let elapsed = t0.elapsed().as_secs_f64();

    let total_px = o.pixels as f64 * o.frames as f64;
    eprintln!(
        "{}: {} px × {} frames in {:.3}s — {:.0} px/s, {:.1} fps equivalent",
        path,
        o.pixels,
        o.frames,
        elapsed,
        total_px / elapsed,
        o.frames as f64 / elapsed,
    );
    if let Some(e) = first_err {
        eprintln!(
            "warning: runtime error: line {}:{}: {}",
            e.line, e.col, e.message
        );
    }

    if o.profile {
        #[cfg(feature = "profile")]
        report_profile(path, &engine, o.pixels as u64 * o.frames as u64, o.json);
        #[cfg(not(feature = "profile"))]
        {
            eprintln!("error: this luxel was built without the `profile` feature");
            return ExitCode::FAILURE;
        }
    }

    if !bench && o.out != "-" {
        let header = format!("P6\n{} {}\n255\n", o.pixels, o.frames);
        let write_result = std::fs::File::create(&o.out).and_then(|mut f| {
            f.write_all(header.as_bytes())
                .and_then(|_| f.write_all(&strip))
        });
        match write_result {
            Ok(()) => eprintln!("wrote {} ({} frames as rows)", o.out, o.frames),
            Err(e) => {
                eprintln!("error: cannot write {}: {e}", o.out);
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

/// `luxel bench <pattern> --profile` — what the interpreter actually
/// executed during the render pass: per-opcode counts, the statically
/// adjacent opcode pairs and triples by dynamic weight (the shapes a
/// compiler-side peephole can fuse into a superinstruction), and per-
/// builtin call counts. `--json` emits the same thing for
/// tools/profile-library.mjs to aggregate over the library.
///
/// Counting costs time, so the px/s line printed above it is NOT a
/// throughput measurement — run `bench` without `--profile` for that.
#[cfg(feature = "profile")]
fn report_profile(path: &str, engine: &Engine, pixels: u64, json: bool) {
    use luxel_core::bytecode::op_name;
    use luxel_core::vm::BUILTINS;
    let p = engine.profile();
    let per_px = p.insns as f64 / pixels.max(1) as f64;

    let mut ops: Vec<(u8, u64)> = (0u16..256)
        .filter(|&i| p.ops[i as usize] > 0)
        .map(|i| (i as u8, p.ops[i as usize]))
        .collect();
    ops.sort_by(|a, b| b.1.cmp(&a.1));
    let mut bi: Vec<((u8, u8), u64)> = p.bigrams.iter().map(|(k, v)| (*k, *v)).collect();
    bi.sort_by(|a, b| b.1.cmp(&a.1));
    let mut tri: Vec<((u8, u8, u8), u64)> = p.trigrams.iter().map(|(k, v)| (*k, *v)).collect();
    tri.sort_by(|a, b| b.1.cmp(&a.1));
    let mut bl: Vec<(u16, u64)> = p.builtins.iter().map(|(k, v)| (*k, *v)).collect();
    bl.sort_by(|a, b| b.1.cmp(&a.1));
    let bname = |b: u16| {
        BUILTINS
            .get(b as usize)
            .map(|d| d.name)
            .unwrap_or("?")
            .to_string()
    };

    if json {
        let obj = serde_json::json!({
            "file": path,
            "pixels": pixels,
            "insns": p.insns,
            "insns_per_px": per_px,
            "ops": ops.iter().map(|(o, n)| serde_json::json!([op_name(*o), n])).collect::<Vec<_>>(),
            "bigrams": bi.iter().map(|((a, b), n)|
                serde_json::json!([format!("{} {}", op_name(*a), op_name(*b)), n])).collect::<Vec<_>>(),
            "trigrams": tri.iter().take(200).map(|((a, b, c), n)|
                serde_json::json!([format!("{} {} {}", op_name(*a), op_name(*b), op_name(*c)), n])).collect::<Vec<_>>(),
            "builtins": bl.iter().map(|(b, n)| serde_json::json!([bname(*b), n])).collect::<Vec<_>>(),
        });
        println!("{obj}");
        return;
    }

    let pct = |n: u64| 100.0 * n as f64 / p.insns.max(1) as f64;
    println!("profile: {path}");
    println!(
        "  {} instructions over {pixels} pixel renders — {per_px:.1} insns/px",
        p.insns
    );
    println!("\n  opcode                     count      %");
    for (o, n) in ops.iter().take(30) {
        println!("  {:<20} {:>10}  {:>5.1}", op_name(*o), n, pct(*n));
    }
    println!("\n  adjacent pair                                  count      %");
    for ((a, b), n) in bi.iter().take(25) {
        println!(
            "  {:<40} {:>10}  {:>5.1}",
            format!("{} {}", op_name(*a), op_name(*b)),
            n,
            pct(*n)
        );
    }
    println!("\n  adjacent triple                                            count      %");
    for ((a, b, c), n) in tri.iter().take(25) {
        println!(
            "  {:<52} {:>10}  {:>5.1}",
            format!("{} {} {}", op_name(*a), op_name(*b), op_name(*c)),
            n,
            pct(*n)
        );
    }
    println!("\n  builtin                    calls   per px");
    for (b, n) in bl.iter().take(25) {
        println!(
            "  {:<20} {:>10}  {:>7.3}",
            bname(*b),
            n,
            *n as f64 / pixels.max(1) as f64
        );
    }
}
