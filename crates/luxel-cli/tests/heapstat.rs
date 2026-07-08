//! Host-side heap model of the device's pattern lifecycle: for every gallery
//! pattern, measure live/peak heap through decode → engine init → frames —
//! the same sequence the firmware runs. Run with:
//!
//!   cargo test -p luxel-cli --test heapstat -- --nocapture
//!
//! Written for the soak-v5 OOM hunt (v0.1.24/25): the ESP32 has ~50 KB of
//! free heap at idle, so any pattern whose resident-or-peak footprint nears
//! that OOMs the device. This prints the offenders and the breakdown.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
        PEAK.fetch_max(live, Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let live = LIVE.fetch_add(new, Ordering::Relaxed) + new;
        PEAK.fetch_max(live, Ordering::Relaxed);
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        System.realloc(p, l, new)
    }
}

#[global_allocator]
static A: Counting = Counting;

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}
fn reset_peak() {
    PEAK.store(live(), Ordering::Relaxed);
}
fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

#[test]
fn gallery_heap_model() {
    use luxel_core::{bytecode, compile::compile, engine::Engine, fixed::Fx};

    let gallery = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/public/gallery.json"),
    )
    .expect("web/public/gallery.json (run web npm build once)");
    let gallery: serde_json::Value = serde_json::from_str(&gallery).unwrap();

    struct Row {
        name: String,
        blob: usize,
        prog: usize,
        engine: usize,
        frames_peak: usize,
        /// The device picture: lean decode (no debug info), 4096-element
        /// array budget, PLUS the resident source + blob (PATTERN_SRC/_BC).
        device_peak: usize,
    }
    let mut rows: Vec<Row> = Vec::new();

    for p in gallery.as_array().unwrap() {
        let name = p["name"].as_str().unwrap().to_string();
        let src = p["source"].as_str().unwrap().to_string();
        let Ok(prog) = compile(&src) else { continue };
        let blob = bytecode::serialize(&prog).unwrap();
        drop(prog);

        let base = live();
        let prog = bytecode::deserialize(&blob).unwrap();
        let prog_bytes = live() - base;
        reset_peak();
        let mut eng = Engine::from_program(prog, 300, 1);
        let engine_bytes = live() - base;
        for _ in 0..3 {
            eng.frame(Fx::from_f64(16.7));
        }
        let frames_peak = peak() - base;
        drop(eng);

        // device model: lean program, budgeted arrays, residents included
        let base = live();
        let prog = bytecode::deserialize_lean(&blob).unwrap();
        reset_peak();
        let mut eng = Engine::from_program_budgeted(prog, 300, 1, 32 * 1024);
        for _ in 0..3 {
            eng.frame(Fx::from_f64(16.7));
        }
        let device_peak = peak() - base + src.len() + blob.len();
        drop(eng);

        rows.push(Row {
            name,
            blob: blob.len(),
            prog: prog_bytes,
            engine: engine_bytes,
            frames_peak,
            device_peak,
        });
    }

    rows.sort_by_key(|r| std::cmp::Reverse(r.device_peak));
    println!(
        "\n{:<40} {:>7} {:>8} {:>8} {:>9} {:>9}",
        "pattern", "blob", "program", "engine", "run-peak", "device"
    );
    for r in rows.iter().take(25) {
        println!(
            "{:<40} {:>7} {:>8} {:>8} {:>9} {:>9}",
            &r.name[..r.name.len().min(40)],
            r.blob,
            r.prog,
            r.engine,
            r.frames_peak,
            r.device_peak
        );
    }
    let over: Vec<_> = rows.iter().filter(|r| r.device_peak > 45_000).collect();
    println!(
        "\n{} of {} patterns model over 45 KB on-device (free heap ≈ 50 KB at idle):",
        over.len(),
        rows.len()
    );
    for r in &over {
        println!("  {} ({} B)", r.name, r.device_peak);
    }
}
