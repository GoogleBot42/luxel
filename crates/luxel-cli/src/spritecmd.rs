//! `luxel sprite import` — the subcommand's argument parsing and reporting
//! (Gitea #784). The pipeline itself is `spriteimport.rs`, which is a separate
//! module for one reason: `tests/spriteimport.rs` includes it directly (the
//! crate is a `[[bin]]`, so there is no library to link against), and a module
//! that reached for `crate::usage()` could not be included that way.

use luxel_core::sprite::{
    SPRITE_HDR, SPRITE_MAX_BYTES, SPRITE_MAX_COLORS, SPRITE_MAX_FPS, SPRITE_MAX_FRAMES,
};

use crate::spriteimport::{
    check_record, decode_file, default_target_size, encode, fit_under_cap, import_sprite, FitMode,
    Options, Resample,
};

/// `luxel sprite import <image> [opts] -o out.lxsp` (Gitea #784). The shipped
/// sprite/scene library generator (#785) is what will call this, which is also
/// why the record it writes is exactly the one the browser writes.
pub(crate) fn sprite_cmd(args: &[String]) -> std::process::ExitCode {
    match args.first().map(String::as_str) {
        Some("import") if args.len() >= 2 => import_cmd(&args[1], &args[2..]),
        _ => crate::usage(),
    }
}

fn import_cmd(path: &str, rest: &[String]) -> std::process::ExitCode {
    let mut out: Option<String> = None;
    let mut size: Option<(usize, usize)> = None;
    let mut o = Options::default();
    let mut name: Option<String> = None;
    let mut fit_cap = false;
    let mut max_bytes = SPRITE_MAX_BYTES;

    let mut i = 0usize;
    while i < rest.len() {
        let a = rest[i].as_str();
        let need = |i: usize| -> Result<&String, std::process::ExitCode> {
            rest.get(i + 1).ok_or_else(|| {
                eprintln!("error: {} needs a value", rest[i]);
                std::process::ExitCode::from(2)
            })
        };
        match a {
            "-o" | "--out" => {
                let v = match need(i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                out = Some(v.clone());
                i += 2;
            }
            "--size" => {
                let v = match need(i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                match v.split_once(['x', 'X']) {
                    Some((a, b)) => match (a.parse::<usize>(), b.parse::<usize>()) {
                        (Ok(w), Ok(h)) if w >= 1 && h >= 1 => size = Some((w, h)),
                        _ => {
                            eprintln!("error: --size expects WxH (e.g. 32x32)");
                            return std::process::ExitCode::from(2);
                        }
                    },
                    None => {
                        eprintln!("error: --size expects WxH (e.g. 32x32)");
                        return std::process::ExitCode::from(2);
                    }
                }
                i += 2;
            }
            "--fit" => {
                let v = match need(i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                o.fit = match v.as_str() {
                    "fit" | "contain" => FitMode::Fit,
                    "fill" | "stretch" => FitMode::Fill,
                    "crop" | "cover" => FitMode::Crop,
                    _ => {
                        eprintln!("error: --fit expects fit|fill|crop");
                        return std::process::ExitCode::from(2);
                    }
                };
                i += 2;
            }
            "--resample" => {
                let v = match need(i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                o.resample = match v.as_str() {
                    "nearest" | "near" => Resample::Nearest,
                    "area" | "average" => Resample::Area,
                    _ => {
                        eprintln!("error: --resample expects nearest|area");
                        return std::process::ExitCode::from(2);
                    }
                };
                i += 2;
            }
            "--fps" => {
                let v = match need(i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                match v.parse::<u8>() {
                    Ok(f) if f <= SPRITE_MAX_FPS => o.fps_override = Some(f),
                    _ => {
                        eprintln!("error: --fps expects 0..{SPRITE_MAX_FPS}");
                        return std::process::ExitCode::from(2);
                    }
                }
                i += 2;
            }
            "--colors" | "--colours" => {
                let v = match need(i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                match v.parse::<usize>() {
                    Ok(n) if (1..=SPRITE_MAX_COLORS).contains(&n) => o.colors = n,
                    _ => {
                        eprintln!("error: --colors expects 1..{SPRITE_MAX_COLORS}");
                        return std::process::ExitCode::from(2);
                    }
                }
                i += 2;
            }
            "--alpha" => {
                let v = match need(i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                match v.parse::<u16>() {
                    Ok(n) if (1..=255).contains(&n) => o.alpha_threshold = n,
                    _ => {
                        eprintln!("error: --alpha expects 1..255");
                        return std::process::ExitCode::from(2);
                    }
                }
                i += 2;
            }
            "--keep-every" => {
                let v = match need(i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                match v.parse::<usize>() {
                    Ok(n) if n >= 1 => o.keep_every = n,
                    _ => {
                        eprintln!("error: --keep-every expects 1 or more");
                        return std::process::ExitCode::from(2);
                    }
                }
                i += 2;
            }
            "--name" => {
                let v = match need(i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                name = Some(v.clone());
                i += 2;
            }
            "--max-bytes" => {
                let v = match need(i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                match v.parse::<usize>() {
                    Ok(n) if n > SPRITE_HDR => max_bytes = n.min(SPRITE_MAX_BYTES),
                    _ => {
                        eprintln!("error: --max-bytes expects a byte count");
                        return std::process::ExitCode::from(2);
                    }
                }
                i += 2;
            }
            "--dither" => {
                o.dither = true;
                i += 1;
            }
            "--fit-cap" => {
                fit_cap = true;
                i += 1;
            }
            _ => {
                eprintln!("error: unknown option {a}");
                return std::process::ExitCode::from(2);
            }
        }
    }

    let src = match decode_file(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    // The name defaults to the FILE's, exactly as the browser's does.
    o.name = name.unwrap_or_else(|| path.to_string());
    // …and the size to the long edge at 64, unless one was asked for.
    let (w, h) = size.unwrap_or_else(|| default_target_size(src.w, src.h, None));
    o.w = w;
    o.h = h;

    if fit_cap {
        let fix = fit_under_cap(&src, &o, max_bytes);
        if !fix.said.is_empty() {
            eprintln!("note: fitting under {max_bytes} B by {}", fix.said);
        }
        o = fix.options;
    }

    let r = import_sprite(&src, &o);
    let bytes = encode(&r.sprite);
    if r.dropped_frames > 0 {
        eprintln!(
            "warning: {} frame(s) past the {SPRITE_MAX_FRAMES}-frame limit are not in this record \
             — raise --keep-every to sample the whole animation",
            r.dropped_frames
        );
    }
    // The CAP FIRST, and before `check_record`: that would refuse the same
    // record with `sprite: over the 16 KiB cap`, which is the right sentence
    // for a device route and a useless one here — the console shows the knobs
    // and so must this.
    if r.over_cap || bytes.len() > max_bytes {
        eprintln!(
            "error: the record is {} B and the limit is {max_bytes} B. Use fewer colours \
             (--colors), a smaller --size, more --keep-every, or --fit-cap to do it for you.",
            bytes.len()
        );
        return std::process::ExitCode::FAILURE;
    }
    if let Err(why) = check_record(&bytes) {
        eprintln!("error: {why}");
        return std::process::ExitCode::FAILURE;
    }

    if r.skipped_frames > 0 {
        eprintln!(
            "note: --keep-every {} sampled {} of {} source frames",
            o.keep_every,
            r.sprite.frames,
            r.skipped_frames + r.sprite.frames
        );
    }
    let summary = format!(
        "{}x{} x{} @ {} fps, {} colours of {} seen, {} B",
        r.sprite.w, r.sprite.h, r.sprite.frames, r.fps, r.colors_used, r.colors_seen, r.bytes
    );
    match out {
        Some(dest) => match std::fs::write(&dest, &bytes) {
            Ok(()) => {
                eprintln!("{dest}: {summary}");
                std::process::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: cannot write {dest}: {e}");
                std::process::ExitCode::FAILURE
            }
        },
        // No -o: report what it WOULD write, so a size can be checked without
        // making a file (the same courtesy `luxel compile --stats` has).
        None => {
            println!("{summary}");
            std::process::ExitCode::SUCCESS
        }
    }
}
