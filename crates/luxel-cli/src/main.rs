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
        "serve" => serve::serve_cmd(&args[1..]),
        _ => usage(),
    }
}

/// Compile + smoke-run a pattern (.js source or .epe export) and report one
/// JSON line: {"file", "stage": "ok"|"epe"|"compile"|"init"|"frame", "error"?}.
/// The corpus report tooling drives this. Optional: --grid WxH (default
/// 10x10; sets pixel count to W·H and installs a 2D grid map).
fn check_cmd(path: &str, rest: &[String]) -> ExitCode {
    let (w, h) = match rest {
        [flag, v] if flag == "--grid" => match v.split_once('x') {
            Some((a, b)) => match (num(a), num(b)) {
                (Ok(a), Ok(b)) => (a.max(1), b.max(1)),
                _ => return usage(),
            },
            None => return usage(),
        },
        [] => (10, 10),
        _ => return usage(),
    };
    check_at(path, w, h)
}

fn check_at(path: &str, w: u32, h: u32) -> ExitCode {
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
    let pixels = w * h;
    let mut engine = match Engine::new(&src, pixels, 1) {
        Ok(e) => e,
        Err(d) => {
            let (line, col) = line_col(&src, d.span.start);
            return report("compile", Some(format!("{line}:{col}: {}", d.message)));
        }
    };
    if let Some(e) = engine.take_error() {
        return report("init", Some(e.message));
    }
    // a W×H grid map so render2D patterns exercise real coordinates
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
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        engine.set_wall_clock(now.as_secs() as i64);
    }
    for _ in 0..3 {
        engine.frame(Fx::from_f64(16.7));
        if let Some(e) = engine.take_error() {
            return report("frame", Some(e.message));
        }
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
    let mut engine = match Engine::new(&src, pixels, 1) {
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
    let mut engine = match Engine::new(&src, pixels, 1) {
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
                    .unwrap_or(&[])
                    .iter()
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
        "usage: luxel parse <pattern.js>\n       luxel run   <pattern.js> [--pixels N] [--frames N] [--fps F] [--out PATH] [--seed S] [--control NAME=V]\n       luxel bench <pattern.js> [--pixels N] [--frames N]\n       luxel serve [--pixels N] [--port P]"
    );
    ExitCode::from(2)
}

fn read(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read {path}: {e}");
        ExitCode::FAILURE
    })
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

    let mut engine = match Engine::new(&src, o.pixels, o.seed) {
        Ok(e) => e,
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
