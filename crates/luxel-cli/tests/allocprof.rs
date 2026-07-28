//! Per-allocation heap profile of the device pattern lifecycle, on the host.
//!
//! Where tests/heapstat.rs models aggregate live/peak bytes, this tracks
//! EVERY allocation through dhat and attributes it to its callsite, split
//! three ways: resident at end (what the running pattern holds), live at
//! global peak (what the worst transient moment holds), and total churn
//! (allocation traffic — per-frame cost shows up here).
//!
//! One scenario per process run (dhat allows a single Profiler); drive it
//! with env vars:
//!
//!   AP_SRC      "rainbow" (builtin literal) | substring of a gallery name
//!   AP_PIXELS   strip length            (default 300)
//!   AP_FRAMES   frames to render        (default 3)
//!   AP_BUDGET   array budget in bytes   (default 66_560 = device @ ~90 KB free)
//!   AP_JSONVIEW "1": call vars_json+controls_json each frame (GET /api/vars)
//!
//!   cargo test -p luxel-cli --test allocprof -- --nocapture
//!
//! The device-resident model matches firmware/src/main.rs: PATTERN_SRC and
//! PATTERN_BC stay alive alongside the engine (deserialize_lean, budgeted).

use std::collections::HashMap;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const RAINBOW: &str = r#"
export function render(index) {
  hsv(time(.1) + index / pixelCount, 1, 1)
}
"#;

fn env_usize(k: &str, default: usize) -> usize {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
fn alloc_profile() {
    use luxel_core::{bytecode, compile::compile, engine::Engine, fixed::Fx, jsonview};

    let sel = std::env::var("AP_SRC").unwrap_or_else(|_| "rainbow".into());
    let pixels = env_usize("AP_PIXELS", 300) as u32;
    let frames = env_usize("AP_FRAMES", 3);
    let budget = env_usize("AP_BUDGET", 66_560);
    let jsonview_on = std::env::var("AP_JSONVIEW").is_ok_and(|v| v == "1");

    // Resolve the source OUTSIDE the profiled window (gallery parse noise).
    let (name, src) = if sel == "rainbow" {
        ("rainbow (builtin)".to_string(), RAINBOW.to_string())
    } else {
        let gallery = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/public/gallery.json"),
        )
        .expect("web/public/gallery.json (run web npm build once)");
        let gallery: serde_json::Value = serde_json::from_str(&gallery).unwrap();
        let p = gallery
            .as_array()
            .unwrap()
            .iter()
            .find(|p| {
                p["name"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
                    .contains(&sel.to_lowercase())
            })
            .unwrap_or_else(|| panic!("no gallery pattern matching {sel:?}"));
        (
            p["name"].as_str().unwrap().to_string(),
            p["source"].as_str().unwrap().to_string(),
        )
    };
    // Compile on host = the editor/CLI path; devices receive the blob. Do it
    // outside the window too, keeping only the blob.
    let prog = compile(&src).expect("pattern compiles");
    let blob = bytecode::serialize(&prog).unwrap();
    drop(prog);

    let out = std::env::temp_dir().join(format!("dhat-{}.json", std::process::id()));
    println!(
        "\n=== allocprof: {name} | {pixels} px | {frames} frames | budget {budget} B | jsonview {jsonview_on} ==="
    );

    let profiler = dhat::Profiler::builder().file_name(&out).build();

    // --- profiled window: exactly the firmware's swap-path + steady state ---
    // Residents the firmware keeps for /api/pattern and playlist persistence.
    // inline(never) wrappers so dhat attributes them by name:
    #[inline(never)]
    fn resident_pattern_src(s: &str) -> String {
        s.to_string()
    }
    #[inline(never)]
    fn resident_pattern_bc(b: &[u8]) -> Vec<u8> {
        b.to_vec()
    }
    let pattern_src: String = resident_pattern_src(&src); // PATTERN_SRC
    let pattern_bc: Vec<u8> = resident_pattern_bc(&blob); // PATTERN_BC
    let prog = bytecode::deserialize_lean(&pattern_bc).unwrap();
    let mut eng = Engine::from_program_budgeted(prog, pixels, 1, budget);
    let mut sink = 0u64; // defeat DCE on snapshot strings
    for _ in 0..frames {
        let frame = eng.frame(Fx::from_f64(16.7));
        sink = sink.wrapping_add(frame.first().map(|p| p[0] as u64).unwrap_or(0));
        if jsonview_on {
            sink = sink.wrapping_add(jsonview::vars_json(&eng).len() as u64);
            sink = sink.wrapping_add(jsonview::controls_json(&eng).len() as u64);
        }
    }
    // Drop the profiler while engine + residents are LIVE: dhat's "end"
    // numbers then equal the device's steady-state resident set.
    drop(profiler);
    // -------------------------------------------------------------------------

    std::mem::forget(eng);
    std::mem::forget(pattern_src);
    std::mem::forget(pattern_bc);
    let _ = sink;

    report(&out, budget);
}

/// Parse dhat-heap.json (dhatFileVersion 2) and print three per-callsite
/// tables: resident-at-end, live-at-global-peak, total churn.
fn report(path: &std::path::Path, _budget: usize) {
    let j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let ftbl: Vec<&str> = j["ftbl"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();

    struct Pp {
        site: String,
        tb: u64,
        tbk: u64,
        gb: u64,
        eb: u64,
    }
    let mut pps: Vec<Pp> = Vec::new();
    for pp in j["pps"].as_array().unwrap() {
        let fs: Vec<usize> = pp["fs"].as_array().unwrap().iter().map(|i| i.as_u64().unwrap() as usize).collect();
        pps.push(Pp {
            site: collapse(&fs, &ftbl),
            tb: pp["tb"].as_u64().unwrap_or(0),
            tbk: pp["tbk"].as_u64().unwrap_or(0),
            gb: pp["gb"].as_u64().unwrap_or(0),
            eb: pp["eb"].as_u64().unwrap_or(0),
        });
    }

    // dhat emits one pp per unique stack; merge by collapsed site for readability.
    let mut by_site: HashMap<String, (u64, u64, u64, u64)> = HashMap::new();
    for p in &pps {
        let e = by_site.entry(p.site.clone()).or_default();
        e.0 += p.tb;
        e.1 += p.tbk;
        e.2 += p.gb;
        e.3 += p.eb;
    }
    let mut rows: Vec<(&String, &(u64, u64, u64, u64))> = by_site.iter().collect();

    let totals = rows.iter().fold((0u64, 0u64, 0u64, 0u64), |a, (_, v)| {
        (a.0 + v.0, a.1 + v.1, a.2 + v.2, a.3 + v.3)
    });
    println!(
        "TOTALS: churn {} B in {} blocks | at-peak {} B | resident-at-end {} B",
        totals.0, totals.1, totals.2, totals.3
    );

    for (title, key) in [
        ("RESIDENT AT END (steady-state footprint)", 3usize),
        ("LIVE AT GLOBAL PEAK (worst transient)", 2),
        ("TOTAL CHURN (allocation traffic)", 0),
    ] {
        rows.sort_by_key(|(_, v)| {
            std::cmp::Reverse(match key {
                0 => v.0,
                2 => v.2,
                _ => v.3,
            })
        });
        println!("\n-- {title} --");
        println!("{:>9}  {:>7}  {:>9}  {:>9}  callsite", "churn B", "blocks", "peak B", "end B");
        let mut shown = (0u64, 0u64, 0u64, 0u64);
        for (site, v) in rows.iter() {
            let val = match key {
                0 => v.0,
                2 => v.2,
                _ => v.3,
            };
            if val == 0 {
                continue;
            }
            println!("{:>9}  {:>7}  {:>9}  {:>9}  {}", v.0, v.1, v.2, v.3, site);
            shown = (shown.0 + v.0, shown.1 + v.1, shown.2 + v.2, shown.3 + v.3);
        }
        println!(
            "   (rows cover {}/{} churn B, {}/{} peak B, {}/{} end B)",
            shown.0, totals.0, shown.2, totals.2, shown.3, totals.3
        );
    }
}

/// Reduce a dhat stack (frame indices, leaf-first) to a readable callsite:
/// the innermost luxel_core frame plus its caller, else the innermost
/// non-allocator frame.
fn collapse(fs: &[usize], ftbl: &[&str]) -> String {
    let clean = |f: &str| {
        // "0x555...: sym (file:line:col)" -> "sym (file:line)"
        let f = f.split_once(": ").map(|(_, s)| s).unwrap_or(f);
        let f = f.replace("luxel_core::", "");
        // trim generic noise and column numbers
        let f = f.split("::h").next().unwrap_or(&f).to_string();
        f
    };
    let frames: Vec<&str> = fs.iter().map(|&i| ftbl[i]).collect();
    let is_noise = |f: &str| {
        f.contains("dhat::")
            || f.contains("__rust_alloc")
            || f.contains("alloc::alloc")
            || f.contains("alloc::raw_vec")
            || f.contains("[root]")
    };
    if let Some(f) = frames.iter().find(|f| f.contains("resident_pattern_")) {
        return clean(f);
    }
    let luxel: Vec<&&str> = frames
        .iter()
        .filter(|f| f.contains("luxel") && !f.contains("allocprof"))
        .collect();
    if let Some(inner) = luxel.first() {
        let mut s = clean(inner);
        if let Some(outer) = luxel.get(1) {
            let o = clean(outer);
            // keep two luxel frames when they differ meaningfully
            if o.split(' ').next() != s.split(' ').next() {
                s = format!("{s}  <-  {o}");
            }
        }
        return s;
    }
    frames
        .iter()
        .find(|f| !is_noise(f))
        .map(|f| clean(f))
        .unwrap_or_else(|| "<unattributed>".into())
}
