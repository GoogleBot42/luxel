//! How much HEAP the emitter's bookkeeping needs, measured — the number
//! the firmware sizes its compile headroom from (Gitea #665).
//!
//! On the panel a compile runs on the render task beside a resident
//! 4096-px engine, with 20–30 KB of internal heap left, and an allocation
//! that fails there is a panic on a core with no serial port (it was: a
//! `Vec<Kind>` clone inside `plan_all`, 2026-09-23). The image itself no
//! longer touches that heap — `compile_into` writes it straight into the
//! exec block — but the plans, the literal-pool index and the fixup lists
//! still do. This test wraps the global allocator, compiles every
//! `library/*.js` into a pre-allocated buffer, and records the peak of
//! LIVE bytes above the entry level. `firmware/src/jit.rs`'s
//! `EMIT_PER_WORD`/`EMIT_PER_FN`/`EMIT_FIXED` are the same rule; if this test moves the number,
//! move that constant with it.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{env_with_fake_addresses, library, program_of};

/// The per-program rule: bookkeeping bytes <= words * PER_WORD +
/// fns * PER_FN + FIXED. Fitted over `library/` 2026-09-24 (the tightest
/// rule that clears every pattern; `ALLOC_PEAK_CSV=1` prints the rows).
/// Mirrored by `firmware/src/jit.rs` (`EMIT_PER_WORD`, `EMIT_PER_FN`,
/// `EMIT_FIXED`) — the firmware applies it to the heap it has left BEFORE
/// compiling and refuses `no-memory` instead of running out mid-way.
/// Host bytes: `Vec` headers and `usize` are twice the device's, so this
/// overstates the device by roughly a third — safe, not tight.
const PER_WORD: usize = 24;
const PER_FN: usize = 240;
const FIXED: usize = 1024;
/// Sanity ceiling on the largest pattern in `library/`, so a bookkeeping
/// regression shows up as a number and not just as a rule failure.
const CEILING: usize = 96 * 1024;

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(l.size(), Ordering::SeqCst) + l.size();
        PEAK.fetch_max(live, Ordering::SeqCst);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::SeqCst);
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn emitter_bookkeeping_peak_over_the_library() {
    let env = env_with_fake_addresses();
    let mut out = vec![0u8; env.max_code];
    let mut worst = (0usize, String::new());
    let mut n = 0usize;
    for (name, src) in library() {
        let (prog, kinds) = program_of(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let base = LIVE.load(Ordering::SeqCst);
        PEAK.store(base, Ordering::SeqCst);
        let plans = luxel_jit::__plan_all(&prog, &kinds);
        let plan_peak = PEAK.load(Ordering::SeqCst).saturating_sub(base);
        let plan_live = LIVE.load(Ordering::SeqCst).saturating_sub(base);
        drop(plans);
        PEAK.store(base, Ordering::SeqCst);
        let r = luxel_jit::compile_into(&prog, &kinds, &env, &mut out);
        let peak = PEAK.load(Ordering::SeqCst).saturating_sub(base);
        if std::env::var_os("ALLOC_PEAK_CSV").is_some() {
            eprintln!("plan,{name},{},{plan_peak},{plan_live}", prog.words.len());
        }
        let words = prog.words.len();
        let img = r.as_ref().map(|p| p.len_bytes).unwrap_or(0);
        if r.is_ok() {
            n += 1;
        }
        if std::env::var_os("ALLOC_PEAK_CSV").is_some() {
            eprintln!("csv,{name},{words},{img},{peak},{}", prog.fns.len());
        }
        if peak > worst.0 {
            worst = (peak, name.clone());
        }
        // The per-program rule the firmware applies BEFORE compiling
        // (`jit::emit_heap_need`): bookkeeping is linear in the bytecode.
        let fns = prog.fns.len();
        let need = words * PER_WORD + fns * PER_FN + FIXED;
        assert!(
            peak <= need || std::env::var_os("ALLOC_PEAK_CSV").is_some(),
            "{name}: bookkeeping peaked at {peak} B for {words} words / {fns} fns, over \
             the {PER_WORD} B/word + {PER_FN} B/fn + {FIXED} B rule ({need} B) — refit \
             the rule here AND in firmware/src/jit.rs together"
        );
    }
    eprintln!(
        "emitter bookkeeping peak over {n} compiled patterns: {} B ({})",
        worst.0, worst.1
    );
    assert!(
        worst.0 <= CEILING,
        "{}: the emitter's bookkeeping peaked at {} B, over the {CEILING} B ceiling — \
         raise CEILING here (and refit the rule in firmware/src/jit.rs) together",
        worst.1,
        worst.0
    );
}
