//! Where the VM's array arena gets its memory.
//!
//! Pattern arrays and the per-frame pixel buffer are the parts of a running
//! engine that are both large and cold enough to live somewhere other than
//! internal SRAM. On a board with external PSRAM (Gitea #253) the firmware
//! installs a hook here and every `ArrRepr::Owned` allocation — plus each
//! engine's [`FrameVec`] (Gitea #709) — goes to that external arena instead
//! of the main heap; everything else the VM owns — globals, the operand
//! stack, locals, the arena's own slot vector — stays on the ordinary
//! allocator, because those are touched per *instruction* and PSRAM is
//! several times slower per access.
//!
//! With no hook installed (every host build, the wasm playground, and every
//! board without PSRAM) [`ArenaAlloc`] is exactly the global allocator, so
//! this module changes nothing about how those builds behave.
//!
//! ## Contract for an embedder that installs a hook
//!
//! * [`install`] must be called **once**, before any `Vm` exists, and never
//!   undone: a block allocated through the hook is freed through the hook,
//!   so swapping or removing one under a live arena would free a pointer
//!   into the wrong heap.
//! * The `dealloc` half must accept a pointer from *either* heap. The
//!   `alloc` half is allowed to fall back to the ordinary allocator when
//!   the external arena is full, so the two are not symmetric by
//!   construction — route on the pointer's address, not on bookkeeping.
//! * A null return from `alloc` means a genuine allocation failure and is
//!   reported to the pattern as a runtime error, exactly like a failed
//!   `try_reserve` on the main heap.

use core::alloc::Layout;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};

use allocator_api2::alloc::{AllocError, Allocator, Global};

/// Hook signature: allocate `layout`, or return null.
pub type AllocFn = unsafe fn(Layout) -> *mut u8;
/// Hook signature: free a block previously returned by the paired
/// [`AllocFn`] — or by the ordinary global allocator, if that `AllocFn`
/// fell back to it.
pub type DeallocFn = unsafe fn(*mut u8, Layout);

/// The installed hooks, as raw function-pointer words (`AtomicPtr` would
/// need a data pointer; a `usize` round-trips a `fn` pointer fine on every
/// target this crate builds for). Zero = not installed.
static ALLOC_HOOK: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_HOOK: AtomicUsize = AtomicUsize::new(0);

/// Route arena allocations through `alloc`/`dealloc` instead of the global
/// allocator.
///
/// # Safety
///
/// The caller guarantees the contract in the module docs: called once,
/// before any `Vm` exists, never undone, and `dealloc` accepts pointers
/// from the global allocator as well as from `alloc`.
pub unsafe fn install(alloc: AllocFn, dealloc: DeallocFn) {
    // dealloc first: a reader that sees a non-zero alloc hook must already
    // be able to see the matching dealloc hook.
    DEALLOC_HOOK.store(dealloc as usize, Ordering::Release);
    ALLOC_HOOK.store(alloc as usize, Ordering::Release);
}

/// Whether an external arena is installed. Only diagnostics should care.
pub fn installed() -> bool {
    ALLOC_HOOK.load(Ordering::Acquire) != 0
}

/// Allocator for pattern-array storage: the installed hook if there is one,
/// the global allocator otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaAlloc;

unsafe impl Allocator for ArenaAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let f = ALLOC_HOOK.load(Ordering::Acquire);
        if f == 0 {
            return Global.allocate(layout);
        }
        // SAFETY: `install` only ever stores a valid `AllocFn` here.
        let f: AllocFn = unsafe { core::mem::transmute::<usize, AllocFn>(f) };
        let p = unsafe { f(layout) };
        let p = NonNull::new(p).ok_or(AllocError)?;
        Ok(NonNull::slice_from_raw_parts(p, layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let f = DEALLOC_HOOK.load(Ordering::Acquire);
        if f == 0 {
            unsafe { Global.deallocate(ptr, layout) };
            return;
        }
        // SAFETY: `install` only ever stores a valid `DeallocFn` here, and
        // the hooks are installed before the first arena allocation, so a
        // block allocated without a hook can never reach one.
        let f: DeallocFn = unsafe { core::mem::transmute::<usize, DeallocFn>(f) };
        unsafe { f(ptr.as_ptr(), layout) };
    }
}

/// Element storage of one owned arena array.
pub type ArrVec<T> = allocator_api2::vec::Vec<T, ArenaAlloc>;

/// An empty arena vector. `const` so `ArrRepr::default()` stays free.
pub const fn empty<T>() -> ArrVec<T> {
    ArrVec::new_in(ArenaAlloc)
}

/// One engine's per-frame RGB888 pixel buffer (Gitea #709).
///
/// Same hook as the arrays, for the same reason it exists: at 4096 px a
/// frame is 12,288 B of internal DRAM, and two pattern layers plus the
/// host's staging frame plus the runtime floor is more internal DRAM than
/// the Seengreat panel has. It is the largest single thing a resident
/// engine owns and the only one that is not touched per *instruction* — the
/// VM writes it once per pixel per frame and the compositor reads it once
/// per pixel per frame, both sequentially enough for the S3's data cache to
/// carry (measured on metal: see docs/boards.md "Engine frames in PSRAM").
///
/// Every consumer still sees `&[[u8; 3]]` — [`crate::Engine::pixels`] and
/// the bulk ops deref to a slice exactly as they did.
pub type FrameVec = ArrVec<[u8; 3]>;

/// An engine frame of `pixel_count` black pixels, from the arena if one is
/// installed and the main heap otherwise.
///
/// Infallible, like the `alloc::vec![[0u8; 3]; n]` it replaces: the hook
/// itself already falls back to the main heap when the external arena is
/// full, so the only way this aborts is the way the old code aborted.
pub fn frame(pixel_count: usize) -> FrameVec {
    let mut v: FrameVec = empty();
    v.resize(pixel_count, [0u8; 3]);
    v
}

/// Whether an engine frame allocated *now* would come from the external
/// arena — i.e. whether [`crate::budget::layer_cost`] should charge a
/// layer's 3 B/px to internal DRAM.
///
/// Exactly [`installed`] today: there is one hook and the frame rides it.
/// It is a separate name because the two questions are separate — an
/// embedder could install a hook whose arena is too small for frames — and
/// because the budget's caller reads better for it.
pub fn frames_external() -> bool {
    installed()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No hook → no external frames, which is every host, the wasm
    /// playground and every board without PSRAM. The budget's
    /// `frame_external` argument is this, so those boards charge a layer
    /// its 3 B/px exactly as before (Gitea #709).
    #[test]
    fn frames_are_internal_without_a_hook() {
        assert!(!frames_external());
        let f = frame(300);
        assert_eq!(f.len(), 300);
        assert!(f.iter().all(|p| *p == [0, 0, 0]));
        assert_eq!(crate::budget::layer_cost(300, frames_external()), 4 * 1024 + 900);
    }

    /// Without a hook the arena is the global allocator, which is what
    /// every host and wasm build gets.
    #[test]
    fn default_is_the_global_allocator() {
        assert!(!installed());
        let mut v: ArrVec<u32> = empty();
        v.try_reserve_exact(64).unwrap();
        v.resize(64, 7);
        assert_eq!(v.iter().copied().sum::<u32>(), 7 * 64);
    }
}
