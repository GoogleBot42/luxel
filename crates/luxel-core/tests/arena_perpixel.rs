//! The per-pixel buffers that used to come out of internal SRAM now come out
//! of the array arena (Gitea #768).
//!
//! Its own integration test binary, for the reason `tests/arena.rs` explains:
//! `arena::install` is global and one-way, so a test that installs a hook must
//! not share a process with tests that assume the default arena. The fake
//! arena is the same shape as that file's — a fixed static extent,
//! bump-allocated, frees routed by address the way `firmware/src/psram.rs`
//! routes them.
//!
//! Two properties, both of which the #768 pixel-cap raise depends on:
//!
//! * `outpipe::DeviceChain`'s scratch frame (3 B/px — 49 KB at the panel
//!   board's new 16384-px cap) lands in the arena, and `release` still hands
//!   it back through the hook.
//! * `setPixelState` storage (4 B/px/channel, double-buffered — 131 KB for one
//!   channel at 16384 px) lands in the arena. That one is a *correctness* fix
//!   as much as a capacity one: the VM charges it to the arena byte budget, so
//!   before #768 the bill and the heap disagreed.

use core::alloc::Layout;
use std::sync::atomic::{AtomicUsize, Ordering};

use luxel_core::arena;
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::outpipe::{ChainSettings, DeviceChain, PowerModel};

const ARENA_BYTES: usize = 1 << 20;
static mut ARENA: [u8; ARENA_BYTES] = [0; ARENA_BYTES];

static NEXT: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);
static FALLBACKS: AtomicUsize = AtomicUsize::new(0);

fn base() -> usize {
    &raw const ARENA as usize
}

unsafe fn fake_alloc(layout: Layout) -> *mut u8 {
    let start = base();
    loop {
        let cur = NEXT.load(Ordering::Relaxed);
        let at = (start + cur).next_multiple_of(layout.align()) - start;
        let end = at + layout.size();
        if end > ARENA_BYTES {
            FALLBACKS.fetch_add(1, Ordering::Relaxed);
            return unsafe { std::alloc::alloc(layout) };
        }
        if NEXT.compare_exchange(cur, end, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            return (start + at) as *mut u8;
        }
    }
}

unsafe fn fake_dealloc(ptr: *mut u8, layout: Layout) {
    let a = ptr as usize;
    if a >= base() && a < base() + ARENA_BYTES {
        FREES.fetch_add(1, Ordering::Relaxed);
        return; // a bump arena never reclaims; the test only counts
    }
    unsafe { std::alloc::dealloc(ptr, layout) }
}

fn in_arena<T>(p: *const T) -> bool {
    let a = p as usize;
    a >= base() && a < base() + ARENA_BYTES
}

/// One test per binary would need one binary per property, so both live in
/// this one function: the hook may only be installed once.
#[test]
fn the_output_scratch_and_pixel_state_both_come_from_the_arena() {
    assert!(!arena::installed());
    // SAFETY: single test binary, installed before any Vm exists, and
    // `fake_dealloc` accepts global-allocator pointers — the contract.
    unsafe { arena::install(fake_alloc, fake_dealloc) };
    assert!(arena::installed());

    // ---- DeviceChain's scratch frame ----
    let px = vec![[10u8, 20, 30]; 4096];
    let mut chain = DeviceChain::new();
    let off = ChainSettings::default();
    let on = ChainSettings { blur_pct: 50, ..off };

    chain.apply(&px, &off, 31, None, PowerModel::Strip, Vec::new);
    assert_eq!(chain.resident_bytes(), 0, "an untouched chain holds nothing");

    let frees_before = FREES.load(Ordering::Relaxed);
    let out = chain.apply(&px, &on, 31, None, PowerModel::Strip, Vec::new);
    assert!(in_arena(out.as_ptr()), "the scratch frame must come from the arena");
    assert_eq!(chain.resident_bytes(), 12_288, "3 B/px at 4096 px, as before");

    chain.apply(&px, &off, 31, None, PowerModel::Strip, Vec::new);
    assert_eq!(chain.resident_bytes(), 0, "every stage off still gives it back");
    assert!(
        FREES.load(Ordering::Relaxed) > frees_before,
        "release must free through the hook, not the global allocator"
    );
    assert_eq!(FALLBACKS.load(Ordering::Relaxed), 0, "arena was big enough");

    // ---- setPixelState storage ----
    // Gated on the frame counter so the buffer is allocated on a frame we can
    // bracket: everything else an engine allocates is already warm by then.
    let prog = luxel_core::compile::compile(
        "export var f = 0\n\
         export function beforeRender(d) { f = f + 1 }\n\
         export function render(i) { if (f > 2) setPixelState(0, pixelState(0) + 1) }",
    )
    .expect("compiles");
    let mut e = Engine::from_program_budgeted_at_ext(prog, 256, 1, 4 << 20, 1 << 20, None);
    e.frame(Fx::from_int(16));
    e.frame(Fx::from_int(16));
    assert_eq!(e.pixel_state_bytes(), 0, "nothing written yet, nothing allocated");

    let allocs_before = ALLOCS.load(Ordering::Relaxed);
    e.frame(Fx::from_int(16));
    assert_eq!(e.pixel_state_bytes(), 2 * 256 * 4, "front + back, 4 B/px/channel");
    assert!(
        ALLOCS.load(Ordering::Relaxed) >= allocs_before + 2,
        "front and back must both come from the arena, got {} new allocations",
        ALLOCS.load(Ordering::Relaxed) - allocs_before
    );
    assert_eq!(FALLBACKS.load(Ordering::Relaxed), 0, "arena was big enough");

    // …and the state still round-trips through the frame handover.
    e.frame(Fx::from_int(16));
    assert!(e.pixel_state_bytes() > 0);
}
