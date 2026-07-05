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
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: luxel parse <pattern.js>\n       luxel run   <pattern.js> [--pixels N] [--frames N] [--fps F] [--out PATH] [--seed S] [--control NAME=V]\n       luxel bench <pattern.js> [--pixels N] [--frames N]"
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
}

fn parse_opts(args: &[String], bench: bool) -> Result<Opts, ExitCode> {
    let mut o = Opts {
        pixels: if bench { 1000 } else { 60 },
        frames: if bench { 500 } else { 300 },
        fps: 30,
        out: if bench { "-".into() } else { "out.ppm".into() },
        seed: 1,
        controls: Vec::new(),
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
    let o = match parse_opts(rest, bench) {
        Ok(o) => o,
        Err(c) => return c,
    };

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
            "warning: runtime error during init: {} (fn {}, pc {})",
            e.message, e.fn_idx, e.pc
        );
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
            "warning: runtime error: {} (fn {}, pc {})",
            e.message, e.fn_idx, e.pc
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
