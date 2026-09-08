//! The array arena really does come out of the installed hook (Gitea #253).
//!
//! Its own integration test binary on purpose: `arena::install` is global
//! and deliberately one-way, so a test that installs a hook must not share
//! a process with tests that assume the default (global-allocator) arena.
//!
//! The fake arena here stands in for the firmware's PSRAM heap: a fixed
//! static extent, bump-allocated, with frees routed by address exactly the
//! way `firmware/src/psram.rs` routes them. What it proves is the property
//! the device change depends on — pattern-array storage lands in the arena,
//! and the VM's own working memory does not.

use core::alloc::Layout;
use std::sync::atomic::{AtomicUsize, Ordering};

use luxel_core::arena;
use luxel_core::engine::Engine;
use luxel_core::vm::DEFAULT_ARRAY_BUDGET;

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
    // bump, aligned; overflow falls back to the global allocator, like the
    // firmware hook does when PSRAM is full
    let start = base();
    loop {
        let cur = NEXT.load(Ordering::Relaxed);
        let at = (start + cur).next_multiple_of(layout.align()) - start;
        let end = at + layout.size();
        if end > ARENA_BYTES {
            FALLBACKS.fetch_add(1, Ordering::Relaxed);
            return unsafe { std::alloc::alloc(layout) };
        }
        if NEXT
            .compare_exchange(cur, end, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
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

fn engine(src: &str, pixels: u32) -> Engine {
    let prog = luxel_core::compile::compile(src).expect("compiles");
    Engine::from_program_budgeted_at_ext(prog, pixels, 1, 4 << 20, 1 << 20, None)
}

#[test]
fn pattern_arrays_come_from_the_arena_and_the_vm_does_not() {
    assert!(!arena::installed());
    // SAFETY: single test binary, installed before any Vm exists, and
    // `fake_dealloc` accepts global-allocator pointers — the contract.
    unsafe { arena::install(fake_alloc, fake_dealloc) };
    assert!(arena::installed());

    let before = ALLOCS.load(Ordering::Relaxed);
    let mut e = engine(
        "export var a = array(4096)\n\
         export var b = array(4096)\n\
         export var c = array(4096)\n\
         export function beforeRender(d) { a[0] = d }\n\
         export function render(i) { hsv(a[0], 1, 1) }",
        256,
    );
    // three 4096-element arrays: 12,288 units, well past PB's 10,236 —
    // this pattern only loads at all because the element ledger was raised
    // (the `1 << 20` above), which is the board-scoped divergence #253 buys
    assert!(DEFAULT_ARRAY_BUDGET < 12_288);
    assert!(
        ALLOCS.load(Ordering::Relaxed) >= before + 3,
        "expected at least three arena allocations, got {}",
        ALLOCS.load(Ordering::Relaxed) - before
    );
    assert_eq!(FALLBACKS.load(Ordering::Relaxed), 0, "arena was big enough");

    // the frames still render, reading and writing arena-backed arrays
    let px = e.frame(luxel_core::fixed::Fx::from_int(16)).to_vec();
    assert_eq!(px.len(), 256);

    // ...and the engine's own hot buffer is NOT in the arena
    assert!(
        !in_arena(e.frame(luxel_core::fixed::Fx::from_int(16)).as_ptr()),
        "the per-frame pixel buffer must stay on the ordinary allocator"
    );
}
