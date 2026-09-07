//! On-device pattern library — the firmware half of the `/api/patterns`
//! CRUD contract the native mirror (crates/luxel-cli/src/serve.rs) and the
//! playground already speak.
//!
//! # Storage: one mappable extent region + a small key area (Gitea #330)
//!
//! The `storage` partition (0x210000, 1 MB) was freed by dropping factory
//! (partitions.csv). We resolve it from the *live* partition table at boot
//! and disable the store if it is absent — so this firmware is safe even on
//! the old table, where 0x210000 is still the live ota_1 app slot.
//!
//! The partition is split by what the device actually does with the bytes,
//! not in half:
//!
//! | rel | abs | size | what |
//! |---|---|---:|---|
//! | `0x00000` | `0x210000` | 128 KiB (32 pages) | **key area** — a `sequential-storage` map: format key, playlist, playstate, pixel map, resume, palette, and the pattern **directory** |
//! | `0x20000` | `0x230000` | 896 KiB (224 pages) | **extent region** — mapped read-only through the cache MMU at boot |
//!
//! Everything large and immutable lives in the extent region as a
//! **contiguous run of 4 KiB pages** (an *extent*), which is the one
//! property XIP needs (docs/research/flash-mmap.md): the VM executes a
//! pattern's LXBC in place and the HTTP layer serves its source straight
//! out of the mapping. Within the region:
//!
//! | rel | pages | what |
//! |---|---:|---|
//! | `0x20000` | 1 | ad-hoc header page: magic, src/bc lengths, bc side |
//! | `0x21000` | 8 | ad-hoc (live-coding) source, 32 KiB |
//! | `0x29000` | 2 × 16 | ad-hoc bytecode, TWO sides of 64 KiB — a push writes the side the running engine is not executing from |
//! | `0x49000` | 183 | the **extent arena**, 732 KiB |
//!
//! A save is **one extent write per blob** — the source and the bytecode,
//! each once. There are no chunk items and no duplicate copy: what the API
//! serves back is the same bytes the VM runs from.
//!
//! # The key area
//!
//! `sequential-storage` is an established, power-loss-safe, wear-leveled
//! key→value map over NOR flash. It is exactly right for what is small and
//! written often — and wrong for anything that must be mapped (it caps an
//! item at one page, stores it unaligned, and relocates it on GC). So it
//! keeps only the small hot keys, 128 KiB of them:
//!
//! | key | bytes | written |
//! |---|---:|---|
//! | [FORMAT_KEY] | 4 | once, at a wipe |
//! | [PLAYLIST_KEY] | ≤ [BLOB_MAX] | per playlist edit |
//! | [PLAYSTATE_KEY] | 1 | per play/pause |
//! | [MAP_KEY] | ≤ [BLOB_MAX] | per pixel-map upload |
//! | [RESUME_KEY] | ~16 | per swap (deduped: skipped when unchanged) |
//! | [PALETTE_KEY] | ≤ 64 | per palette edit |
//! | [DIR_KEY] | ≤ 3,398 | per save / delete / compaction move |
//!
//! ~12 KB of live data in 128 KiB gives the log an order of magnitude of
//! GC headroom.
//!
//! # The directory
//!
//! ONE map item ([DIR_KEY]) holds the whole store's metadata, so the
//! pattern list and the extent table can never disagree:
//!
//! ```text
//! [ver u8][npat u8]  npat × { seq u32, gen u8, name_len u8, name }
//! [extents::Dir::to_bytes]  — seq, gen, kind, start page, len, FNV-1a
//! ```
//!
//! Each pattern has a small monotonic **seq** (its API id is
//! `seq ^ ID_MASK`, mirroring serve.rs) and a **generation** that flips on
//! every save. A save writes the new generation's extents, verifies them,
//! and only then publishes this item — the single atomic commit point. A
//! power cut before it leaves the previous generation fully intact and the
//! new extents unreferenced (boot rebuilds the page bitmap from the
//! directory, so their pages come back free).
//!
//! # Format changes
//!
//! [FORMAT_VERSION] wipes the key area on mismatch. There is no migration
//! code: the playground re-syncs the library (Jeremy's decision, #330).
//!
//! # Flash access
//!
//! sequential-storage is async; esp-storage's FlashStorage is blocking (see
//! the AsyncFlash adapter). Each key-area transaction *leases* the driver
//! out of the OTA module (never holding its critical-section mutex across
//! erases) and drives the ops with `block_on` (the adapter never truly
//! pends). Extent writes go through [crate::ota::with_flash] one op at a
//! time with yields between — the fenced door every other writer uses.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::ops::Range;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_futures::block_on;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;
use esp_storage::FlashStorage;
use luxel_core::jsonview::{json_escape, push_hex, push_piece, push_u32};
use sequential_storage::cache::PageStateCache;
use sequential_storage::map;

use crate::extents::{self, KIND_BC, KIND_SRC};

// --- blocking → async flash adapter ---
// esp-storage's FlashStorage implements the *blocking* NorFlash traits;
// sequential-storage wants the *async* ones. embassy's BlockingAsync would
// work but only over an *owned* flash (embedded-storage 0.3 lacks a
// `MultiwriteNorFlash for &mut T` blanket impl, which `remove_item` needs)
// and offers no way to get the flash back out. This borrows `&mut
// FlashStorage` instead — the lease keeps ownership — and forwards each
// async method to the blocking one (which completes immediately, so
// `block_on` never truly pends).
use embedded_storage::nor_flash::ErrorType as BlockingErrorType;
use embedded_storage::nor_flash::{NorFlash as BlockingNorFlash, ReadNorFlash as BlockingRead};
use embedded_storage_async::nor_flash as anf;

struct AsyncFlash<'a> {
    flash: &'a mut FlashStorage<'static>,
}
impl<'a> AsyncFlash<'a> {
    fn new(flash: &'a mut FlashStorage<'static>) -> Self { Self { flash } }
    fn invalidate(&mut self) {}
}

impl anf::ErrorType for AsyncFlash<'_> {
    type Error = <FlashStorage<'static> as BlockingErrorType>::Error;
}
impl anf::ReadNorFlash for AsyncFlash<'_> {
    const READ_SIZE: usize = <FlashStorage<'static> as BlockingRead>::READ_SIZE;
    // Every op is fenced individually (dual-core: the other core parks for
    // the op — core1.rs): this adapter drives a LEASED driver, outside
    // ota::with_flash's fence. Hence the page cache: a fence is expensive
    // (an interrupt on the other core, and this core's interrupts held off
    // for the op), so reads that fit a cached page must not take one.
    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        crate::core1::fenced_as(crate::core1::tag::STORE_READ, || {
            BlockingRead::read(self.flash, offset, bytes)
        })
    }
    fn capacity(&self) -> usize {
        BlockingRead::capacity(self.flash)
    }
}
impl anf::NorFlash for AsyncFlash<'_> {
    const WRITE_SIZE: usize = <FlashStorage<'static> as BlockingNorFlash>::WRITE_SIZE;
    const ERASE_SIZE: usize = <FlashStorage<'static> as BlockingNorFlash>::ERASE_SIZE;
    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.invalidate();
        crate::core1::fenced_as(crate::core1::tag::STORE_ERASE, || {
            BlockingNorFlash::erase(self.flash, from, to)
        })
    }
    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.invalidate();
        crate::core1::fenced_as(crate::core1::tag::STORE_WRITE, || {
            BlockingNorFlash::write(self.flash, offset, bytes)
        })
    }
}
impl anf::MultiwriteNorFlash for AsyncFlash<'_> {}

/// The `storage` partition — ours exclusively (partitions.csv). Expected
/// bounds; the actual region is resolved from the live table at boot.
pub const PAT_START: u32 = 0x21_0000;
pub const PAT_LEN: u32 = 0x10_0000;

/// Erase-page size — the extent allocator's unit, and the map's page.
const PAGE: u32 = 4096;

/// Flash range `sequential-storage` manages: the KEY AREA, the first
/// 128 KiB / 32 pages of the partition. Small, hot, power-loss-safe keys
/// only (see the module docs' table) — ~12 KB of live data, so the log has
/// ~10× GC headroom, and every op's page scan is 4× cheaper than the old
/// 512 KiB range.
const STORE_LEN: u32 = 0x2_0000;

/// The EXTENT REGION: everything after the key area, mapped read-only
/// through the cache MMU at boot. Both bounds are 64 KiB-aligned (the MMU
/// page size), so this is 14 MMU entries.
const EXT_OFF: u32 = STORE_LEN;
const EXT_LEN: u32 = PAT_LEN - STORE_LEN;
const _: () = assert!((PAT_START + EXT_OFF) % 0x1_0000 == 0);
const _: () = assert!(EXT_LEN % 0x1_0000 == 0);

/// Resolved flash offset of the `storage` partition, or 0 if absent (old
/// table) — every op then refuses / reads empty. Set once in [init].
static REGION: AtomicU32 = AtomicU32::new(0);
/// Next pattern seq (monotonic). API id = `seq ^ ID_MASK` (mirrors serve.rs).
static NEXT_SEQ: AtomicU32 = AtomicU32::new(0);
const ID_MASK: u32 = 0x5eed_1e55;

const MAX_NAME: usize = 64;
/// Largest source text the store accepts: 8 pages. Was 30,720 (the old
/// 8 × 3,840-byte chunk budget); rounded up to the extent granularity now
/// that a source is one extent, and matched by the ad-hoc slot's own
/// 32 KiB region. The 16 KiB HTTP request buffer bounds a POST well below
/// this anyway.
pub const MAX_SOURCE: usize = 8 * PAGE as usize; // 32 KiB
/// Largest LXBC the store accepts: 10 pages. Was 38,400 (10 × 3,840);
/// rounded up to the page. LXBC can run larger than its source, so it gets
/// the bigger cap. Both sit comfortably past what the device's heap can
/// actually run — the RAM floor, not flash, is the real ceiling.
pub const MAX_BC: usize = 10 * PAGE as usize; // 40 KiB
/// Patterns the directory holds. Was 24 while every pattern cost a dozen
/// map items; the arena is 183 pages and the directory is one map item, so
/// the binding constraint is now that item's one-page cap — see the
/// [DIR_SER_MAX] assert. 32 patterns of the median size use ~64 of the
/// 183 pages.
const MAX_PATTERNS: usize = 32;
/// Largest value [store_blob] accepts: safely under one 4 KiB page
/// alongside the u32 key + item header.
pub const BLOB_MAX: usize = 3840;
/// sequential-storage scratch: ≥ the largest item (one blob + key + header).
const BUF: usize = 4096;

/// On-flash layout version. Bumped when the key/value scheme changes; a
/// mismatch at boot **wipes** the key area — there is deliberately no
/// migration code (#330: the playground re-syncs the library). Stored
/// under a reserved meta-space key no real seq can reach.
/// v3: bytecode chunks + bc_count in the meta (patterns carry LXBC).
/// v4: bigger chunk budgets (MC 4→8, MC_BC 6→10).
/// v5: the chunk store is gone — key area + extent region (#330). Source
/// and bytecode are extents; one directory item replaces every meta and
/// chunk key.
const FORMAT_VERSION: u32 = 5;
const FORMAT_KEY: u32 = 0x7FFF_FFFF;

fn id_hex(seq: u32) -> String {
    let mut out = String::new();
    push_hex(&mut out, seq ^ ID_MASK, 8);
    out
}
fn seq_of(id: &str) -> Option<u32> {
    u32::from_str_radix(id, 16).ok().map(|v| v ^ ID_MASK)
}

/// RAM index entry — one per stored pattern. The bytes live in extents;
/// this is the name/identity half of the directory record.
#[derive(Clone)]
struct Entry {
    seq: u32,
    gen: u8,
    name: String,
}

static INDEX: BlockingMutex<CriticalSectionRawMutex, RefCell<Vec<Entry>>> =
    BlockingMutex::new(RefCell::new(Vec::new()));

/// Pages sequential-storage manages — the const the cache below is sized
/// by; wrong and the crate panics at some point.
const PAGES: usize = (STORE_LEN / PAGE) as usize;

/// The ONE sequential-storage cache for the key area, kept alive across
/// transactions.
///
/// sequential-storage wants exactly one live cache per region, passed to
/// every call. Building a fresh `NoCache` per call — which is what this
/// did — meant every `fetch_item`/`store_item` re-scanned all
/// [PAGES] pages and every item header in them, and on a dual-core board
/// each of those reads is an individually FENCED flash op. Measured on the
/// Athom (Gitea #292): **81,143 fenced reads for one 650-byte POST
/// /api/patterns**, ~13 s of them, which blocked the ProCpu executor past
/// the 20 s RTC watchdog and rebooted the board mid-save.
///
/// Sound because of the LEASE: a transaction owns the flash driver
/// ([crate::ota::take_flash] hands it out to one caller at a time, across
/// both cores), so the `&mut` below is exclusive for exactly as long as
/// the driver is out, and nothing else writes this range (the extent
/// region starts at [EXT_OFF] = `STORE_LEN`, past the map's range). Any
/// write that goes AROUND sequential-storage inside the range (the format
/// wipe in [init]) must call `invalidate_cache_state()`.
struct StoreCache(core::cell::UnsafeCell<PageStateCache<PAGES>>);
// SAFETY: only reachable through `store_cache`, whose caller holds the
// exclusive flash lease (see above).
unsafe impl Sync for StoreCache {}
static STORE_CACHE: StoreCache = StoreCache(core::cell::UnsafeCell::new(PageStateCache::new()));

/// The shared cache. Call only from inside a `with_store!` body or a
/// helper it calls — i.e. while the flash lease is held.
#[allow(clippy::mut_from_ref)]
fn store_cache() -> &'static mut PageStateCache<PAGES> {
    // SAFETY: the flash lease makes this borrow exclusive; see StoreCache.
    unsafe { &mut *STORE_CACHE.0.get() }
}

/// Leases the flash driver out of the OTA module and returns it on drop.
struct FlashLease(Option<FlashStorage<'static>>);
impl Drop for FlashLease {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            crate::ota::give_flash(f);
        }
    }
}

/// Lease flash, wrap it async, run a sequential-storage transaction to
/// completion. `None` if an OTA owns the flash. `$body` is an async block; it
/// must copy borrowed values out to owned before returning.
macro_rules! with_store {
    ($start:expr, |$af:ident, $range:ident, $buf:ident| $body:block) => {{
        let mut lease = FlashLease(crate::ota::take_flash());
        match lease.0.as_mut() {
            None => None,
            Some(flash) => {
                #[allow(unused_mut)]
                let mut $af = AsyncFlash::new(flash);
                let $range: Range<u32> = $start..($start + STORE_LEN);
                let mut buf_vec = alloc::vec![0u8; BUF];
                let $buf: &mut [u8] = buf_vec.as_mut_slice();
                Some(block_on(async move { $body }))
            }
        }
    }};
}

// --- the persisted directory ---

/// Reserved key holding the whole store directory (pattern section +
/// extent table). One item, one write, one atomic commit point.
const DIR_KEY: u32 = 0x7FFF_FFF9;
/// Pattern-section version (the extent table carries [extents::DIR_VER]).
const DIR_PAT_VER: u8 = 1;
/// Worst-case pattern section: ver + count + every name at [MAX_NAME].
const PAT_SEC_MAX: usize = 2 + MAX_PATTERNS * (4 + 1 + 1 + MAX_NAME);
/// Worst-case whole directory item. It must fit ONE map item — that is what
/// caps [MAX_PATTERNS].
const DIR_SER_MAX: usize = PAT_SEC_MAX + extents::SER_MAX;
const _: () = assert!(DIR_SER_MAX <= BLOB_MAX);

// --- small reserved-key blobs (playlist definition + playback state) ---
// These live in the key area alongside FORMAT_KEY and DIR_KEY. Each must
// fit one flash page.
pub const PLAYLIST_KEY: u32 = 0x7FFF_FFFE;
pub const PLAYSTATE_KEY: u32 = 0x7FFF_FFFD;
pub const MAP_KEY: u32 = 0x7FFF_FFFC;
/// Single-pattern resume record (see resume.rs).
pub const RESUME_KEY: u32 = 0x7FFF_FFFB;
/// Device output palette (see outpal.rs) — variable-length, so it lives
/// here rather than in the fixed-size nvs device record.
pub const PALETTE_KEY: u32 = 0x7FFF_FFFA;

/// Store a small blob under a reserved key. False if storage is unavailable or
/// the blob is too large for one page.
pub fn store_blob(key: u32, bytes: &[u8]) -> bool {
    if bytes.len() > BLOB_MAX {
        return false;
    }
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return false;
    }
    with_store!(start, |af, range, buf| {
        let cache = store_cache();
        let b: &[u8] = bytes;
        map::store_item(&mut af, range.clone(), cache, buf, &key, &b)
            .await
            .is_ok()
    })
    .unwrap_or(false)
}

/// Read a blob previously written with [store_blob].
pub fn read_blob(key: u32) -> Option<Vec<u8>> {
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return None;
    }
    with_store!(start, |af, range, buf| {
        let cache = store_cache();
        match map::fetch_item::<u32, &[u8], _>(&mut af, range, cache, buf, &key).await {
            Ok(Some(b)) => Some(b.to_vec()),
            _ => None,
        }
    })
    .flatten()
}

// --- the extent region layout ---
//
// The ad-hoc (live-coding) read-back slot sits at the head of the region.
// The source + blob of the *currently running* pattern used to live in two
// standing heap Vecs (shared::PATTERN_SRC / PATTERN_BC) purely for
// read-back (GET /api/pattern, the sync envelope, engine rebuilds). On a
// heap already dominated by that same running pattern they were the single
// largest resident cost, so they live in flash and are served from the
// mapping. LIBRARY patterns do not use this slot at all — their bytes are
// already extents; only an ad-hoc push (/api/code, sync adoption) writes
// here, which is the flash-wear rule (2026-08-15).
//
// Layout (offsets relative to the partition base):
//   CUR_OFF          header page: [magic u32][src_len u32][bc_len u32]
//                    [bc_side u32], written LAST so a power loss mid-store
//                    leaves a stale header at worst (the slot is ignored at
//                    boot anyway -- RAM meta rules within a session)
//   CUR_SRC_OFF      ad-hoc source bytes (up to CUR_SRC_MAX)
//   CUR_BC_OFF       ad-hoc LXBC bytes, TWO sides of CUR_BC_MAX: a push
//                    writes the side the running engine is NOT executing
//                    from, so a mapped engine never sees its code change
//   ARENA_OFF        the extent arena: ARENA_PAGES 4 KiB pages carved into
//                    contiguous, 4-byte-aligned EXTENTS by the page-granular
//                    allocator in extents.rs -- one per stored blob (source
//                    and bytecode alike); the directory lives under DIR_KEY
const CUR_OFF: u32 = EXT_OFF;
/// 32 KiB — [MAX_SOURCE], the hard cap on a stored source and therefore on
/// anything read-back can ever want.
const CUR_SRC_MAX: u32 = 8 * PAGE;
const CUR_BC_MAX: u32 = 16 * PAGE; // 64 KiB per side
const CUR_SRC_OFF: u32 = CUR_OFF + PAGE;
const CUR_BC_OFF: u32 = CUR_SRC_OFF + CUR_SRC_MAX;
const CUR_MAGIC: u32 = 0x4C58_4350; // "LXCP"
/// The bc side holding the RUNNING ad-hoc blob (the last successful
/// store_current); the next store writes the other one.
static CUR_BC_SIDE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
/// The arena: everything left in the extent region after the ad-hoc slot.
/// 0x49000..0x100000 = 732 KiB = 183 pages.
const ARENA_OFF: u32 = CUR_BC_OFF + 2 * CUR_BC_MAX;
const ARENA_PAGES: usize = ((EXT_OFF + EXT_LEN - ARENA_OFF) / PAGE) as usize;
const _: () = assert!(CUR_SRC_MAX as usize >= MAX_SOURCE);
const _: () = assert!(CUR_BC_MAX as usize >= MAX_BC);
const _: () = assert!(ARENA_OFF + (ARENA_PAGES as u32) * PAGE <= EXT_OFF + EXT_LEN);
const _: () = assert!(ARENA_PAGES <= extents::MAX_PAGES);
const _: () = assert!(ARENA_PAGES * extents::PAGE >= MAX_SOURCE + MAX_BC);
const _: () = assert!(PAGE as usize == extents::PAGE);

fn cur_bc_off(side: u8) -> u32 {
    CUR_BC_OFF + side as u32 * CUR_BC_MAX
}

/// Partition-relative offset of an arena page.
fn arena_off(page: u16) -> u32 {
    ARENA_OFF + page as u32 * PAGE
}

/// Absolute flash offsets of the ad-hoc slot's (src, bc) data, or None when
/// the storage partition is absent. Lengths come from the RAM meta
/// (shared::SrcLoc/BcLoc) -- the header page is for future boot-time use.
pub fn current_slot_abs() -> Option<(u32, u32)> {
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return None;
    }
    let side = CUR_BC_SIDE.load(Ordering::Relaxed);
    Some((start + CUR_SRC_OFF, start + cur_bc_off(side)))
}

// --- the mapped extent region (flashmap.rs) ---

static RAW_BASE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static RAW_MAPPED_LEN: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// The extent region as memory, if the boot-time mapping succeeded; None =
/// every reader takes the read_nor path instead (`flashmap-off`, or a
/// refused self-check) — the store still works, it just copies.
pub fn raw() -> Option<&'static [u8]> {
    let base = RAW_BASE.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    let len = RAW_MAPPED_LEN.load(Ordering::Acquire);
    // SAFETY: base/len come from a flashmap::Mapped leaked in map_ext.
    Some(unsafe { core::slice::from_raw_parts(base as *const u8, len) })
}

/// Map the extent region read-only through the cache MMU and prove it (the
/// header page read both ways must agree) — same discipline as
/// assets::map_region. Once, from init, right after REGION resolves.
#[inline(never)]
fn map_ext(start: u32) {
    let m = match crate::flashmap::map(start + EXT_OFF, EXT_LEN) {
        Ok(m) => m,
        Err(e) => {
            println!("flashmap: pattern store not mapped ({:?}) — flash-controller reads", e);
            return;
        }
    };
    let mut via_nor = alloc::vec![0u8; PAGE as usize];
    let ok = crate::assets::read_chunk(start + EXT_OFF, &mut via_nor)
        && via_nor[..] == m.bytes()[..PAGE as usize];
    drop(via_nor);
    if !ok {
        println!(
            "flashmap: pattern store self-check FAILED at 0x{:x} — unmapping, flash-controller reads",
            m.vaddr()
        );
        crate::flashmap::unmap(m);
        return;
    }
    println!(
        "flashmap: pattern store 0x{:x}+0x{:x} -> 0x{:x} ({} x {} KiB pages from entry {}), self-check ok",
        m.phys(),
        EXT_LEN,
        m.vaddr(),
        m.pages(),
        crate::flashmap::page_size() / 1024,
        m.first_entry()
    );
    let b = m.leak();
    RAW_MAPPED_LEN.store(b.len(), Ordering::Release);
    RAW_BASE.store(b.as_ptr() as usize, Ordering::Release);
}

/// `len` bytes at partition-relative `rel` (inside the extent region) as
/// mapped memory.
fn raw_slice(rel: u32, len: usize) -> Option<&'static [u8]> {
    let r = raw()?;
    let s = rel.checked_sub(EXT_OFF)? as usize;
    r.get(s..s.checked_add(len)?)
}

/// After a raw write: drop the cache lines the write made stale.
fn raw_invalidate(rel: u32, len: usize) {
    if let Some(s) = raw_slice(rel, len) {
        crate::flashmap::invalidate_slice(s);
    }
}

/// The ad-hoc read-back slot's running bytecode side, mapped.
pub fn current_slot_code(len: usize) -> Option<&'static [u8]> {
    if len == 0 || len > CUR_BC_MAX as usize || REGION.load(Ordering::Relaxed) == 0 {
        return None;
    }
    raw_slice(cur_bc_off(CUR_BC_SIDE.load(Ordering::Relaxed)), len)
}

/// The ad-hoc read-back slot's source, mapped.
pub fn current_slot_src(len: usize) -> Option<&'static [u8]> {
    if len == 0 || len > CUR_SRC_MAX as usize || REGION.load(Ordering::Relaxed) == 0 {
        return None;
    }
    raw_slice(CUR_SRC_OFF, len)
}

const FNV_INIT: u32 = 0x811c_9dc5;

/// Incremental FNV-1a (same function as luxel_core::netin::fnv1a).
fn fnv1a_update(mut h: u32, bytes: &[u8]) -> u32 {
    for &b in bytes {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h
}

fn fnv1a(bytes: &[u8]) -> u32 {
    fnv1a_update(FNV_INIT, bytes)
}

// --- reading an extent ---

/// An extent's bytes as MAPPED memory — the zero-copy path. None when the
/// region is not mapped (`flashmap-off` / refused self-check).
fn ext_slice(e: &extents::Extent) -> Option<&'static [u8]> {
    raw_slice(arena_off(e.start), e.len as usize)
}

/// An extent's bytes in a transient fallible Vec — the `flashmap-off`
/// fallback, and the only path that ever copies a stored blob.
fn ext_vec(e: &extents::Extent) -> Option<Vec<u8>> {
    let len = e.len as usize;
    let mut out: Vec<u8> = Vec::new();
    if out.try_reserve_exact(len).is_err() {
        println!("store: {} B extent buffer failed to allocate", len);
        return None;
    }
    if let Some(b) = ext_slice(e) {
        out.extend_from_slice(b);
        return Some(out);
    }
    let region = REGION.load(Ordering::Relaxed);
    if region == 0 {
        return None;
    }
    out.resize(len, 0);
    let base = region + arena_off(e.start);
    let mut at = 0usize;
    while at < len {
        let n = (len - at).min(PAGE as usize);
        if !crate::assets::read_chunk(base + at as u32, &mut out[at..at + n]) {
            return None;
        }
        at += n;
    }
    Some(out)
}

/// FNV-1a of an extent's flash bytes — through the mapping when there is
/// one, else streamed a page at a time through the flash controller (no
/// large allocation either way). This is the proof a write landed and the
/// boot-time check that an extent is not torn.
fn ext_hash(e: &extents::Extent) -> Option<u32> {
    if let Some(b) = ext_slice(e) {
        return Some(fnv1a(b));
    }
    let region = REGION.load(Ordering::Relaxed);
    if region == 0 {
        return None;
    }
    let len = e.len as usize;
    let base = region + arena_off(e.start);
    let mut buf = alloc::vec![0u8; PAGE as usize];
    let mut h = FNV_INIT;
    let mut at = 0usize;
    while at < len {
        let n = (len - at).min(PAGE as usize);
        if !crate::assets::read_chunk(base + at as u32, &mut buf[..n]) {
            return None;
        }
        h = fnv1a_update(h, &buf[..n]);
        at += n;
    }
    Some(h)
}

// --- the extent directory + its concurrency rules ---

/// Zero pages until [init] installs the real size — an all-zero
/// initializer keeps the directory in .bss instead of .data (it is image
/// bytes otherwise, and the OTA slot is the scarce resource). A 0-page
/// directory refuses every allocation, which is exactly what a device
/// whose store never came up should do.
static ARENA: BlockingMutex<CriticalSectionRawMutex, RefCell<extents::Dir>> =
    BlockingMutex::new(RefCell::new(extents::Dir::new(0)));

/// Store concurrency: `0` idle, [WRITER] a mutating transaction in flight,
/// anything else a count of mapped readers.
///
/// A mutation (save, delete, compaction) frees and moves pages an unpinned
/// reader may be looking at, so the two exclude each other. Pinned reads
/// (the RUNNING pattern — see the pin set) need no guard at all: nothing
/// ever moves or frees a pinned extent. Readers hold the guard only across
/// SYNCHRONOUS copies (microseconds), so a writer's retry converges.
static BUSY: AtomicU32 = AtomicU32::new(0);
const WRITER: u32 = u32::MAX;

struct StoreWrite;
struct MapRead;

impl StoreWrite {
    /// None when a mutation or an unpinned mapped read is in flight (the
    /// caller retries or degrades; it never blocks).
    fn take() -> Option<StoreWrite> {
        // ARENA.lock is the critical section that makes this test-and-set
        // atomic across both cores -- no extra primitive needed.
        ARENA.lock(|_| {
            if BUSY.load(Ordering::Relaxed) != 0 {
                return None;
            }
            BUSY.store(WRITER, Ordering::Relaxed);
            Some(StoreWrite)
        })
    }
    /// Same, waiting up to ~1 s for a reader or another writer to finish.
    async fn acquire() -> Option<StoreWrite> {
        for _ in 0..50 {
            if let Some(g) = StoreWrite::take() {
                return Some(g);
            }
            embassy_time::Timer::after(embassy_time::Duration::from_millis(20)).await;
        }
        None
    }
}
impl Drop for StoreWrite {
    fn drop(&mut self) {
        ARENA.lock(|_| BUSY.store(0, Ordering::Relaxed));
    }
}

impl MapRead {
    fn take() -> Option<MapRead> {
        ARENA.lock(|_| {
            let n = BUSY.load(Ordering::Relaxed);
            if n == WRITER || n == WRITER - 1 {
                return None;
            }
            BUSY.store(n + 1, Ordering::Relaxed);
            Some(MapRead)
        })
    }
    /// Same, waiting up to ~200 ms for a mutation to finish.
    async fn acquire() -> Option<MapRead> {
        for _ in 0..20 {
            if let Some(g) = MapRead::take() {
                return Some(g);
            }
            embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
        }
        None
    }
}
impl Drop for MapRead {
    fn drop(&mut self) {
        ARENA.lock(|_| {
            let n = BUSY.load(Ordering::Relaxed);
            BUSY.store(n.saturating_sub(1), Ordering::Relaxed);
        });
    }
}

/// Serialize the whole directory — pattern section then extent table — and
/// write it as ONE map item. This is the store's atomic commit point: an
/// extent is only ever reachable after the item that names it landed. The
/// staging buffer is a heap Vec, never a stack array: this runs at
/// picoserve depth on the shared main-task stack.
fn persist_dir() -> bool {
    let mut buf = alloc::vec![0u8; DIR_SER_MAX];
    buf[0] = DIR_PAT_VER;
    let mut at = 2usize;
    let count = INDEX.lock(|c| {
        let idx = c.borrow();
        let mut n = 0u8;
        for e in idx.iter().take(MAX_PATTERNS) {
            let nb = e.name.as_bytes();
            let nl = nb.len().min(MAX_NAME);
            buf[at..at + 4].copy_from_slice(&e.seq.to_le_bytes());
            buf[at + 4] = e.gen;
            buf[at + 5] = nl as u8;
            buf[at + 6..at + 6 + nl].copy_from_slice(&nb[..nl]);
            at += 6 + nl;
            n += 1;
        }
        n
    });
    buf[1] = count;
    let m = ARENA.lock(|c| c.borrow().to_bytes(&mut buf[at..]));
    if m == 0 {
        return false;
    }
    at += m;
    store_blob(DIR_KEY, &buf[..at])
}

/// Parse a persisted directory. Anything malformed yields an EMPTY store —
/// never a partly-believed one.
fn parse_dir(b: &[u8]) -> (Vec<Entry>, extents::Dir, u32) {
    let empty = || (Vec::new(), extents::Dir::new(ARENA_PAGES as u16), 0u32);
    if b.len() < 2 || b[0] != DIR_PAT_VER {
        return empty();
    }
    let n = b[1] as usize;
    if n > MAX_PATTERNS {
        return empty();
    }
    let mut out: Vec<Entry> = Vec::new();
    let mut at = 2usize;
    for _ in 0..n {
        if at + 6 > b.len() {
            return empty();
        }
        let seq = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        let gen = b[at + 4] & 1; // clamp to {0,1} so `1 - gen` can't underflow
        let nl = b[at + 5] as usize;
        if nl == 0 || nl > MAX_NAME || at + 6 + nl > b.len() {
            return empty();
        }
        let Ok(name) = core::str::from_utf8(&b[at + 6..at + 6 + nl]) else {
            return empty();
        };
        out.push(Entry { seq, gen, name: String::from(name) });
        at += 6 + nl;
    }
    let (dir, dropped) = extents::Dir::from_bytes(&b[at..], ARENA_PAGES as u16);
    (out, dir, dropped)
}

/// An extent's length is bounded by its kind's cap — a stored record that
/// claims more is corrupt (and its `pages()` would over-claim the bitmap).
fn plausible(e: &extents::Extent) -> bool {
    let cap = if e.kind == KIND_SRC { MAX_SOURCE } else { MAX_BC };
    e.kind <= KIND_SRC && e.len as usize <= cap
}

/// Resolve the `storage` partition, map the extent region, and load the
/// directory. Disables the store (REGION = 0) if the partition is absent
/// (old table).
pub fn init() {
    let start = match crate::ota::data_partition("storage") {
        Some((off, len)) if len >= PAT_LEN => off,
        Some((off, len)) => {
            println!("patterns: storage partition too small ({} B at {:#x})", len, off);
            REGION.store(0, Ordering::Relaxed);
            return;
        }
        None => {
            println!("patterns: no storage partition — library disabled (reflash to enable)");
            REGION.store(0, Ordering::Relaxed);
            return;
        }
    };
    if start != PAT_START {
        println!("patterns: storage @ {:#x}, expected {:#x} (csv drift?)", start, PAT_START);
    }
    REGION.store(start, Ordering::Relaxed);
    map_ext(start);
    if raw().is_none() {
        println!("patterns: extent region unmapped — blobs read through the flash controller");
    }

    // Format check: wipe the key area if the on-flash layout isn't ours.
    // There is no migration path by design (#330) — the playground resyncs.
    let fresh = with_store!(start, |af, range, buf| {
        let cache = store_cache();
        let fmt = match map::fetch_item::<u32, &[u8], _>(
            &mut af, range.clone(), cache, buf, &FORMAT_KEY,
        )
        .await
        {
            Ok(Some(b)) if b.len() == 4 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            _ => 0,
        };
        if fmt == FORMAT_VERSION {
            return false;
        }
        println!("patterns: format {} != {}, wiping storage", fmt, FORMAT_VERSION);
        let _ = anf::NorFlash::erase(&mut af, range.start, range.end).await;
        // erased behind sequential-storage's back — the shared cache
        // still describes the old contents
        *store_cache() = PageStateCache::new();
        let ver = FORMAT_VERSION.to_le_bytes();
        let vslice: &[u8] = &ver;
        let c2 = store_cache();
        let _ = map::store_item(&mut af, range.clone(), c2, buf, &FORMAT_KEY, &vslice).await;
        true
    })
    .unwrap_or(true);
    // The extent region is left alone by a wipe: with no directory nothing
    // references it, and every page is erased before it is written again.

    let stored = if fresh { None } else { read_blob(DIR_KEY) };
    let (mut entries, mut dir, mut dropped) = match stored.as_deref() {
        Some(b) => parse_dir(b),
        None => (Vec::new(), extents::Dir::new(ARENA_PAGES as u16), 0),
    };
    // Drop every extent the flash itself does not vouch for: a torn write,
    // a layout change, or a record claiming more than its kind's cap.
    dropped += dir.retain(|e| plausible(e) && ext_hash(e) == Some(e.hash)) as u32;
    // A pattern needs BOTH of its current generation's extents to be
    // readable; otherwise it is gone (the playground re-syncs it).
    let before = entries.len();
    entries.retain(|p| {
        dir.find(p.seq, p.gen, KIND_SRC).is_some() && dir.find(p.seq, p.gen, KIND_BC).is_some()
    });
    let lost = (before - entries.len()) as u32;
    // ...and an extent whose pattern is gone goes with it.
    dropped += dir.retain(|e| entries.iter().any(|p| p.seq == e.seq && p.gen == e.gen)) as u32;

    let mut next = 0u32;
    for e in &entries {
        next = next.max(e.seq.wrapping_add(1));
    }
    NEXT_SEQ.store(next, Ordering::Relaxed);
    let (exts, used) = (dir.count(), dir.used_pages());
    let npat = entries.len();
    INDEX.lock(|c| *c.borrow_mut() = entries);
    ARENA.lock(|c| *c.borrow_mut() = dir);
    println!(
        "patterns: store {} pages, {} patterns, {} extents ({} dropped, {} patterns lost), {} pages used (storage @ {:#x})",
        ARENA_PAGES, npat, exts, dropped, lost, used, start
    );
    if dropped > 0 || lost > 0 {
        persist_dir();
    }
}

/// `GET /api/patterns` → `{"patterns":[{"id","name"},…]}` (from RAM index).
pub fn list_json() -> String {
    let mut out = String::new();
    push_piece(&mut out, "{\"patterns\":[");
    INDEX.lock(|c| {
        for (i, e) in c.borrow().iter().enumerate() {
            if i > 0 {
                push_piece(&mut out, ",");
            }
            push_piece(&mut out, "{\"id\":\"");
            push_piece(&mut out, &id_hex(e.seq));
            push_piece(&mut out, "\",\"name\":\"");
            push_piece(&mut out, &json_escape(&e.name));
            push_piece(&mut out, "\"}");
        }
    });
    push_piece(&mut out, "]}");
    out
}

/// (id, name) of every stored pattern, from the RAM index (for the MQTT
/// pattern select).
pub fn list() -> Vec<(String, String)> {
    INDEX.lock(|c| {
        c.borrow()
            .iter()
            .map(|e| (id_hex(e.seq), e.name.clone()))
            .collect()
    })
}

/// Find a stored pattern's id by exact name (first match).
pub fn id_by_name(name: &str) -> Option<String> {
    INDEX.lock(|c| {
        c.borrow()
            .iter()
            .find(|e| e.name == name)
            .map(|e| id_hex(e.seq))
    })
}

/// Look up a pattern's (seq, gen, name) in the RAM index by id.
#[inline(never)]
fn lookup(id: &str) -> Option<(u32, u8, String)> {
    let seq = seq_of(id)?;
    INDEX.lock(|c| {
        c.borrow()
            .iter()
            .find(|e| e.seq == seq)
            .map(|e| (e.seq, e.gen, e.name.clone()))
    })
}

/// The extent holding a stored pattern's current `kind` blob.
#[inline(never)]
fn extent_of(id: &str, kind: u8) -> Option<extents::Extent> {
    let (seq, gen, _) = lookup(id)?;
    ARENA.lock(|c| c.borrow().find(seq, gen, kind).map(|(_, e)| e))
}

/// Escape a string as JSON *into* an existing buffer — no intermediate
/// allocation (mirrors luxel_core::jsonview::json_escape's rules).
fn escape_into(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '"' => push_piece(out, "\\\""),
            '\\' => push_piece(out, "\\\\"),
            '\n' => push_piece(out, "\\n"),
            '\r' => push_piece(out, "\\r"),
            '\t' => push_piece(out, "\\t"),
            c if (c as u32) < 0x20 => {
                push_piece(out, "\\u");
                push_hex(out, c as u32, 4);
            }
            c => out.push(c),
        }
    }
}

/// `GET /api/patterns/<id>` → `{"id","name","source"}` | None.
///
/// The source is read STRAIGHT OUT OF THE MAPPING and escaped into one
/// pre-sized response buffer — no source Vec, no intermediate copy. The
/// read guard is held across that synchronous copy only, so a concurrent
/// save waits microseconds; if a save or compaction is already running we
/// wait for it (up to ~200 ms) rather than reporting the pattern missing.
pub async fn get_json(id: &str) -> Option<String> {
    let (_, _, name) = lookup(id)?;
    let e = extent_of(id, KIND_SRC)?;
    let guard = MapRead::acquire().await?;
    let len = e.len as usize;
    let mut out = String::new();
    out.try_reserve_exact(len + len / 8 + name.len() + 48).ok()?;
    push_piece(&mut out, "{\"id\":\"");
    push_piece(&mut out, id);
    push_piece(&mut out, "\",\"name\":\"");
    escape_into(&mut out, &name);
    push_piece(&mut out, "\",\"source\":\"");
    match ext_slice(&e) {
        Some(b) => escape_into(&mut out, core::str::from_utf8(b).ok()?),
        None => {
            // flashmap-off: the one path that still copies
            let v = ext_vec(&e)?;
            escape_into(&mut out, core::str::from_utf8(&v).ok()?);
        }
    }
    drop(guard);
    push_piece(&mut out, "\"}");
    Some(out)
}

/// A stored pattern's SOURCE as mapped memory — served in place. The
/// caller must hold a pin on the pattern (the running one always is) or a
/// [MapRead] guard: the bytes are flash the store may otherwise move.
pub fn source_slice(id: &str) -> Option<&'static [u8]> {
    ext_slice(&extent_of(id, KIND_SRC)?)
}

/// A stored pattern's source in a transient Vec — the `flashmap-off`
/// fallback for [source_slice].
pub fn source_vec(id: &str) -> Option<Vec<u8>> {
    ext_vec(&extent_of(id, KIND_SRC)?)
}

/// Read a stored pattern's LXBC bytecode into a transient Vec — the
/// `flashmap-off` fallback for [code_of].
pub fn bytecode_of(id: &str) -> Option<Vec<u8>> {
    ext_vec(&extent_of(id, KIND_BC)?)
}

/// A stored pattern's executable bytes, MAPPED. The engine executes these
/// IN PLACE (`deserialize_lean_static`), so the slice must stay valid for
/// as long as the `Program` does: pin the pattern ([pin_code] /
/// [pin_running] / [pin_prev_from_running]) before taking one and hold the
/// pin until the engine is dropped. The store never writes, moves or frees
/// a pinned extent.
pub fn code_of(id: &str) -> Option<&'static [u8]> {
    ext_slice(&extent_of(id, KIND_BC)?)
}

/// Run `f` over a stored pattern's bytecode: the mapped extent when the
/// region is mapped (no copy), else a transient Vec.
pub fn with_code<R>(id: &str, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
    if let Some(c) = code_of(id) {
        return Some(f(c));
    }
    bytecode_of(id).map(|v| f(&v))
}

/// Does a stored pattern's bytecode still decode on this firmware?
/// None = no such pattern / no bytecode.
#[inline(never)]
pub fn validate_stored(id: &str) -> Option<Result<(), luxel_core::bytecode::BcError>> {
    with_code(id, |bc| luxel_core::bytecode::validate(bc).map(|_| ()))
}

/// (source length, FNV-1a of the source) — the identity hash + Content-Length
/// a library swap stamps. Both are directory fields now: no flash read, no
/// allocation, and the hash is the same one the store verified the extent
/// with (`luxel_core::netin::fnv1a` over the source bytes).
pub fn source_stat(id: &str) -> Option<(usize, u32)> {
    extent_of(id, KIND_SRC).map(|e| (e.len as usize, e.hash))
}

/// A stored pattern's source + bytecode bytes — exact, from the directory.
/// No flash reads, no allocation. For heap pre-flights.
#[inline(never)]
pub fn stored_size_hint(id: &str) -> Option<usize> {
    let src = extent_of(id, KIND_SRC)?.len as usize;
    let bc = extent_of(id, KIND_BC).map(|e| e.len as usize).unwrap_or(0);
    Some(src + bc)
}

/// Human name of a stored pattern.
pub fn name_of(id: &str) -> Option<String> {
    lookup(id).map(|(_, _, name)| name)
}

/// (arena pages in use, arena pages total, stored patterns) for
/// `/api/status`. A total of 0 means the store never came up.
pub fn store_stats() -> (u32, u32, u32) {
    let (used, total) = ARENA.lock(|c| {
        let d = c.borrow();
        (d.used_pages(), d.total_pages())
    });
    let n = INDEX.lock(|c| c.borrow().len());
    (used as u32, total as u32, n as u32)
}

// --- raw writes ---

/// Write one raw region (erase + word-aligned page writes). Every flash op
/// borrows the driver via [crate::ota::with_flash] for just that one op --
/// the driver never leaves the global, so concurrent flash users (asset
/// serving/pushes, an incoming OTA begin, a key-area transaction)
/// interleave between pages instead of reading busy for the whole burst.
/// The previous take-for-the-whole-burst design starved them: with a 5 s
/// playlist churning swaps, asset pushes failed 5/6, /api/ota misreported
/// "update already in progress", and served assets truncated mid-body
/// (measured on the Athom, 2026-08-15). The 1 ms yields between ops keep
/// WiFi airtime AND give waiting HTTP tasks a real window to grab the
/// driver. Best-effort: false when an OTA begins or a key-area transaction
/// leases the driver away mid-burst -- the caller degrades (never a panic).
async fn write_raw(abs: u32, data: &[u8]) -> bool {
    let op_ok = |r: Option<bool>| r == Some(true) && !crate::ota::ota_active();
    let end = abs + (data.len() as u32).div_ceil(PAGE) * PAGE;
    let mut at = abs;
    while at < end {
        let r = crate::ota::with_flash_as(crate::core1::tag::RAW_ERASE, |f| {
            BlockingNorFlash::erase(f, at, at + PAGE).is_ok()
        });
        if !op_ok(r) {
            return false;
        }
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
        at += PAGE;
    }
    // stage through a word-aligned heap buffer (write wants 4-byte units;
    // never a stack buffer -- this runs on the shared main-task stack)
    let mut stage = alloc::vec![0u32; PAGE as usize / 4];
    let mut at = 0usize;
    while at < data.len() {
        let n = (data.len() - at).min(PAGE as usize);
        let words = n.div_ceil(4);
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(stage.as_mut_ptr() as *mut u8, words * 4)
        };
        bytes[words * 4 - 4..].fill(0xFF); // pad the tail word with erased-state bytes
        bytes[..n].copy_from_slice(&data[at..at + n]);
        let r = crate::ota::with_flash_as(crate::core1::tag::RAW_WRITE, |f| {
            BlockingNorFlash::write(f, abs + at as u32, &bytes[..words * 4]).is_ok()
        });
        if !op_ok(r) {
            return false;
        }
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
        at += n;
    }
    true
}

/// Persist the running pattern's source + blob to the AD-HOC read-back
/// slot. Runs on the render task's swap path for ad-hoc pushes ONLY
/// (library swaps -- playlist/activate/resume/MQTT -- already have their
/// bytes as extents and never touch this slot: the flash-wear fix; see
/// main.rs::persist_current_pattern). A brief hitch (a few dozen page
/// erases/writes with yields between them), borrowing the driver per op
/// (see [write_raw]) instead of monopolizing it. Best-effort: false if
/// storage is absent, an OTA is in progress, a key-area transaction leases
/// the driver away mid-burst, or the pattern is implausibly large --
/// read-back degrades until the next swap (documented per caller).
pub async fn store_current(src: &str, bc: &[u8]) -> bool {
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return false;
    }
    if src.len() > CUR_SRC_MAX as usize || bc.len() > CUR_BC_MAX as usize {
        return false;
    }
    if crate::ota::ota_active() {
        return false;
    }
    // ad-hoc swaps only — if this line shows up on every playlist advance,
    // the flash-wear fix regressed (library swaps must not reach here)
    println!("current-pattern: slot write (ad-hoc, src {} B + bc {} B)", src.len(), bc.len());
    // the OTHER bc side: the running engine may be executing the current one
    let side = 1 - CUR_BC_SIDE.load(Ordering::Relaxed);
    let mut ok = write_raw(start + CUR_SRC_OFF, src.as_bytes()).await
        && write_raw(start + cur_bc_off(side), bc).await;
    if ok {
        let mut hdr = [0u8; 16];
        hdr[0..4].copy_from_slice(&CUR_MAGIC.to_le_bytes());
        hdr[4..8].copy_from_slice(&(src.len() as u32).to_le_bytes());
        hdr[8..12].copy_from_slice(&(bc.len() as u32).to_le_bytes());
        hdr[12..16].copy_from_slice(&(side as u32).to_le_bytes());
        ok = write_raw(start + CUR_OFF, &hdr).await;
    }
    if ok {
        // the mapping's cache lines still hold what was there before
        raw_invalidate(CUR_SRC_OFF, src.len());
        raw_invalidate(cur_bc_off(side), bc.len());
        CUR_BC_SIDE.store(side, Ordering::Relaxed);
    }
    ok
}

/// Read the running AD-HOC pattern's whole blob back from flash into a
/// TRANSIENT fallible Vec -- the engine rebuild (pixel-count / map change)
/// needs it contiguous to deserialize, then drops it immediately. `len`
/// comes from the RAM meta. None on a failed reservation or a flash-busy
/// read; the caller keeps the engine paused rather than panicking. Only
/// reached when the region is not mapped.
pub fn read_current_bc(len: usize) -> Option<Vec<u8>> {
    let (_, bc_abs) = current_slot_abs()?;
    let mut out: Vec<u8> = Vec::new();
    if out.try_reserve_exact(len).is_err() {
        println!("readback: {} B bc buffer failed to allocate", len);
        return None;
    }
    out.resize(len, 0);
    let mut at = 0usize;
    while at < len {
        let n = (len - at).min(PAGE as usize);
        if !crate::assets::read_chunk(bc_abs + at as u32, &mut out[at..at + n]) {
            println!("readback: bc read failed at {}/{} B (flash busy?)", at, len);
            return None;
        }
        at += n;
    }
    Some(out)
}

// --- save / delete ---

fn err_json(msg: &str) -> String {
    let mut out = String::new();
    push_piece(&mut out, "{\"ok\":false,\"error\":\"");
    push_piece(&mut out, msg);
    push_piece(&mut out, "\"}");
    out
}

/// Reserve an extent's pages in the RAM directory. Nothing on flash yet
/// and nothing reachable — the new generation only becomes findable when
/// the pattern index moves to it, after both blobs verify.
fn reserve(seq: u32, gen: u8, kind: u8, len: usize, hash: u32) -> Option<extents::Extent> {
    ARENA.lock(|c| {
        let mut d = c.borrow_mut();
        let start = d.first_fit(extents::pages_for(len as u32))?;
        let e = extents::Extent { seq, gen, kind, start, len: len as u32, hash };
        d.insert(e).map(|_| e)
    })
}

fn release(e: &extents::Extent) {
    ARENA.lock(|c| {
        let mut d = c.borrow_mut();
        if let Some((i, _)) = d.find(e.seq, e.gen, e.kind) {
            d.remove(i);
        }
    });
}

/// Write one blob into its reserved extent, then prove it: invalidate the
/// cache lines the write made stale and re-hash the bytes as the readers
/// will see them.
async fn write_extent(region: u32, e: &extents::Extent, data: &[u8]) -> bool {
    let rel = arena_off(e.start);
    if !write_raw(region + rel, data).await {
        return false;
    }
    raw_invalidate(rel, data.len());
    ext_hash(e) == Some(e.hash)
}

/// `POST /api/patterns` (LXP1 envelope) → `{"ok":true,"id"}`.
/// Upserts by name. Caller decode-validates the bytecode first.
///
/// One extent write per blob and one directory item: the source and the
/// bytecode are written once each, verified through the mapping, and only
/// then does the directory name them. A power cut before that leaves the
/// previous generation whole; a cut after it leaves at most a superseded
/// extent, which the next save sweeps.
pub async fn save(name: &str, source: &str, bc: &[u8]) -> String {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_NAME {
        return err_json("name must be 1..=64 bytes");
    }
    if source.is_empty() || source.len() > MAX_SOURCE {
        let mut out = String::new();
        push_piece(&mut out, "{\"ok\":false,\"error\":\"this pattern's source is too large for the on-device library (");
        push_u32(&mut out, (source.len() / 1024) as u32);
        push_piece(&mut out, " KB; the device stores up to ");
        push_u32(&mut out, (MAX_SOURCE / 1024) as u32);
        push_piece(&mut out, " KB)\"}");
        return out;
    }
    if bc.is_empty() || bc.len() > MAX_BC {
        let mut out = String::new();
        push_piece(&mut out, "{\"ok\":false,\"error\":\"this pattern's compiled code is too large for the on-device library (");
        push_u32(&mut out, (bc.len() / 1024) as u32);
        push_piece(&mut out, " KB; the device stores up to ");
        push_u32(&mut out, (MAX_BC / 1024) as u32);
        push_piece(&mut out, " KB)\"}");
        return out;
    }
    let region = REGION.load(Ordering::Relaxed);
    if region == 0 {
        return err_json("pattern storage unavailable (device needs reflash)");
    }
    if crate::ota::ota_active() {
        return err_json("an update is in progress — try again in a moment");
    }
    let Some(_guard) = StoreWrite::acquire().await else {
        return err_json("the store is busy — try again in a moment");
    };
    if sweep_unreferenced() > 0 {
        persist_dir();
    }

    // Decide seq + generation under the index lock.
    enum Plan {
        Update { seq: u32, new_gen: u8 },
        New { seq: u32 },
        Full,
    }
    let plan = INDEX.lock(|c| {
        let idx = c.borrow();
        if let Some(e) = idx.iter().find(|e| e.name == name) {
            Plan::Update { seq: e.seq, new_gen: 1 - e.gen }
        } else if idx.len() >= MAX_PATTERNS {
            Plan::Full
        } else {
            Plan::New { seq: NEXT_SEQ.load(Ordering::Relaxed) }
        }
    });
    let (seq, new_gen, is_new) = match plan {
        Plan::Full => {
            let mut out = String::new();
            push_piece(&mut out, "{\"ok\":false,\"error\":\"the device library is full (");
            push_u32(&mut out, MAX_PATTERNS as u32);
            push_piece(&mut out, " patterns) — delete one first\"}");
            return out;
        }
        Plan::New { seq } => (seq, 0u8, true),
        Plan::Update { seq, new_gen } => (seq, new_gen, false),
    };

    // Reserve BOTH extents before writing either: a save that can only
    // place half of itself must not leave a written orphan behind.
    let need = extents::pages_for(source.len() as u32) + extents::pages_for(bc.len() as u32);
    let src_hash = fnv1a(source.as_bytes());
    let bc_hash = fnv1a(bc);
    let mut src_e = reserve(seq, new_gen, KIND_SRC, source.len(), src_hash);
    let mut bc_e = match src_e {
        Some(_) => reserve(seq, new_gen, KIND_BC, bc.len(), bc_hash),
        None => None,
    };
    if bc_e.is_none() {
        // No room as things stand. Compaction is a user-action-only cost
        // (never on the activation path — the wear rule), so this is where
        // it happens: slide unpinned extents toward page 0 until a run
        // opens, then retry the reservation.
        if let Some(e) = src_e.take() {
            release(&e);
        }
        if !compact_for(need, region).await {
            let (hole, used, total) = ARENA.lock(|c| {
                let d = c.borrow();
                (d.largest_hole(), d.used_pages(), d.total_pages())
            });
            println!(
                "patterns: store full — {} needs {} pages, {}/{} used, best hole {}",
                name, need, used, total, hole
            );
            return err_json(
                "the device's pattern storage is full — delete some patterns and retry",
            );
        }
        src_e = reserve(seq, new_gen, KIND_SRC, source.len(), src_hash);
        bc_e = match src_e {
            Some(_) => reserve(seq, new_gen, KIND_BC, bc.len(), bc_hash),
            None => None,
        };
    }
    let (Some(src_e), Some(bc_e)) = (src_e, bc_e) else {
        if let Some(e) = src_e {
            release(&e);
        }
        return err_json("the device's pattern storage is full — delete some patterns and retry");
    };

    // Write + verify both blobs. Nothing published yet, so a failure here
    // just gives the pages back.
    let written = write_extent(region, &src_e, source.as_bytes()).await
        && write_extent(region, &bc_e, bc).await;
    if !written {
        release(&src_e);
        release(&bc_e);
        println!("patterns: extent write of {} failed (pages {} + {})", name, src_e.start, bc_e.start);
        return err_json(
            "couldn't write the pattern to flash — the store may be busy; try again in a moment",
        );
    }

    // Publish: the index moves to the new generation and the directory
    // item lands. THIS is the commit.
    INDEX.lock(|c| {
        let mut idx = c.borrow_mut();
        if let Some(e) = idx.iter_mut().find(|e| e.seq == seq) {
            e.gen = new_gen;
            e.name = String::from(name);
        } else {
            idx.push(Entry { seq, gen: new_gen, name: String::from(name) });
        }
    });
    if !persist_dir() {
        // The bytes are on flash and the RAM directory names them, so this
        // session is correct; the next boot re-syncs from the playground.
        println!("patterns: directory write failed — {} lives this session only", name);
    }
    if is_new {
        NEXT_SEQ.store(seq.wrapping_add(1), Ordering::Relaxed);
    }

    // The superseded generation may go now — unless an engine is still
    // executing (or streaming) from it, in which case it stays in the
    // directory as a stale generation and the next save sweeps it.
    if !is_new && !pinned(seq) && free_other_gens(seq, new_gen) > 0 {
        persist_dir();
    }
    let mut out = String::new();
    push_piece(&mut out, "{\"ok\":true,\"id\":\"");
    push_piece(&mut out, &id_hex(seq));
    push_piece(&mut out, "\"}");
    out
}

/// Drop every extent of `seq` that is NOT generation `keep_gen`.
fn free_other_gens(seq: u32, keep_gen: u8) -> usize {
    ARENA.lock(|c| {
        let mut d = c.borrow_mut();
        let mut n = 0;
        while let Some((i, _)) = d.find_other_gen(seq, keep_gen) {
            d.remove(i);
            n += 1;
        }
        n
    })
}

/// Reclaim every extent the index no longer references — a deleted
/// pattern's, or the stale generation a re-save of the RUNNING pattern
/// left behind — except any an engine is still executing from. Runs at the
/// head of a save (a user action), so playlist churn never pays for it.
fn sweep_unreferenced() -> usize {
    let (pin_buf, pin_n) = pins();
    let held = &pin_buf[..pin_n];
    let live: Vec<(u32, u8)> =
        INDEX.lock(|c| c.borrow().iter().map(|e| (e.seq, e.gen)).collect());
    ARENA.lock(|c| {
        c.borrow_mut()
            .retain(|e| held.contains(&e.seq) || live.contains(&(e.seq, e.gen)))
    })
}

/// `DELETE /api/patterns/<id>` → `{"ok":true}` | `{"ok":false,…}`.
///
/// A pattern an engine is still executing from KEEPS its extents: dropping
/// them here would hand its pages to the next save, which would erase them
/// under the live VM. It is gone from the index, so the next save's sweep
/// reclaims them once the engine lets go.
pub async fn delete(id: &str) -> String {
    let Some((seq, _, _)) = lookup(id) else {
        return err_json("no such pattern");
    };
    let Some(_guard) = StoreWrite::acquire().await else {
        return err_json("the store is busy — try again in a moment");
    };
    INDEX.lock(|c| c.borrow_mut().retain(|e| e.seq != seq));
    if pinned(seq) {
        println!("patterns: extents of {} kept — an engine is executing them", id);
    } else {
        ARENA.lock(|c| c.borrow_mut().remove_seq(seq));
    }
    if !persist_dir() {
        return err_json("flash error");
    }
    String::from("{\"ok\":true}")
}

// --- the engine pin set (Gitea #260) ---
//
// The engine executes a library pattern's LXBC **in place** from its
// extent: `deserialize_lean_static` makes the `Program`'s code and constant
// pool a `&'static [u32]` INTO THE MAPPING, not a heap copy. So an extent an
// engine still holds must never be written, moved (compaction) or freed
// while it holds it — the render task would be executing erased flash, and
// on a dual-core board the store runs on the OTHER core, in parallel. The
// same pin covers the pattern's SOURCE extent, which the HTTP layer streams
// out of the mapping for the running pattern's read-back.
//
// `shared::get_current_pattern_id()` is not enough on its own: it names ONE
// pattern, and there are two windows where an engine is executing something
// else.
//   * a swap decodes the INCOMING pattern's mapped bytes before it becomes
//     the current pattern (`code_of` .. `set_current_pattern_id`), and
//   * a CROSSFADE keeps the outgoing engine alive as the blend source for
//     up to several seconds after the current pattern has moved on
//     (main.rs' `prev`).
// A compaction in either window is a use-after-free, so the render task
// publishes the seqs it is actually executing here. Slots:
//   [0] `pin_code`      — what a decode is about to borrow
//   [1] `pin_running`   — what the LIVE engine borrows
//   [2] `pin_prev`      — what the crossfade's OUTGOING engine borrows
// The current pattern id stays in the set as well: belt and braces, so a
// path that forgets to pin is still covered for the running pattern. Pins
// are conservative by construction (a stale one wastes arena pages until
// the next swap overwrites it; it can never free something live).
//
// An UNPINNED pattern's mapped bytes are covered by the [MapRead] guard
// instead — see [BUSY].
static PINS: BlockingMutex<CriticalSectionRawMutex, Cell<[Option<u32>; 3]>> =
    BlockingMutex::new(Cell::new([None; 3]));

fn set_pin(slot: usize, seq: Option<u32>) {
    PINS.lock(|c| {
        let mut p = c.get();
        p[slot] = seq;
        c.set(p);
    });
}

/// Slot 0: the pattern a decode is about to borrow. Call it BEFORE
/// [code_of] on a swap — until it returns, nothing stops a save on the
/// other core from compacting that extent out from under the decode.
pub fn pin_code(id: &str) {
    set_pin(0, seq_of(id));
}

/// Slot 1: the pattern the live engine borrows. Call it when the new engine
/// is installed, with the id its code came from (the empty id — an ad-hoc
/// push — clears it: those Programs own their words).
pub fn pin_running(id: &str) {
    set_pin(1, seq_of(id));
}

/// Slot 2 := slot 1: the engine that was live becomes the crossfade's
/// outgoing blend source and keeps executing its extent. Call it at the
/// same moment main.rs does `prev = engine.take()`.
pub fn pin_prev_from_running() {
    PINS.lock(|c| {
        let mut p = c.get();
        p[2] = p[1];
        c.set(p);
    });
}

/// Slot 2 released — the outgoing engine has been dropped.
pub fn unpin_prev() {
    set_pin(2, None);
}

/// Every seq an engine may be executing from, as a slice-able buffer.
fn pins() -> ([u32; 4], usize) {
    let mut out = [0u32; 4];
    let mut n = 0;
    let mut push = |s: Option<u32>| {
        if let Some(s) = s {
            if !out[..n].contains(&s) {
                out[n] = s;
                n += 1;
            }
        }
    };
    push(seq_of(&crate::shared::get_current_pattern_id()));
    for p in PINS.lock(|c| c.get()) {
        push(p);
    }
    (out, n)
}

/// Is this pattern's extent held by an engine right now?
fn pinned(seq: u32) -> bool {
    let (buf, n) = pins();
    buf[..n].contains(&seq)
}

// --- compaction ---

/// One compaction step: slide an extent down to `mv.to` so the free pages
/// coalesce. The extent is UNPUBLISHED first (which is also what frees the
/// destination when the two ranges overlap), so a power cut anywhere in
/// here costs exactly this one extent — never a torn one handed to a
/// reader or the engine. Pages are copied one at a time, each an
/// `ota::with_flash` erase + write with yields between (the same door and
/// the same fence stall as every other write here), so the render task
/// never waits on a whole extent.
#[inline(never)]
async fn compact_move(mv: extents::Move, region: u32) -> bool {
    let Some(e) = ARENA.lock(|c| c.borrow().get(mv.idx).copied()) else {
        return false;
    };
    if pinned(e.seq) {
        return false; // an engine is executing it; it moves at the next swap
    }
    ARENA.lock(|c| {
        c.borrow_mut().remove(mv.idx);
    });
    persist_dir();
    let (src, dst) = (arena_off(e.start), arena_off(mv.to));
    let mut buf = alloc::vec![0u8; PAGE as usize];
    let mut at = 0u32;
    let mut ok = true;
    while at < e.len {
        let n = (e.len - at).min(PAGE) as usize;
        // Ascending page order with dst strictly below src: page k is read
        // before the erase of destination page k could reach it, so an
        // overlapping downward move is safe one page at a time (memmove).
        ok = crate::assets::read_chunk(region + src + at, &mut buf[..n])
            && write_raw(region + dst + at, &buf[..n]).await;
        if !ok {
            break;
        }
        at += PAGE;
    }
    drop(buf);
    let moved = extents::Extent { start: mv.to, ..e };
    if ok {
        raw_invalidate(dst, e.len as usize);
        ok = ext_hash(&moved) == Some(e.hash);
    }
    if !ok {
        println!("patterns: compaction dropped seq {} (page {} -> {})", e.seq, e.start, mv.to);
        return false;
    }
    let placed = ARENA.lock(|c| c.borrow_mut().insert(moved).is_some());
    persist_dir();
    placed
}

/// Slide unpinned extents toward page 0 until a run of `need` pages opens.
/// Returns false when packing could not produce one (nothing is erased in
/// that case — the "is it worth it?" test runs first). Only a SAVE calls
/// this: an activation never compacts, so playlist churn stays wear-free.
async fn compact_for(need: u16, region: u32) -> bool {
    let (pin_buf, pin_n) = pins();
    let held = &pin_buf[..pin_n];
    if ARENA.lock(|c| c.borrow().compacted_free_run(held)) < need {
        return false;
    }
    println!("patterns: compacting the store for {} pages", need);
    for _ in 0..2 * extents::MAX_EXTENTS {
        if ARENA.lock(|c| c.borrow().first_fit(need)).is_some() {
            return true;
        }
        let Some(mv) = ARENA.lock(|c| c.borrow().next_move(held)) else {
            break;
        };
        // A move un-publishes the extent before copying it, so a power cut
        // mid-pass costs exactly the blob in flight — and, since there is
        // no second copy any more, the pattern that owned it. Boot drops a
        // record whose extents are not both there; the playground re-syncs
        // that one pattern. Bounded, and only ever on a save that had no
        // contiguous room left.
        if !compact_move(mv, region).await {
            break;
        }
    }
    ARENA.lock(|c| c.borrow().first_fit(need)).is_some()
}

/// The running pattern's executable bytes as mapped memory, per the VM
/// consumer contract (docs/research/flash-mmap.md): rodata for the built-in
/// default (the bootloader's own mapping), the ad-hoc slot side the last
/// store_current wrote, or the library pattern's extent. None = read it
/// through a Vec (read_current_bc / bytecode_of) — the fallback when the
/// mapping is absent.
pub fn current_code() -> Option<&'static [u8]> {
    use crate::shared::BcLoc;
    match crate::shared::current_bc() {
        BcLoc::Default(b) => Some(b),
        BcLoc::Flash(len) => current_slot_code(len),
        BcLoc::Library(_) => code_of(&crate::shared::get_current_pattern_id()),
        BcLoc::Gone => None,
    }
}
