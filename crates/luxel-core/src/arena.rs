//! Where the VM's array arena gets its memory.
//!
//! Pattern arrays are the one part of a running engine that is both large
//! and cold enough to live somewhere other than internal SRAM. On a board
//! with external PSRAM (Gitea #253) the firmware installs a hook here and
//! every `ArrRepr::Owned` allocation goes to that external arena instead of
//! the main heap; everything else the VM owns — globals, the operand stack,
//! locals, the arena's own slot vector, the engine's per-frame pixel
//! buffers — stays on the ordinary allocator, because those are touched per
//! pixel per frame and PSRAM is several times slower per access.
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

#[cfg(test)]
mod tests {
    use super::*;

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
