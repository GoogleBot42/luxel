//! On-device pattern library — the firmware half of the `/api/patterns`
//! CRUD contract the native mirror (crates/luxel-cli/src/serve.rs) and the
//! playground already speak.
//!
//! # Storage: a packed file log + a small key area (Gitea #340)
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
//! | `0x00000` | `0x210000` | 128 KiB (32 pages) | **key area** — a `sequential-storage` map: format key, playlist, playstate, pixel map, resume, palette |
//! | `0x20000` | `0x230000` | 896 KiB (224 pages) | **extent region** — mapped read-only through the cache MMU at boot |
//!
//! Everything large and immutable lives in the extent region, contiguous
//! and 4-byte aligned, which is the one property XIP needs
//! (docs/research/flash-mmap.md): the VM executes a pattern's LXBC in place
//! and the HTTP layer serves its source straight out of the mapping.
//! Within the region:
//!
//! | rel | size | what |
//! |---|---:|---|
//! | `0x20000` | 4 KiB | ad-hoc header page: magic, src/bc lengths, bc side |
//! | `0x21000` | 32 KiB | ad-hoc (live-coding) source |
//! | `0x29000` | 2 × 64 KiB | ad-hoc bytecode, TWO sides — a push writes the side the running engine is not executing from |
//! | `0x49000` | 732 KiB | the **file log**, 183 pages |
//!
//! # The file log
//!
//! Gitea #330 gave every blob a whole number of 4 KiB erase pages and put
//! the directory in ONE `sequential-storage` item, whose one-page cap is
//! what held the store to 32 patterns. #340 replaced both: a file is now
//! **exactly the bytes it needs** (rounded only to 4) and the log is
//! **self-describing**, so there is no directory anywhere and the pattern
//! count is bounded by bytes.
//!
//! One file = one pattern: a 48-byte header, the name, the source text and
//! the LXBC, packed back to back, and the next file starts immediately
//! after. `patlog.rs` owns that format — the header layout, the boot scan,
//! append planning and compaction placement — and is host-tested against a
//! NOR simulator that cuts power at every write boundary
//! (`tools/patlog-check`). Everything here is the flash I/O half: the
//! fenced door, the mapping, the pin set and the RAM index.
//!
//! Measured on the real library (305 patterns, median source 2,853 B):
//! **32 patterns in 328 KiB page-granular → 119 patterns in 722 KiB
//! packed**, in the same 183-page log.
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
//!
//! Patterns are NOT in here any more — not even their directory.
//!
//! # Format changes
//!
//! [FORMAT_VERSION] wipes the key area on mismatch and
//! [crate::patlog::VER] retires every older record in the log. There is no
//! migration code: the playground re-syncs the library (Jeremy's decision,
//! #330).
//!
//! # Flash access
//!
//! sequential-storage is async; esp-storage's FlashStorage is blocking (see
//! the AsyncFlash adapter). Each key-area transaction *leases* the driver
//! out of the OTA module (never holding its critical-section mutex across
//! erases) and drives the ops with `block_on` (the adapter never truly
//! pends). Log writes go through [crate::ota::with_flash] one op at a
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

use crate::patlog::{self, Arena, Rec};

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

/// Erase-page size — the reclaim unit, and the map's page.
const PAGE: u32 = 4096;

/// Flash range `sequential-storage` manages: the KEY AREA, the first
/// 128 KiB / 32 pages of the partition. Small, hot, power-loss-safe keys
/// only (see the module docs' table) — a few KB of live data, so the log
/// has plenty of GC headroom, and every op's page scan is 4× cheaper than
/// the old 512 KiB range.
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
/// Next write stamp (monotonic). The highest stamp for a seq is its current
/// record — the tiebreak a power cut between "commit the new file" and
/// "mark the old one dead" leaves behind.
static NEXT_STAMP: AtomicU32 = AtomicU32::new(1);
const ID_MASK: u32 = 0x5eed_1e55;

const MAX_NAME: usize = patlog::MAX_NAME;
/// Largest source text the store accepts. A file is exact-sized now, so
/// this is purely a sanity cap; the 16 KiB HTTP request buffer bounds a
/// POST well below it anyway.
pub const MAX_SOURCE: usize = patlog::MAX_SOURCE as usize; // 32 KiB
/// Largest LXBC the store accepts. LXBC can run larger than its source, so
/// it gets the bigger cap. Both sit comfortably past what the device's heap
/// can actually run — the RAM floor, not flash, is the real ceiling.
pub const MAX_BC: usize = patlog::MAX_BC as usize; // 40 KiB
/// How many files the RAM index will hold. NOT a format limit and not what
/// the log can store — it is the heap guard on `Vec<Rec>` (32 B each) for a
/// log someone filled with tiny files. The real library packs 119 patterns
/// into the 183-page log; past that, bytes run out first.
const MAX_RECS: usize = 192;
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
/// v6: the directory item is gone too — the extent region is a packed,
/// self-describing file log (#340). Patterns leave the key area entirely.
const FORMAT_VERSION: u32 = 6;
const FORMAT_KEY: u32 = 0x7FFF_FFFF;

fn id_hex(seq: u32) -> String {
    let mut out = String::new();
    push_hex(&mut out, seq ^ ID_MASK, 8);
    out
}
fn seq_of(id: &str) -> Option<u32> {
    u32::from_str_radix(id, 16).ok().map(|v| v ^ ID_MASK)
}

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

// --- small reserved-key blobs (playlist definition + playback state) ---
// These live in the key area alongside FORMAT_KEY. Each must fit one flash
// page.
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
// already in the log; only an ad-hoc push (/api/code, sync adoption) writes
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
//   LOG_OFF          the packed file log: LOG_LEN bytes of exact-sized,
//                    4-byte-aligned, self-describing pattern files
//                    (patlog.rs)
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
/// The file log: everything left in the extent region after the ad-hoc
/// slot. 0x49000..0x100000 = 732 KiB = 183 pages.
const LOG_OFF: u32 = CUR_BC_OFF + 2 * CUR_BC_MAX;
const LOG_LEN: u32 = EXT_OFF + EXT_LEN - LOG_OFF;
const _: () = assert!(CUR_SRC_MAX as usize >= MAX_SOURCE);
const _: () = assert!(CUR_BC_MAX as usize >= MAX_BC);
const _: () = assert!(LOG_OFF % PAGE == 0 && LOG_LEN % PAGE == 0);
const _: () = assert!(LOG_LEN as usize >= MAX_SOURCE + MAX_BC);
const _: () = assert!(PAGE == patlog::PAGE);

fn cur_bc_off(side: u8) -> u32 {
    CUR_BC_OFF + side as u32 * CUR_BC_MAX
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

// --- reading the log ---

/// `len` bytes at log-relative `rel` as MAPPED memory — the zero-copy path.
/// None when the region is not mapped (`flashmap-off` / refused self-check).
fn log_slice(rel: u32, len: u32) -> Option<&'static [u8]> {
    raw_slice(LOG_OFF + rel, len as usize)
}

/// The whole log as an [Arena] over the mapping — a view, never a copy.
struct MapLog(&'static [u8]);
impl Arena for MapLog {
    fn len(&self) -> u32 {
        self.0.len() as u32
    }
    fn view(&mut self, off: u32, want: usize) -> Option<&[u8]> {
        let at = off as usize;
        if at >= self.0.len() {
            return None;
        }
        Some(&self.0[at..(at + want).min(self.0.len())])
    }
}

/// The `flashmap-off` [Arena]: one page of RAM, refilled through the flash
/// controller. Every request is served whole (a header + its name never
/// straddles the buffer), which is what keeps the scan's parse honest when
/// the mapping is gone.
struct NorLog {
    base: u32,
    buf: Vec<u8>,
    at: u32,
    n: u32,
}

impl NorLog {
    fn new(base: u32) -> NorLog {
        NorLog { base, buf: alloc::vec![0u8; PAGE as usize], at: 0, n: 0 }
    }
}

impl Arena for NorLog {
    fn len(&self) -> u32 {
        LOG_LEN
    }
    fn view(&mut self, off: u32, want: usize) -> Option<&[u8]> {
        if off >= LOG_LEN {
            return None;
        }
        let need = (want as u32).min(LOG_LEN - off);
        if off < self.at || off + need > self.at + self.n {
            let n = PAGE.min(LOG_LEN - off);
            if !crate::assets::read_chunk(self.base + off, &mut self.buf[..n as usize]) {
                self.n = 0;
                return None;
            }
            self.at = off;
            self.n = n;
        }
        let s = (off - self.at) as usize;
        Some(&self.buf[s..self.n as usize])
    }
}

/// Run `f` over the log. Through the mapping when there is one, else
/// through a one-page read buffer. None = the store never came up.
fn with_log<R>(f: impl FnOnce(&mut dyn Arena) -> R) -> Option<R> {
    if let Some(b) = raw() {
        let s = b.get((LOG_OFF - EXT_OFF) as usize..)?;
        let mut a = MapLog(s);
        return Some(f(&mut a));
    }
    let region = REGION.load(Ordering::Relaxed);
    if region == 0 {
        return None;
    }
    let mut a = NorLog::new(region + LOG_OFF);
    Some(f(&mut a))
}

// --- the RAM index ---

/// One live [Rec] per stored pattern, ascending by log offset. A record is
/// 32 bytes and carries no name: names are read back out of the mapping on
/// demand, so 192 patterns cost 6 KB of heap rather than 20.
static INDEX: BlockingMutex<CriticalSectionRawMutex, RefCell<Vec<Rec>>> =
    BlockingMutex::new(RefCell::new(Vec::new()));
/// Where the next file goes.
static CURSOR: AtomicU32 = AtomicU32::new(0);
/// Where the last file placed BELOW the cursor ended — the free-space hint
/// [patlog::place_free] packs against, so a hole under a pinned file fills
/// as densely as the log proper instead of costing a page per record
/// (Gitea #388). Zero means "no hole is open"; it is only ever a hint, and
/// `place_free` re-checks the flash before trusting it.
static FREE_HINT: AtomicU32 = AtomicU32::new(0);
/// Bytes held by records the index no longer points at — superseded
/// generations, deletes, and anything a cut operation left behind. This is
/// what a compaction would give back.
static DEAD_BYTES: AtomicU32 = AtomicU32::new(0);
/// The log holds more distinct patterns than [MAX_RECS], so the RAM index
/// is INCOMPLETE. Reads still work for what is indexed, but every mutation
/// refuses: a compaction would rewrite the log from an index that does not
/// name every live file, and drop the ones it cannot see.
static OVERFULL: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);


/// Store concurrency: `0` idle, [WRITER] a mutating transaction in flight,
/// anything else a count of mapped readers.
///
/// A mutation (save, delete, compaction) moves and erases bytes an unpinned
/// reader may be looking at, so the two exclude each other. Pinned reads
/// (the RUNNING pattern — see the pin set) need no guard at all: nothing
/// ever moves or erases a pinned file. Readers hold the guard only across
/// SYNCHRONOUS copies (microseconds), so a writer's retry converges.
static BUSY: AtomicU32 = AtomicU32::new(0);
const WRITER: u32 = u32::MAX;
/// The critical section that makes the test-and-set on [BUSY] atomic across
/// both cores. Its own lock, not the index's: the guarded bodies take the
/// index too.
static BUSY_LOCK: BlockingMutex<CriticalSectionRawMutex, ()> = BlockingMutex::new(());

struct StoreWrite;
struct MapRead;

impl StoreWrite {
    /// None when a mutation or an unpinned mapped read is in flight (the
    /// caller retries or degrades; it never blocks).
    fn take() -> Option<StoreWrite> {
        BUSY_LOCK.lock(|_| {
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
        BUSY_LOCK.lock(|_| BUSY.store(0, Ordering::Relaxed));
    }
}

impl MapRead {
    fn take() -> Option<MapRead> {
        BUSY_LOCK.lock(|_| {
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
        BUSY_LOCK.lock(|_| {
            let n = BUSY.load(Ordering::Relaxed);
            BUSY.store(n.saturating_sub(1), Ordering::Relaxed);
        });
    }
}

/// Order records by log offset. An insertion sort, not `sort_unstable`:
/// the lists are short (≤ [MAX_RECS]) and nearly always already ordered,
/// and each `sort_unstable_by_key` call site would monomorphize a whole
/// pdqsort into an image that has to fit the 1 MiB OTA slot.
fn sort_by_off(v: &mut [Rec]) {
    for i in 1..v.len() {
        let mut k = i;
        while k > 0 && v[k - 1].off > v[k].off {
            v.swap(k - 1, k);
            k -= 1;
        }
    }
}

/// Walk the log and rebuild every piece of RAM state from it: the index,
/// the cursor, the dead-byte count, the seq and stamp counters.
///
/// This is the ONLY thing that populates the index — boot and the end of a
/// compaction both call it — so what the device believes is always exactly
/// what the flash says, re-hashed. Returns the scan's own statistics.
fn reload() -> patlog::Scan {
    // Dedup as we walk, so nothing but the live set is ever resident: one
    // record per seq, the highest stamp winning and, on a tie, the lowest
    // offset (a compaction's new copy sits below the stale one it has not
    // swept yet). Everything else is dead weight the next compaction drops.
    let mut live: Vec<Rec> = Vec::new();
    let mut seq = 0u32;
    let mut stamp = 1u32;
    let mut over = false;
    let stats = with_log(|a| {
        patlog::scan(a, &mut |r: &Rec, _: &[u8]| {
            seq = seq.max(r.seq.wrapping_add(1));
            stamp = stamp.max(r.stamp.wrapping_add(1));
            if r.dead {
                return;
            }
            match live.iter_mut().find(|l| l.seq == r.seq) {
                Some(slot) => {
                    if r.stamp > slot.stamp {
                        *slot = *r;
                    }
                }
                None => {
                    if live.len() >= MAX_RECS {
                        over = true;
                    } else {
                        live.push(*r);
                    }
                }
            }
        })
    })
    .unwrap_or_default();
    sort_by_off(&mut live);

    let used: u32 = live.iter().map(|r| r.size()).sum();
    NEXT_SEQ.store(seq, Ordering::Relaxed);
    NEXT_STAMP.store(stamp, Ordering::Relaxed);
    CURSOR.store(stats.cursor, Ordering::Relaxed);
    // Everything below the cursor that is not one of the files the index
    // points at — superseded generations, deletes, torn headers, and the
    // gaps a frozen page forces a repack to leave. Deriving it from the
    // cursor rather than from the sum of accepted records is what makes it
    // "what a compaction would give back": bytes no record claims are
    // reclaimable too, and after a compaction with nothing pinned this is
    // exactly 0 (Gitea #379).
    DEAD_BYTES.store(stats.cursor.saturating_sub(used), Ordering::Relaxed);
    // The log moved under it; `place_free` starts its search over.
    FREE_HINT.store(0, Ordering::Relaxed);
    OVERFULL.store(over, Ordering::Relaxed);
    INDEX.lock(|c| *c.borrow_mut() = live);
    stats
}

/// Resolve the `storage` partition, map the extent region, and walk the
/// log. Disables the store (REGION = 0) if the partition is absent (old
/// table).
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
        println!("patterns: extent region unmapped — files read through the flash controller");
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
    // The log needs no wipe of its own: `patlog::VER` retires every record
    // an older firmware wrote, so the scan below simply finds nothing and
    // the first append erases the pages it lands on.
    let _ = fresh;

    let s = reload();
    let (npat, dead) = (
        INDEX.lock(|c| c.borrow().len()),
        DEAD_BYTES.load(Ordering::Relaxed),
    );
    println!(
        "patterns: log {} B, {} patterns, {} B used, {} B reclaimable, {} files ({} torn, {} resyncs), cursor {} (storage @ {:#x})",
        LOG_LEN, npat, s.live, dead, s.recs, s.torn, s.resync, s.cursor, start
    );
    if OVERFULL.load(Ordering::Relaxed) {
        println!(
            "patterns: more than {} patterns in the log — the index is incomplete and the store is READ-ONLY until some are deleted",
            MAX_RECS
        );
    }
}

// --- reads ---

/// The live record for an API id.
fn rec_of(id: &str) -> Option<Rec> {
    let seq = seq_of(id)?;
    INDEX.lock(|c| c.borrow().iter().find(|r| r.seq == seq).copied())
}

/// A file's name, read back out of the log (never held in RAM).
fn rec_name(r: &Rec) -> Option<String> {
    let n = r.name_len as u32;
    if let Some(b) = log_slice(r.name_off(), n) {
        return core::str::from_utf8(b).ok().map(String::from);
    }
    let region = REGION.load(Ordering::Relaxed);
    if region == 0 {
        return None;
    }
    let mut buf = alloc::vec![0u8; n as usize];
    if !crate::assets::read_chunk(region + LOG_OFF + r.name_off(), &mut buf) {
        return None;
    }
    String::from_utf8(buf).ok()
}

fn rec_by_name(name: &str) -> Option<Rec> {
    let recs: Vec<Rec> = INDEX.lock(|c| c.borrow().clone());
    recs.into_iter().find(|r| rec_name(r).as_deref() == Some(name))
}

/// A file's payload as MAPPED memory — the zero-copy path.
fn payload_slice(off: u32, len: u32) -> Option<&'static [u8]> {
    log_slice(off, len)
}

/// A file's payload in a transient fallible Vec — the `flashmap-off`
/// fallback, and the only path that ever copies a stored blob.
fn payload_vec(off: u32, len: u32) -> Option<Vec<u8>> {
    let len = len as usize;
    let mut out: Vec<u8> = Vec::new();
    if out.try_reserve_exact(len).is_err() {
        println!("store: {} B file buffer failed to allocate", len);
        return None;
    }
    if let Some(b) = payload_slice(off as u32, len as u32) {
        out.extend_from_slice(b);
        return Some(out);
    }
    let region = REGION.load(Ordering::Relaxed);
    if region == 0 {
        return None;
    }
    out.resize(len, 0);
    let base = region + LOG_OFF + off;
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

/// `GET /api/patterns` → `{"patterns":[{"id","name"},…]}` (from the RAM
/// index; names come out of the mapping).
pub async fn list_json() -> String {
    let recs: Vec<Rec> = INDEX.lock(|c| c.borrow().clone());
    let mut out = String::new();
    push_piece(&mut out, "{\"patterns\":[");
    let mut first = true;
    for (i, r) in recs.iter().enumerate() {
        // #395: `rec_name` allocates per record and, without the mapping,
        // takes a flash-controller op each. The store holds up to MAX_RECS
        // (192), so cap the uninterruptible run at 16 records.
        if i > 0 && i % 16 == 0 {
            embassy_futures::yield_now().await;
        }
        let Some(name) = rec_name(r) else { continue };
        if !first {
            push_piece(&mut out, ",");
        }
        first = false;
        push_piece(&mut out, "{\"id\":\"");
        push_piece(&mut out, &id_hex(r.seq));
        push_piece(&mut out, "\",\"name\":\"");
        push_piece(&mut out, &json_escape(&name));
        push_piece(&mut out, "\"}");
    }
    push_piece(&mut out, "]}");
    out
}

/// (id, name) of every stored pattern (for the MQTT pattern select).
pub fn list() -> Vec<(String, String)> {
    let recs: Vec<Rec> = INDEX.lock(|c| c.borrow().clone());
    recs.iter()
        .filter_map(|r| rec_name(r).map(|n| (id_hex(r.seq), n)))
        .collect()
}

/// Find a stored pattern's id by exact name (first match).
pub fn id_by_name(name: &str) -> Option<String> {
    rec_by_name(name).map(|r| id_hex(r.seq))
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

/// [escape_into] over a long body, in 4 KiB slices with a yield between them
/// (Gitea #395). A stored source can be 32 KiB, and `escape_into` is a
/// per-`char` loop — 32,768 iterations was the longest uninterruptible
/// stretch in any read handler, long enough on its own to push the HUB75
/// compose past the panel's frame boundary. 4 KiB a turn keeps each stretch
/// a few hundred µs while costing a 32 KiB source only 8 yields, so a save
/// blocked on the [MapRead] guard still clears in tens of ms.
///
/// Chunk ends are walked forward to a char boundary, so multi-byte UTF-8 is
/// never split and the output is byte-identical to one `escape_into` call.
async fn escape_yielding(out: &mut String, s: &str) {
    const CHUNK: usize = 4096;
    let mut at = 0;
    while at < s.len() {
        let mut end = (at + CHUNK).min(s.len());
        while end < s.len() && !s.is_char_boundary(end) {
            end += 1;
        }
        escape_into(out, &s[at..end]);
        at = end;
        embassy_futures::yield_now().await;
    }
}

/// `GET /api/patterns/<id>` → `{"id","name","source"}` | None.
///
/// The source is read STRAIGHT OUT OF THE MAPPING and escaped into one
/// pre-sized response buffer — no source Vec, no intermediate copy. The
/// read guard is held across that copy, which since #395 yields every 4 KiB
/// (see [escape_yielding]): a concurrent save now waits a handful of
/// scheduler turns rather than one 32 KiB uninterruptible loop. If a save or
/// compaction is already running we wait for it (up to ~200 ms) rather than
/// reporting the pattern missing.
pub async fn get_json(id: &str) -> Option<String> {
    let r = rec_of(id)?;
    let name = rec_name(&r)?;
    let guard = MapRead::acquire().await?;
    let len = r.src_len as usize;
    let mut out = String::new();
    out.try_reserve_exact(len + len / 8 + name.len() + 48).ok()?;
    push_piece(&mut out, "{\"id\":\"");
    push_piece(&mut out, id);
    push_piece(&mut out, "\",\"name\":\"");
    escape_into(&mut out, &name);
    push_piece(&mut out, "\",\"source\":\"");
    match payload_slice(r.src_off(), r.src_len) {
        Some(b) => escape_yielding(&mut out, core::str::from_utf8(b).ok()?).await,
        None => {
            // flashmap-off: the one path that still copies
            let v = payload_vec(r.src_off(), r.src_len)?;
            escape_yielding(&mut out, core::str::from_utf8(&v).ok()?).await;
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
    let r = rec_of(id)?;
    payload_slice(r.src_off(), r.src_len)
}

/// A stored pattern's source in a transient Vec — the `flashmap-off`
/// fallback for [source_slice].
pub fn source_vec(id: &str) -> Option<Vec<u8>> {
    let r = rec_of(id)?;
    payload_vec(r.src_off(), r.src_len)
}

/// Read a stored pattern's LXBC bytecode into a transient Vec — the
/// `flashmap-off` fallback for [code_of].
pub fn bytecode_of(id: &str) -> Option<Vec<u8>> {
    let r = rec_of(id)?;
    payload_vec(r.bc_off(), r.bc_len)
}

/// A stored pattern's executable bytes, MAPPED. The engine executes these
/// IN PLACE (`deserialize_lean_static`), so the slice must stay valid for
/// as long as the `Program` does: pin the pattern ([pin_code] /
/// [pin_running] / [pin_prev_from_running]) before taking one and hold the
/// pin until the engine is dropped. The store never writes, moves or frees
/// a pinned file.
pub fn code_of(id: &str) -> Option<&'static [u8]> {
    let r = rec_of(id)?;
    payload_slice(r.bc_off(), r.bc_len)
}

/// Run `f` over a stored pattern's bytecode: the mapped bytes when the
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
/// a library swap stamps. Both are header fields: no flash read, no
/// allocation, and the hash is the same one the store verified the file
/// with (`luxel_core::netin::fnv1a` over the source bytes).
pub fn source_stat(id: &str) -> Option<(usize, u32)> {
    rec_of(id).map(|r| (r.src_len as usize, r.src_hash))
}

/// A stored pattern's source + bytecode bytes — exact, from the header.
/// No flash reads, no allocation. For heap pre-flights.
#[inline(never)]
pub fn stored_size_hint(id: &str) -> Option<usize> {
    rec_of(id).map(|r| (r.src_len + r.bc_len) as usize)
}

/// Human name of a stored pattern.
pub fn name_of(id: &str) -> Option<String> {
    rec_name(&rec_of(id)?)
}

/// (log bytes in use, log bytes total, stored patterns) for `/api/status`.
/// A total of 0 means the store never came up. Bytes, not pages: exact
/// packing is the point of #340, so pages would hide it.
pub fn store_stats() -> (u32, u32, u32, u32) {
    let (used, n) = INDEX.lock(|c| {
        let idx = c.borrow();
        (idx.iter().map(|r| r.size()).sum::<u32>(), idx.len() as u32)
    });
    let total = if REGION.load(Ordering::Relaxed) == 0 { 0 } else { LOG_LEN };
    (used, total, n, DEAD_BYTES.load(Ordering::Relaxed))
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
///
/// This is the AD-HOC slot's writer, where every region is page-aligned and
/// wholly owned. The file log packs at 4-byte granularity and uses
/// [erase_pages] + [write_at] instead.
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

/// Erase log pages `[from, to)` — skipping any the mapping already shows as
/// erased, which is the usual case at the head of an append and what keeps
/// a save's wear to the pages it actually dirties. One
/// `ota::with_flash` op per page with yields between, same as everything
/// else that touches this flash.
async fn erase_pages(region: u32, from: u32, to: u32) -> bool {
    for p in from..to {
        if with_log(|a| patlog::erased(a, p * PAGE, PAGE)) == Some(true) {
            continue;
        }
        let abs = region + LOG_OFF + p * PAGE;
        let r = crate::ota::with_flash_as(crate::core1::tag::RAW_ERASE, |f| {
            BlockingNorFlash::erase(f, abs, abs + PAGE).is_ok()
        });
        if r != Some(true) || crate::ota::ota_active() {
            return false;
        }
        raw_invalidate(LOG_OFF + p * PAGE, PAGE as usize);
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
    true
}

/// Write `data` at log-relative `rel` WITHOUT erasing: the pages behind it
/// are already erased (an append) or have just been erased (a compaction),
/// and a `dead` word is a deliberate 1 → 0 write into a live page. `rel`
/// must be 4-byte aligned; a trailing partial word is padded with 0xFF, the
/// erased state, so it never disturbs a neighbour.
///
/// No single fenced op crosses a page boundary — a fence parks the other
/// core, and 40 KiB of bytecode in one op would park it for the whole
/// write.
async fn write_at(region: u32, rel: u32, data: &[u8]) -> bool {
    if rel % 4 != 0 {
        return false;
    }
    if data.is_empty() {
        return true;
    }
    let base = region + LOG_OFF + rel;
    let mut stage = alloc::vec![0u32; PAGE as usize / 4];
    let mut at = 0usize;
    while at < data.len() {
        let page_left = (PAGE - ((rel + at as u32) % PAGE)) as usize;
        let n = (data.len() - at).min(page_left);
        let words = n.div_ceil(4);
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(stage.as_mut_ptr() as *mut u8, words * 4)
        };
        bytes[words * 4 - 4..].fill(0xFF);
        bytes[..n].copy_from_slice(&data[at..at + n]);
        let r = crate::ota::with_flash_as(crate::core1::tag::RAW_WRITE, |f| {
            BlockingNorFlash::write(f, base + at as u32, &bytes[..words * 4]).is_ok()
        });
        if r != Some(true) || crate::ota::ota_active() {
            return false;
        }
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
        at += n;
    }
    raw_invalidate(LOG_OFF + rel, data.len());
    true
}

/// Persist the running pattern's source + blob to the AD-HOC read-back
/// slot. Runs on the render task's swap path for ad-hoc pushes ONLY
/// (library swaps -- playlist/activate/resume/MQTT -- already have their
/// bytes in the log and never touch this slot: the flash-wear fix; see
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

/// Mark a file dead: one 4-byte 1 → 0 write into its header, no erase. Its
/// bytes stay put — which is what makes it safe to do to a file an engine
/// is still executing — until a compaction reclaims them.
async fn mark_dead(region: u32, r: &Rec) -> bool {
    if !write_at(region, r.dead_off(), &patlog::DEAD.to_le_bytes()).await {
        return false;
    }
    // sound without a CAS: every caller holds the StoreWrite guard
    let n = DEAD_BYTES.load(Ordering::Relaxed);
    DEAD_BYTES.store(n.saturating_add(r.size()), Ordering::Relaxed);
    true
}

/// `POST /api/patterns` (LXP1 envelope) → `{"ok":true,"id"}`.
/// Upserts by name. Caller decode-validates the bytecode first.
///
/// One file, appended whole: header, name, source, bytecode, then — after
/// the bytes are re-read through the mapping and hashed — the commit word.
/// A power cut before that leaves a record the next boot's scan refuses and
/// the previous version untouched; a cut after it leaves the new version
/// whole and, at worst, the old one still live (higher stamp wins).
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

    // Identity: a re-save keeps the pattern's seq, a new name takes the
    // next one. [MAX_RECS] caps how many files RAM will track — not a
    // storage limit (the log runs out of bytes long before, with real
    // patterns), but the index must be COMPLETE before anything mutates:
    // a compaction rewrites the log from it, and would drop what it cannot
    // see.
    let mut old = rec_by_name(name);
    let seq = match &old {
        Some(r) => r.seq,
        None => {
            if OVERFULL.load(Ordering::Relaxed) || INDEX.lock(|c| c.borrow().len()) >= MAX_RECS {
                let mut out = String::new();
                push_piece(&mut out, "{\"ok\":false,\"error\":\"the device library is full (");
                push_u32(&mut out, MAX_RECS as u32);
                push_piece(&mut out, " patterns) — delete one first\"}");
                return out;
            }
            NEXT_SEQ.load(Ordering::Relaxed)
        }
    };
    if OVERFULL.load(Ordering::Relaxed) {
        return err_json("the device library is full — delete some patterns and reboot");
    }
    // Whether this name was already stored, which is NOT the same question
    // as whether `old` still holds a record further down: a compaction can
    // leave `old` empty, and the seq counter must not move for that.
    let existed = old.is_some();

    let size = Rec::bytes(name.len() as u8, source.len() as u32, bc.len() as u32);
    let mut off = with_log(|a| patlog::place(a, CURSOR.load(Ordering::Relaxed), size)).flatten();
    if off.is_none() {
        // No room at the TAIL — but the cursor is a high-water mark and a
        // PINNED file cannot be moved down, so an earlier compaction may
        // have left whole erased pages below it that [patlog::place] will
        // never look at. Reach them first: it is a read-only page scan, and
        // an erased page holds no record, so an append there can overwrite
        // nothing (Gitea #388). Doing this BEFORE the compaction also keeps
        // a pinned log from re-repacking itself on every single save.
        off = with_log(|a| patlog::place_free(a, FREE_HINT.load(Ordering::Relaxed), size))
            .flatten();
    }
    if off.is_none() {
        // Still nothing. Compaction is a user-action-only cost (never on
        // the activation path — the wear rule), so this is where it
        // happens: repack the live files down over the dead ones, then try
        // again.
        if compact(region, size).await {
            off = with_log(|a| patlog::place(a, CURSOR.load(Ordering::Relaxed), size)).flatten();
        }
        if off.is_none() {
            off = with_log(|a| patlog::place_free(a, FREE_HINT.load(Ordering::Relaxed), size))
                .flatten();
        }
        // A compaction moves every unpinned file, so `old` is a stale
        // ADDRESS now — retiring it below would drop a DEAD word into the
        // middle of whatever was repacked over those bytes, tearing an
        // innocent file (Gitea #379). `reload` rebuilt the index; take the
        // record's new home from it.
        if old.is_some() {
            old = INDEX.lock(|c| c.borrow().iter().find(|r| r.seq == seq).copied());
        }
    }
    let Some(off) = off else {
        let (used, total, n, dead) = store_stats();
        println!(
            "patterns: store full — {} needs {} B, {}/{} used by {} files, {} reclaimable",
            name, size, used, total, n, dead
        );
        return err_json("the device's pattern storage is full — delete some patterns and retry");
    };

    let rec = Rec {
        off,
        stamp: NEXT_STAMP.load(Ordering::Relaxed),
        seq,
        src_len: source.len() as u32,
        bc_len: bc.len() as u32,
        src_hash: patlog::fnv1a(source.as_bytes()),
        bc_hash: patlog::fnv1a(bc),
        name_len: name.len() as u8,
        dead: false,
    };

    // Write it in the one order power-cut safety depends on (patlog's
    // append_plan, which the host suite cuts power inside at every word).
    for step in patlog::append_plan(&rec) {
        let ok = match step {
            patlog::Step::Erase(a, b) => erase_pages(region, a, b).await,
            patlog::Step::Header(at) => {
                let mut hdr = [0u8; patlog::HDR_PREFIX];
                patlog::encode_header(&rec, name.as_bytes(), &mut hdr);
                write_at(region, at, &hdr).await
            }
            patlog::Step::Name(at) => write_at(region, at, name.as_bytes()).await,
            patlog::Step::Src(at) => write_at(region, at, source.as_bytes()).await,
            patlog::Step::Bc(at) => write_at(region, at, bc).await,
            patlog::Step::Commit(at) => {
                // Prove the payload landed BEFORE publishing it: re-hash
                // the bytes as the readers will see them, through the
                // mapping we just invalidated.
                let good = with_log(|a| {
                    patlog::hash_range(a, rec.src_off(), rec.src_len) == Some(rec.src_hash)
                        && patlog::hash_range(a, rec.bc_off(), rec.bc_len) == Some(rec.bc_hash)
                }) == Some(true);
                good && write_at(region, at, &patlog::COMMIT.to_le_bytes()).await
            }
        };
        if !ok {
            // no {:?} on the step: deriving Debug for it costs 710 B of
            // OTA slot to name six variants (#310's margin is that tight)
            println!("patterns: writing {} at {} failed after {} B", name, off, size);
            return err_json(
                "couldn't write the pattern to flash — the store may be busy; try again in a moment",
            );
        }
    }

    // Published. The index moves to the new file and the cursor past it.
    INDEX.lock(|c| {
        let mut idx = c.borrow_mut();
        match idx.iter_mut().find(|r| r.seq == seq) {
            Some(slot) => *slot = rec,
            None => idx.push(rec),
        }
        sort_by_off(&mut idx);
    });
    let hi = CURSOR.load(Ordering::Relaxed);
    CURSOR.store(hi.max(rec.end()), Ordering::Relaxed);
    if rec.end() <= hi {
        // it went into a hole below the cursor, so those bytes were part of
        // what the store called reclaimable and are not any more
        let d = DEAD_BYTES.load(Ordering::Relaxed);
        DEAD_BYTES.store(d.saturating_sub(rec.size()), Ordering::Relaxed);
        FREE_HINT.store(rec.end(), Ordering::Relaxed);
    }
    NEXT_STAMP.store(rec.stamp.wrapping_add(1), Ordering::Relaxed);
    if !existed {
        NEXT_SEQ.store(seq.wrapping_add(1), Ordering::Relaxed);
    }
    // The superseded version can go now. Marking it dead is safe even when
    // an engine is executing it: nothing moves or erases those bytes until
    // a compaction, and a compaction skips a pinned seq.
    if let Some(o) = old {
        if !mark_dead(region, &o).await {
            println!("patterns: could not retire the old {} — it goes at the next boot", name);
        }
    }
    let mut out = String::new();
    push_piece(&mut out, "{\"ok\":true,\"id\":\"");
    push_piece(&mut out, &id_hex(seq));
    push_piece(&mut out, "\"}");
    out
}

/// `DELETE /api/patterns/<id>` → `{"ok":true}` | `{"ok":false,…}`.
///
/// The bytes stay where they are — a pattern an engine is still executing
/// keeps running — and a compaction reclaims them later.
pub async fn delete(id: &str) -> String {
    let Some(r) = rec_of(id) else {
        return err_json("no such pattern");
    };
    let region = REGION.load(Ordering::Relaxed);
    if region == 0 {
        return err_json("pattern storage unavailable (device needs reflash)");
    }
    let Some(_guard) = StoreWrite::acquire().await else {
        return err_json("the store is busy — try again in a moment");
    };
    if !mark_dead(region, &r).await {
        return err_json("flash error");
    }
    INDEX.lock(|c| c.borrow_mut().retain(|e| e.seq != r.seq));
    String::from("{\"ok\":true}")
}

// --- the engine pin set (Gitea #260) ---
//
// The engine executes a library pattern's LXBC **in place** from the log:
// `deserialize_lean_static` makes the `Program`'s code and constant pool a
// `&'static [u32]` INTO THE MAPPING, not a heap copy. So a file an engine
// still holds must never be written, moved (compaction) or erased while it
// holds it — the render task would be executing erased flash, and on a
// dual-core board the store runs on the OTHER core, in parallel. The same
// pin covers the file's SOURCE, which the HTTP layer streams out of the
// mapping for the running pattern's read-back.
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
// are conservative by construction (a stale one wastes log bytes until the
// next swap overwrites it; it can never free something live).
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
/// other core from compacting those bytes out from under the decode.
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
/// outgoing blend source and keeps executing its file. Call it at the
/// same moment main.rs does `prev = engine.take()`.
pub fn pin_prev_from_running() {
    PINS.lock(|c| {
        let mut p = c.get();
        p[2] = p[1];
        c.set(p);
    });
}

/// Slot 0 released — the decode has finished and whatever it produced is
/// either installed (slot 1 names it) or dropped. Call it at the END of a
/// library swap, always: slot 0 exists only for the window between
/// [pin_code] and the engine being installed, and a pin that is never
/// released names the last library pattern the device ever decoded for the
/// rest of the boot. A compaction then treats that file as frozen even
/// after it has been deleted, cannot pack anything below it, and leaves the
/// cursor — and with it every free byte underneath — out of reach. That is
/// how the Athom rig ended up refusing saves with 58 % of its store idle
/// (Gitea #388).
pub fn unpin_code() {
    set_pin(0, None);
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

// --- compaction ---

/// Repack the log: slide the live files down over the dead ones until
/// `need` bytes open up at the tail.
///
/// The log is append-only, so this is the ONLY thing that gives space back,
/// and only a SAVE that ran out of room calls it — an activation never
/// compacts, so playlist churn stays wear-free.
///
/// Shape, and why it is safe:
///
/// * `patlog::plan` places every live file (plus any dead one an engine is
///   still executing) at or below its current offset, and never inside a
///   page a PINNED file occupies. Nothing moves up.
/// * The executor then rewrites one 4 KiB destination page at a time, in
///   ascending order, through a RAM buffer it fills BEFORE erasing the
///   page. Every byte a page needs lives at an offset ≥ that page's start,
///   so buffering is the whole safety argument for an overlapping repack.
/// * Pages a pinned file occupies are skipped outright: those bytes are
///   being executed.
/// * A page whose new content equals its old content is not erased at all,
///   so a compaction that has little to do costs little.
/// * Last, once every destination page is written, the stale copies above
///   the new cursor are erased.
///
/// **Bound.** At most one erase + one 4 KiB write per page of the packed
/// log, plus one erase per page of the tail it sweeps: 183 + 183 flash ops
/// worst case on a full 732 KiB log, each its own fenced door with a 1 ms
/// yield. `core1::fenced` feeds the watchdog every 64 fences (#309), so a
/// long pass is slow, not fatal.
///
/// **A power cut mid-pass** leaves the log below the frontier repacked and
/// above it as it was; every header carries its own offset, so both halves
/// still parse and the most that can be lost is the one file straddling the
/// frontier (`tools/patlog-check` cuts power at every write boundary of a
/// compaction and asserts exactly that).
#[inline(never)]
async fn compact(region: u32, need: u32) -> bool {
    let (pin_buf, pin_n) = pins();
    let pinned = &pin_buf[..pin_n];

    // Everything that must survive: the index, plus any file a pinned
    // engine is executing even though the index has moved on from it.
    let live: Vec<Rec> = INDEX.lock(|c| c.borrow().clone());
    let mut keep = live.clone();
    let mut extra: Vec<Rec> = Vec::new();
    with_log(|a| {
        patlog::scan(a, &mut |r: &Rec, _: &[u8]| {
            if pinned.contains(&r.seq) && !live.iter().any(|l| l.off == r.off) {
                extra.push(*r);
            }
        })
    });
    keep.extend(extra);
    sort_by_off(&mut keep);

    let mut places: Vec<patlog::Place> = Vec::new();
    // A plan that does not place every live file is not a plan. Refusing
    // costs the user a failed save; proceeding costs them the files
    // (Gitea #379), so this never degrades to "compact what we can".
    let Some(packed) = patlog::plan(&keep, pinned, &mut |p| places.push(p)) else {
        println!("patterns: compaction refused — no plan places all {} files", keep.len());
        return false;
    };
    let old_end = CURSOR.load(Ordering::Relaxed);
    if packed + need > LOG_LEN {
        // The TAIL will not take it. That is not the whole question when a
        // pinned record holds the cursor high: the space this repack frees
        // is then all *below* it, and `place_free` can use it. Refusing on
        // the tail alone is what left the rig at 49 of 119 patterns
        // (Gitea #388). Only erase nothing when there is nothing to gain.
        if patlog::free_run_after(&places, LOG_LEN) < patlog::align_page(need) {
            return false;
        }
    }
    println!(
        "patterns: compacting the log for {} B ({} files, {} B -> {} B)",
        need,
        keep.len(),
        old_end,
        packed
    );

    let mut buf = alloc::vec![0u8; PAGE as usize];
    for page in 0..patlog::align_page(packed) / PAGE {
        if patlog::frozen_page(&keep, pinned, page) {
            continue;
        }
        if with_log(|a| patlog::build_page(a, page, &keep, &places, &mut buf)) != Some(true) {
            println!("patterns: compaction read failed at page {}", page);
            break;
        }
        let same = with_log(|a| a.view(page * PAGE, PAGE as usize).map(|v| v == &buf[..]));
        if same == Some(Some(true)) {
            continue;
        }
        if !erase_pages(region, page, page + 1).await
            || !write_at(region, page * PAGE, &buf).await
        {
            println!("patterns: compaction write failed at page {}", page);
            break;
        }
    }
    drop(buf);
    // Last: the stale copies the repack left above the new cursor. Until
    // now they were the source data.
    let (from, to) = patlog::sweep_pages(packed, old_end);
    // Never discarded: when the sweep fails the stale copies above the new
    // cursor are still there, nothing is lost (every header carries its own
    // offset, so both halves parse) but the next scan keeps finding them and
    // the cursor does not come back down. It goes out on the line below
    // rather than in a println! of its own — a second format site is
    // hundreds of bytes of OTA slot (#310).
    let swept = erase_pages(region, from, to).await;

    // Rebuild every byte of RAM state from what the flash now says — which
    // re-hashes every file, so a compaction that damaged one drops it here
    // rather than handing it to the engine.
    let s = reload();
    println!(
        "patterns: compacted — {} files, {} B used, {} B free, {} B held below a pin ({} torn){}",
        s.recs,
        s.live,
        LOG_LEN - s.cursor,
        DEAD_BYTES.load(Ordering::Relaxed),
        s.torn,
        if swept { "" } else { " — SWEEP FAILED" }
    );
    true
}

/// The running pattern's executable bytes as mapped memory, per the VM
/// consumer contract (docs/research/flash-mmap.md): rodata for the built-in
/// default (the bootloader's own mapping), the ad-hoc slot side the last
/// store_current wrote, or the library pattern's file. None = read it
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
