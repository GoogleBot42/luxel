//! Host-side heap model of the device's pattern lifecycle: for every gallery
//! pattern, measure live/peak heap through decode → engine init → frames —
//! the same sequence the firmware runs. Run with:
//!
//!   cargo test -p luxel-cli --test heapstat -- --nocapture
//!
//! Written for the soak-v5 OOM hunt (v0.1.24/25): the ESP32 has ~50 KB of
//! free heap at idle, so any pattern whose peak footprint nears that OOMs the
//! device. This prints the offenders and the breakdown.
//!
//! The three `swap(...)` columns are the pattern-swap peak measured the same
//! way (counting allocator, lean decode, budgeted engine, 3 frames), differing
//! only in where the bytecode lives:
//!
//! * `swap(vec)` — the HISTORICAL cost, before the flash mapping: the store's
//!   source and blob read onto the heap as String/Vec and re-encoded into an
//!   envelope before decoding. No firmware path does this any more; it is the
//!   baseline the other two are measured against.
//! * `swap(nomap)` — the `flashmap-off` fallback the firmware still keeps
//!   (Gitea #330): the bytecode extent is read into ONE transient Vec and
//!   `deserialize_lean` copies its words. No source Vec, no envelope — the
//!   store's source never leaves flash on an activation at all.
//! * `swap(xip)` — the normal path: executing straight out of the mapping,
//!   where the blob bytes are never heap. The lean `Program` is built by
//!   `deserialize_lean_static` over a 4-aligned 'static copy of the blob
//!   (standing in for the mapping), so its code and constant words are
//!   BORROWED, not copied (LXBC v5). `mapped` is that program's own resident
//!   RAM: the header tables alone.

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
        /// Today's library-activation swap peak: heap copies of the store's
        /// source and blob, envelope encode, then lean decode + budgeted engine.
        swap_vec: usize,
        /// The `flashmap-off` fallback: one transient blob Vec, lean decode
        /// (words copied), budgeted engine. No source Vec, no envelope.
        swap_nomap: usize,
        /// The mapped-flash (XIP) swap peak: lean decode straight off the
        /// mapping + budgeted engine; the blob bytes are not heap at all.
        swap_xip: usize,
        /// Resident RAM of the lean program borrowing the mapping (tables only).
        mapped: usize,
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

        // swap(vec): today's library activation — the store reads land on the
        // heap as String/Vec, get re-encoded into an envelope, and only then
        // are decoded into a program.
        let base = live();
        reset_peak();
        let s: String = src.clone();
        let b: Vec<u8> = blob.clone();
        let env = bytecode::encode_envelope("", &s, &b);
        drop((s, b));
        let le = bytecode::decode_envelope(&env).unwrap();
        let prog = bytecode::deserialize_lean(le.bytecode).unwrap();
        drop(env);
        let mut eng = Engine::from_program_budgeted(prog, 300, 1, 32 * 1024);
        for _ in 0..3 {
            eng.frame(Fx::from_f64(16.7));
        }
        let swap_vec = peak() - base;
        drop(eng);

        // swap(nomap): the flashmap-off path — patterns::bytecode_of reads
        // the bytecode extent into one transient Vec, deserialize_lean copies
        // its words, the Vec is dropped. The source is never read.
        let base = live();
        reset_peak();
        let b: Vec<u8> = blob.clone();
        let prog = bytecode::deserialize_lean(&b).unwrap();
        drop(b);
        let mut eng = Engine::from_program_budgeted(prog, 300, 1, 32 * 1024);
        for _ in 0..3 {
            eng.frame(Fx::from_f64(16.7));
        }
        let swap_nomap = peak() - base;
        drop(eng);

        // swap(xip): bytecode executed from a flash mapping — no heap copy of
        // the blob, no source, no envelope. A leaked, 4-aligned copy of the
        // blob stands in for the mapping (allocated OUTSIDE the measured
        // window); the lean program borrows its words.
        let words: Vec<u32> = vec![0u32; blob.len().div_ceil(4)];
        let leaked = Box::leak(words.into_boxed_slice());
        let flash: &'static [u8] = unsafe {
            std::ptr::copy_nonoverlapping(
                blob.as_ptr(),
                leaked.as_mut_ptr() as *mut u8,
                blob.len(),
            );
            std::slice::from_raw_parts(leaked.as_ptr() as *const u8, blob.len())
        };
        let base = live();
        reset_peak();
        let prog = bytecode::deserialize_lean_static(flash).unwrap();
        assert!(
            matches!(prog.words, luxel_core::vm::Words::Static(_)),
            "{name}: aligned 'static blob must be borrowed"
        );
        let mapped = live() - base;
        let mut eng = Engine::from_program_budgeted(prog, 300, 1, 32 * 1024);
        for _ in 0..3 {
            eng.frame(Fx::from_f64(16.7));
        }
        let swap_xip = peak() - base;
        drop(eng);

        rows.push(Row {
            name,
            blob: blob.len(),
            prog: prog_bytes,
            engine: engine_bytes,
            frames_peak,
            swap_vec,
            swap_nomap,
            swap_xip,
            mapped,
        });
    }

    rows.sort_by_key(|r| std::cmp::Reverse(r.swap_vec));
    println!(
        "\n{:<40} {:>7} {:>8} {:>8} {:>9} {:>9} {:>11} {:>9} {:>7}",
        "pattern", "blob", "program", "engine", "run-peak", "swap(vec)", "swap(nomap)",
        "swap(xip)", "mapped"
    );
    for r in rows.iter().take(25) {
        println!(
            "{:<40} {:>7} {:>8} {:>8} {:>9} {:>9} {:>11} {:>9} {:>7}",
            &r.name[..r.name.len().min(40)],
            r.blob,
            r.prog,
            r.engine,
            r.frames_peak,
            r.swap_vec,
            r.swap_nomap,
            r.swap_xip,
            r.mapped
        );
    }

    let over_vec = rows.iter().filter(|r| r.swap_vec > 45_000).count();
    println!(
        "\n{} of {} patterns exceed 45 KB at swap under swap(vec) (free heap ≈ 50 KB at idle)",
        over_vec,
        rows.len()
    );
    let over_xip: Vec<_> = rows.iter().filter(|r| r.swap_xip > 45_000).collect();
    println!(
        "{} of {} patterns exceed 45 KB at swap under swap(xip):",
        over_xip.len(),
        rows.len()
    );
    for r in &over_xip {
        println!("  {} ({} B)", r.name, r.swap_xip);
    }

    let sum_vec: usize = rows.iter().map(|r| r.swap_vec).sum();
    let sum_nomap: usize = rows.iter().map(|r| r.swap_nomap).sum();
    let sum_xip: usize = rows.iter().map(|r| r.swap_xip).sum();
    println!(
        "sum(swap_nomap)={} B — the flashmap-off fallback, {:.1}% under swap(vec)",
        sum_nomap,
        if sum_vec > 0 {
            100.0 * (sum_vec.saturating_sub(sum_nomap)) as f64 / sum_vec as f64
        } else {
            0.0
        }
    );
    let n = rows.len().max(1);
    let saved = sum_vec.saturating_sub(sum_xip);
    println!(
        "\ntotals: sum(swap_vec)={} B  sum(swap_xip)={} B  avg saving={} B ({:.1}%)",
        sum_vec,
        sum_xip,
        saved / n,
        if sum_vec > 0 {
            100.0 * saved as f64 / sum_vec as f64
        } else {
            0.0
        }
    );
}
