//! How much HEAP the emitter's bookkeeping needs, measured — the numbers
//! the firmware sizes its compile headroom from (Gitea #665, #671).
//!
//! On the panel a compile runs on the render task beside a resident
//! 4096-px engine, with 20–30 KB of internal heap left, and an allocation
//! that fails there is a panic on a core with no serial port (it was: a
//! `Vec<Kind>` clone inside `plan_all`, 2026-09-23). The image itself no
//! longer touches that heap — `compile_into` writes it straight into the
//! exec block — and since #671 neither does the bookkeeping: the plans,
//! the stack maps, the literal-pool index and the fixup lists are
//! `luxel_core::arena` vectors, which on a board with external PSRAM go
//! to the arena. This test measures both halves:
//!
//! 1. **The working set** (`PER_WORD`/`PER_FN`/`FIXED`): every byte the
//!    compile allocates, under a counting global allocator with no arena
//!    hook — which is what a board WITHOUT an arena spends internally, and
//!    what a board with one spends in the arena.
//! 2. **The internal residue** (`INT_PER_WORD`/`INT_PER_FN`/`INT_FIXED`):
//!    with a counting arena hook installed, what still reaches the global
//!    allocator — the verifier's per-branch-target map nodes and abstract
//!    stacks, the emitter's running stack copy, the `Placed` result and
//!    its export names. That is what the arena board must still find in
//!    internal DRAM beside the engine.
//!
//! `firmware/src/jit.rs`'s `EMIT_PER_WORD`/`EMIT_PER_FN`/`EMIT_FIXED` and
//! `EMIT_INT_PER_FN`/`EMIT_INT_FIXED` are the same rules; if this test
//! moves a number, move that constant with it.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{env_with_fake_addresses, library, program_of};

/// The per-program rule: working-set bytes <= words * PER_WORD +
/// fns * PER_FN + FIXED. Fitted over `library/` 2026-09-24 (the tightest
/// rule that clears every pattern; `ALLOC_PEAK_CSV=1` prints the rows).
/// Mirrored by `firmware/src/jit.rs` (`EMIT_PER_WORD`, `EMIT_PER_FN`,
/// `EMIT_FIXED`) — the firmware applies it to the heap the working set
/// will come from BEFORE compiling and refuses `no-memory` instead of
/// running out mid-way. Host bytes: `Vec` headers and `usize` are twice
/// the device's, so this overstates the device by roughly a third — safe,
/// not tight.
const PER_WORD: usize = 24;
const PER_FN: usize = 240;
const FIXED: usize = 1024;
/// Sanity ceiling on the largest pattern in `library/`, so a bookkeeping
/// regression shows up as a number and not just as a rule failure.
const CEILING: usize = 96 * 1024;

/// The internal residue with an arena hook installed: bytes <= words *
/// INT_PER_WORD + fns * INT_PER_FN + INT_FIXED. Mirrored by
/// `EMIT_INT_PER_WORD`/`EMIT_INT_PER_FN`/`EMIT_INT_FIXED` in
/// `firmware/src/jit.rs`.
///
/// Refitted over `library/` 2026-09-30 (Gitea #905; the #671 rule was
/// 6/48/1,024 and its tightest pattern used only 72 % of it). What the
/// residue IS was measured then too: at the worst pattern's peak
/// (`2d-fireworks-fade`, 4,122 B) ~3.8 KB is `kinds::walk_fn`'s
/// per-branch-target `BTreeMap` nodes, ~180 B the `FnView` table, and the
/// operand-depth `Vec<Ab>`s only ~150 B — routing those through the arena
/// moved that much for 7.5 KB of flash, so they stay (`kinds::walk_fn`).
/// Hence per word (branch targets scale with code) and per function (the
/// `Placed` result). `words × 3 + fns × 24` is the tightest slope; the
/// fixed term is 1,536 rather than the tightest 1,024 because the patterns
/// that bind are SMALL ones (`novas`, `rainbow-smiley`): at 1,024 the
/// tightest uses 97.5 %, at 1,536 it uses 83 %, for +512 B on every
/// charge.
const INT_PER_WORD: usize = 3;
const INT_PER_FN: usize = 24;
const INT_FIXED: usize = 1536;

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

/// The stand-in arena: `System` directly (so `Counting` never sees it),
/// with its own live/peak counters.
static EXT_LIVE: AtomicUsize = AtomicUsize::new(0);
static EXT_PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe fn ext_alloc(l: Layout) -> *mut u8 {
    let live = EXT_LIVE.fetch_add(l.size(), Ordering::SeqCst) + l.size();
    EXT_PEAK.fetch_max(live, Ordering::SeqCst);
    unsafe { System.alloc(l) }
}

unsafe fn ext_dealloc(p: *mut u8, l: Layout) {
    EXT_LIVE.fetch_sub(l.size(), Ordering::SeqCst);
    unsafe { System.dealloc(p, l) }
}

/// One test, two passes: the hook is process-global and irreversible, so
/// the no-hook measurement has to run first and in the same thread.
#[test]
fn emitter_bookkeeping_peak_over_the_library() {
    let csv = std::env::var_os("ALLOC_PEAK_CSV").is_some();
    let env = env_with_fake_addresses();
    let mut out = vec![0u8; env.max_code];

    // ---- pass 1: the working set, no arena --------------------------
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
        if csv {
            eprintln!("plan,{name},{},{plan_peak},{plan_live}", prog.words.len());
        }
        let words = prog.words.len();
        let img = r.as_ref().map(|p| p.len_bytes).unwrap_or(0);
        if r.is_ok() {
            n += 1;
        }
        if csv {
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
            peak <= need || csv,
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

    // ---- pass 2: the internal residue, arena installed ----------------
    // SAFETY: once, before any `Vm` exists (this binary never builds one),
    // never undone; `ext_dealloc` accepts any `System` pointer, which is
    // what the global allocator hands out here too.
    unsafe { luxel_core::arena::install(ext_alloc, ext_dealloc) };
    assert!(luxel_core::arena::installed());
    let mut worst_int = (0usize, String::new());
    let mut worst_ext = (0usize, String::new());
    for (name, src) in library() {
        let (prog, kinds) = program_of(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let base = LIVE.load(Ordering::SeqCst);
        PEAK.store(base, Ordering::SeqCst);
        let ext_base = EXT_LIVE.load(Ordering::SeqCst);
        EXT_PEAK.store(ext_base, Ordering::SeqCst);
        let r = luxel_jit::compile_into(&prog, &kinds, &env, &mut out);
        let int_peak = PEAK.load(Ordering::SeqCst).saturating_sub(base);
        let ext_peak = EXT_PEAK.load(Ordering::SeqCst).saturating_sub(ext_base);
        let words = prog.words.len();
        let fns = prog.fns.len();
        if csv {
            eprintln!("split,{name},{words},{fns},{ext_peak},{int_peak},{}", r.is_ok());
        }
        if int_peak > worst_int.0 {
            worst_int = (int_peak, name.clone());
        }
        if ext_peak > worst_ext.0 {
            worst_ext = (ext_peak, name.clone());
        }
        // Everything measured in pass 1 must have moved to the arena…
        let need = words * PER_WORD + fns * PER_FN + FIXED;
        assert!(
            ext_peak <= need || csv,
            "{name}: the arena side peaked at {ext_peak} B, over the working-set rule ({need} B)"
        );
        // …and what is left internal is the small rule the arena board
        // applies to its internal heap (`jit::emit_int_need`).
        let int_need = words * INT_PER_WORD + fns * INT_PER_FN + INT_FIXED;
        assert!(
            int_peak <= int_need || csv,
            "{name}: {int_peak} B still reached the internal heap with an arena installed, \
             over the {INT_PER_WORD} B/word + {INT_PER_FN} B/fn + {INT_FIXED} B residue \
             rule ({int_need} B) — something in the compile path allocates outside \
             `luxel_core::arena`; refit here AND in firmware/src/jit.rs together"
        );
    }
    eprintln!(
        "with an arena: arena peak {} B ({}), internal residue peak {} B ({})",
        worst_ext.0, worst_ext.1, worst_int.0, worst_int.1
    );
}
