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
//! Since Gitea #671 the JIT's compile-time tables ride the same hook: the
//! verifier's per-word stack maps (`kinds::stack_maps`), the planner's
//! per-function tables (`luxel-jit/src/plan.rs`) and the emitter's fixup
//! lists, word-offset table and literal-pool index (`emit.rs`). Those are
//! plain vectors that live for the ~10 ms of one compile on the render task
//! and are touched by nothing else, so neither reason the per-instruction
//! state stays internal applies; on the panel they were the last internal
//! allocation standing between a 4096-px scene and a native second layer.
//! The per-operand-DEPTH scratch of the same compile (the verifier's
//! abstract stacks, the emitter's running stack copy) stays on the
//! ordinary allocator on purpose — a few bytes each, and routing them here
//! cost 6 KB of flash on every board; `kinds::walk_fn` has the numbers. The
//! helpers below ([`filled`], [`from_slice`], [`with_capacity`],
//! [`collect`]) are the `vec!` / `collect()` idioms for an [`ArrVec`],
//! which has no `FromIterator`.
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

/// `vec![x; n]` for an [`ArrVec`].
pub fn filled<T: Clone>(n: usize, x: T) -> ArrVec<T> {
    let mut v: ArrVec<T> = empty();
    v.resize(n, x);
    v
}

/// `Vec::with_capacity(n)` for an [`ArrVec`].
pub fn with_capacity<T>(n: usize) -> ArrVec<T> {
    ArrVec::with_capacity_in(n, ArenaAlloc)
}

/// `s.to_vec()` for an [`ArrVec`].
pub fn from_slice<T: Clone>(s: &[T]) -> ArrVec<T> {
    let mut v: ArrVec<T> = with_capacity(s.len());
    v.extend_from_slice(s);
    v
}

/// `it.collect::<Vec<_>>()` for an [`ArrVec`] — `allocator_api2`'s `Vec`
/// implements `FromIterator` for the global allocator only.
pub fn collect<T, I: IntoIterator<Item = T>>(it: I) -> ArrVec<T> {
    let mut v: ArrVec<T> = empty();
    v.extend(it);
    v
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

/// An arena-backed `String` for request-sized and snapshot-sized text
/// (Gitea #905): the `String` counterpart of [`FrameVec`].
///
/// Identical to a `String` on every host, the wasm playground and every
/// board without PSRAM (no hook → [`ArenaAlloc`] is the global allocator);
/// a PSRAM block on a board that installed one. It exists for the bodies a
/// device builds and throws away — the jsonview snapshots the render task
/// rebuilds every 250 ms, a pattern's JSON, the [`crate::jsonview::Chunks`]
/// segments — which are large, written once and read once, so exactly the
/// kind of thing internal DRAM should not be spent on.
///
/// UTF-8 invariant: the bytes are only ever extended by `&str` or `char`
/// pushes ([`AString::push_str`], [`AString::push`], `fmt::Write`) and only
/// ever shortened by [`AString::clear`], so they are always valid UTF-8 —
/// which is what makes [`AString::as_str`] sound.
#[derive(Clone)]
pub struct AString(ArrVec<u8>);

// By hand: `allocator_api2`'s `Vec` is `Default` for the global allocator only.
impl Default for AString {
    fn default() -> Self {
        AString::new()
    }
}

impl AString {
    /// An empty string; allocates nothing.
    pub const fn new() -> Self {
        AString(empty())
    }

    /// `String::with_capacity(n)` — infallible, like the original.
    pub fn with_capacity(n: usize) -> Self {
        AString(with_capacity(n))
    }

    /// Reserve exactly `n` more bytes, or say no.
    pub fn try_reserve_exact(
        &mut self,
        n: usize,
    ) -> Result<(), allocator_api2::collections::TryReserveError> {
        self.0.try_reserve_exact(n)
    }

    pub fn push_str(&mut self, s: &str) {
        self.0.extend_from_slice(s.as_bytes());
    }

    pub fn push(&mut self, c: char) {
        let mut b = [0u8; 4];
        self.push_str(c.encode_utf8(&mut b));
    }

    pub fn as_str(&self) -> &str {
        // SAFETY: the type's UTF-8 invariant — every byte got here through
        // a `&str` or a `char`.
        unsafe { core::str::from_utf8_unchecked(&self.0) }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// The bytes, still in the arena.
    pub fn into_bytes(self) -> ArrVec<u8> {
        self.0
    }
}

impl core::ops::Deref for AString {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for AString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl core::fmt::Write for AString {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

impl core::fmt::Debug for AString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl core::fmt::Display for AString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for AString {
    fn from(s: &str) -> Self {
        AString(from_slice(s.as_bytes()))
    }
}

impl From<alloc::string::String> for AString {
    fn from(s: alloc::string::String) -> Self {
        AString::from(s.as_str())
    }
}

impl PartialEq for AString {
    fn eq(&self, o: &AString) -> bool {
        self.as_bytes() == o.as_bytes()
    }
}

impl Eq for AString {}

impl PartialEq<str> for AString {
    fn eq(&self, o: &str) -> bool {
        self.as_str() == o
    }
}

impl PartialEq<&str> for AString {
    fn eq(&self, o: &&str) -> bool {
        self.as_str() == *o
    }
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

    /// The `vec!`/`collect` stand-ins build what their std counterparts
    /// would (Gitea #671).
    #[test]
    fn helpers_match_std() {
        let f = filled(3, 7u8);
        assert_eq!(&f[..], &[7, 7, 7]);
        let s = from_slice(&[1u32, 2, 3]);
        assert_eq!(&s[..], &[1, 2, 3]);
        let c = collect((0..4).map(|i| i * 2));
        assert_eq!(&c[..], &[0, 2, 4, 6]);
        let w: ArrVec<u64> = with_capacity(16);
        assert!(w.capacity() >= 16 && w.is_empty());
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

    /// `AString` is a `String` on a host: every way in reads back the same
    /// bytes (Gitea #905).
    #[test]
    fn astring_round_trips_like_a_string() {
        use core::fmt::Write;
        let mut a = AString::new();
        assert!(a.is_empty() && a.capacity() == 0);
        a.try_reserve_exact(32).unwrap();
        assert!(a.capacity() >= 32 && a.is_empty());
        a.push_str("héllo");
        a.push(' ');
        a.push('\u{1f680}');
        write!(a, " {}-{}", 4096, "px").unwrap();
        let want = "héllo \u{1f680} 4096-px";
        assert_eq!(a.as_str(), want);
        assert_eq!(a.len(), want.len());
        assert_eq!(a.as_bytes(), want.as_bytes());
        assert_eq!(alloc::format!("{a}"), want);
        assert_eq!(alloc::format!("{a:?}"), alloc::format!("{want:?}"));
        assert!(a.contains("4096")); // Deref<Target = str>
        let b = a.clone();
        assert_eq!(a, b);
        assert_eq!(b, want);
        assert_eq!(AString::from(want), a);
        assert_eq!(AString::from(alloc::string::String::from(want)), a);
        assert_eq!(&b.into_bytes()[..], want.as_bytes());
        a.clear();
        assert!(a.is_empty());
        assert!(a.try_reserve_exact(usize::MAX / 2).is_err());
        assert!(AString::with_capacity(8).capacity() >= 8);
        assert_eq!(AString::default(), "");
    }
}
