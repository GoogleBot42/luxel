//! On-device pattern library — the firmware half of the `/api/patterns`
//! CRUD contract the native mirror (crates/luxel-cli/src/serve.rs) and the
//! playground already speak.
//!
//! # Storage: `sequential-storage` over the `storage` partition
//!
//! An established, power-loss-safe, wear-leveled key→value store over NOR
//! flash (PLAN.md's storage model) rather than a hand-rolled blob. The
//! `storage` partition (0x210000, 1 MB) was freed by dropping factory
//! (partitions.csv). We resolve it from the *live* partition table at boot
//! and disable the store if it is absent — so this firmware is safe even on
//! the old table, where 0x210000 is still the live ota_1 app slot.
//!
//! # Chunked patterns (larger than one flash page)
//!
//! A sequential-storage item must fit one flash page (~4 KB), so a pattern's
//! source is split across up to [MC] chunk items of [CHUNK] bytes, and its
//! LXBC bytecode across up to [MC_BC] chunks (devices execute bytecode only —
//! the compiler lives in the browser/CLI). Each pattern has a small monotonic
//! **seq** (its API id is `seq ^ ID_MASK`, mirroring serve.rs). Keys, all u32:
//!   - meta      key = `seq`                                 (bits 31/30 clear)
//!   - src chunk key = `CHUNK_FLAG | seq*2*MC + gen*MC + c`  (bit 31 set)
//!   - bc  chunk key = `CHUNK_FLAG | BC_FLAG | seq*2*MC_BC + gen*MC_BC + c`
//! The meta value is `[gen][count][bc_count][name_len][name]`; source and
//! bytecode live in their chunk items under the current generation.
//!
//! **Atomic updates via generation flip.** An update writes the new chunks
//! to the *other* generation, then rewrites the meta (which selects the
//! generation) — a single item store that is the atomic commit point. A
//! power loss before the meta write leaves the old generation fully intact
//! (its chunks were never touched), so the pattern reads as its previous
//! version. After commit we best-effort remove the old generation's chunks.
//!
//! A **RAM index** (seq, gen, count, name per pattern) is built at boot so
//! list/lookup never scan flash; runtime reads/writes address chunks by key
//! directly.
//!
//! # Flash access
//!
//! sequential-storage is async; esp-storage's FlashStorage is blocking (see
//! the AsyncFlash adapter). Each transaction *leases* the driver out of the
//! OTA module (never holding its critical-section mutex across erases) and
//! drives the ops with `block_on` (the adapter never truly pends).

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

/// Flash range sequential-storage manages (512 KiB / 128 pages of the 1 MiB
/// partition). Holds a healthy library with GC headroom; the rest is
/// reserved. Bigger ranges cost more per-op scan under NoCache.
const STORE_LEN: u32 = 0x8_0000;

/// Resolved flash offset of the `storage` partition, or 0 if absent (old
/// table) — every op then refuses / reads empty. Set once in [init].
static REGION: AtomicU32 = AtomicU32::new(0);
/// Next pattern seq (monotonic). API id = `seq ^ ID_MASK` (mirrors serve.rs).
static NEXT_SEQ: AtomicU32 = AtomicU32::new(0);
const ID_MASK: u32 = 0x5eed_1e55;

const MAX_NAME: usize = 64;
/// Max chunks per pattern, and bytes per chunk (safely under one 4 KiB page
/// alongside the u32 key + item header). MC=4 caps source at ~15 KB — the
/// practical ceiling: the 16 KiB HTTP request buffer bounds a POST there
/// anyway, and a larger GET response would risk OOM on the fragmented heap.
/// Still ~4x the old single-page limit; covers virtually all patterns.
const MC: u8 = 8;
const CHUNK: usize = 3840;
const MAX_SOURCE: usize = MC as usize * CHUNK; // 30 KB
/// Bytecode chunk budget: LXBC can run larger than its source, so it gets
/// more chunks. Both caps sit comfortably past what the device's heap can
/// actually run — the RAM floor, not flash, is the real ceiling.
const MC_BC: u8 = 10;
pub const MAX_BC: usize = MC_BC as usize * CHUNK; // ~38 KB
const MAX_PATTERNS: usize = 24;
/// Chunk keys carry bit 31; meta keys (= seq) do not, keeping them disjoint.
const CHUNK_FLAG: u32 = 0x8000_0000;
/// Bit 30 separates bytecode chunks from source chunks (seqs are tiny, so
/// the low bits never reach it).
const BC_FLAG: u32 = 0x4000_0000;
/// sequential-storage scratch: ≥ the largest item (one chunk + key + header).
const BUF: usize = 4096;

/// On-flash layout version. Bumped when the key/value scheme changes; a
/// mismatch at boot wipes `storage` (incompatible old data — e.g. the
/// pre-chunking single-item format — would otherwise be misparsed). Stored
/// under a reserved meta-space key no real seq can reach.
/// v3: bytecode chunks + bc_count in the meta (patterns carry LXBC).
/// v4: bigger chunk budgets (MC 4→8, MC_BC 6→10) — the chunk-key layout
/// depends on MC/MC_BC, so raising them is a format bump.
const FORMAT_VERSION: u32 = 4;
const FORMAT_KEY: u32 = 0x7FFF_FFFF;

fn meta_key(seq: u32) -> u32 {
    seq // seq < CHUNK_FLAG always (bounded by save count)
}
fn chunk_key(seq: u32, gen: u8, c: u8) -> u32 {
    CHUNK_FLAG | (seq * (2 * MC as u32) + gen as u32 * MC as u32 + c as u32)
}
fn bc_chunk_key(seq: u32, gen: u8, c: u8) -> u32 {
    CHUNK_FLAG | BC_FLAG | (seq * (2 * MC_BC as u32) + gen as u32 * MC_BC as u32 + c as u32)
}

fn id_hex(seq: u32) -> String {
    let mut out = String::new();
    push_hex(&mut out, seq ^ ID_MASK, 8);
    out
}
fn seq_of(id: &str) -> Option<u32> {
    u32::from_str_radix(id, 16).ok().map(|v| v ^ ID_MASK)
}

/// RAM index entry — one per stored pattern. Sources/bytecode stay in flash.
#[derive(Clone)]
struct Entry {
    seq: u32,
    gen: u8,
    count: u8,
    bc_count: u8,
    name: String,
}

static INDEX: BlockingMutex<CriticalSectionRawMutex, RefCell<Vec<Entry>>> =
    BlockingMutex::new(RefCell::new(Vec::new()));

/// meta value: `[gen][count][bc_count][name_len][name]`
fn deserialize_meta(bytes: &[u8]) -> Option<(u8, u8, u8, String)> {
    let gen = *bytes.first()? & 1; // clamp to {0,1} so `1 - gen` can't underflow
    let count = *bytes.get(1)?;
    let bc_count = *bytes.get(2)?;
    let nlen = *bytes.get(3)? as usize;
    if bytes.len() < 4 + nlen {
        return None;
    }
    let name = String::from(core::str::from_utf8(&bytes[4..4 + nlen]).ok()?);
    Some((gen, count, bc_count, name))
}
fn serialize_meta(gen: u8, count: u8, bc_count: u8, name: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(4 + name.len());
    v.push(gen);
    v.push(count);
    v.push(bc_count);
    v.push(name.len() as u8);
    v.extend_from_slice(name.as_bytes());
    v
}

/// Pages sequential-storage manages — the const the cache below is sized
/// by; wrong and the crate panics at some point.
const PAGES: usize = (STORE_LEN / PAGE) as usize;

/// The ONE sequential-storage cache for the store region, kept alive
/// across transactions.
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
/// the driver is out, and nothing else writes this range (the raw region —
/// current-slot + code arena — starts at [RAW_OFF] = `STORE_LEN`, past the
/// map's range). Any write that goes AROUND sequential-storage inside the
/// range (the format wipe in [init]) must call `invalidate_cache_state()`.
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

// --- flash helpers (run inside a with_store block) ---

async fn read_meta(
    af: &mut AsyncFlash<'_>,
    range: Range<u32>,
    buf: &mut [u8],
    seq: u32,
) -> Option<(u8, u8, u8, String)> {
    let cache = store_cache();
    match map::fetch_item::<u32, &[u8], _>(af, range, cache, buf, &meta_key(seq)).await {
        Ok(Some(bytes)) => deserialize_meta(bytes),
        _ => None,
    }
}

async fn read_source(
    af: &mut AsyncFlash<'_>,
    range: Range<u32>,
    buf: &mut [u8],
    seq: u32,
    gen: u8,
    count: u8,
) -> Option<String> {
    let cache = store_cache();
    // The count comes from a stored TOC record — never trust it with an
    // infallible alloc. A corrupt record on the Athom claimed 32 chunks
    // (120 KB: 4× the writer's own MC cap) and the with_capacity here
    // OOM-panic-rebooted the device on every /api/patterns/<id> fetch,
    // i.e. every web-app cold load (found via serial, 2026-08-15).
    if count > MC {
        return None;
    }
    // pre-size to avoid Vec doubling (a large pattern's peak alloc matters on
    // a fragmented heap — see get_json), but fallibly.
    let mut out: Vec<u8> = Vec::new();
    if out.try_reserve_exact(count as usize * CHUNK).is_err() {
        return None;
    }
    for c in 0..count {
        let key = chunk_key(seq, gen, c);
        match map::fetch_item::<u32, &[u8], _>(af, range.clone(), cache, buf, &key).await {
            Ok(Some(bytes)) => out.extend_from_slice(bytes),
            _ => return None,
        }
    }
    String::from_utf8(out).ok()
}

/// Reassemble a pattern's LXBC bytecode from its chunks.
async fn read_bc(
    af: &mut AsyncFlash<'_>,
    range: Range<u32>,
    buf: &mut [u8],
    seq: u32,
    gen: u8,
    bc_count: u8,
) -> Option<Vec<u8>> {
    let cache = store_cache();
    // Same defense as read_source: bc_count is untrusted stored data.
    if bc_count > MC_BC {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    if out.try_reserve_exact(bc_count as usize * CHUNK).is_err() {
        return None;
    }
    for c in 0..bc_count {
        let key = bc_chunk_key(seq, gen, c);
        match map::fetch_item::<u32, &[u8], _>(af, range.clone(), cache, buf, &key).await {
            Ok(Some(bytes)) => out.extend_from_slice(bytes),
            _ => return None,
        }
    }
    Some(out)
}

/// Write all chunks under `gen`, then the meta (the atomic commit).
async fn write_pattern(
    af: &mut AsyncFlash<'_>,
    range: Range<u32>,
    buf: &mut [u8],
    seq: u32,
    gen: u8,
    count: u8,
    bc_count: u8,
    name: &str,
    source: &str,
    bc: &[u8],
) -> Result<(), ()> {
    let cache = store_cache();
    let bytes = source.as_bytes();
    for c in 0..count {
        let s = c as usize * CHUNK;
        let e = (s + CHUNK).min(bytes.len());
        let chunk: &[u8] = &bytes[s..e];
        let key = chunk_key(seq, gen, c);
        if map::store_item(af, range.clone(), cache, buf, &key, &chunk).await.is_err() {
            return Err(());
        }
    }
    for c in 0..bc_count {
        let s = c as usize * CHUNK;
        let e = (s + CHUNK).min(bc.len());
        let chunk: &[u8] = &bc[s..e];
        let key = bc_chunk_key(seq, gen, c);
        if map::store_item(af, range.clone(), cache, buf, &key, &chunk).await.is_err() {
            return Err(());
        }
    }
    let meta = serialize_meta(gen, count, bc_count, name);
    let mslice: &[u8] = &meta;
    if map::store_item(af, range.clone(), cache, buf, &meta_key(seq), &mslice).await.is_err() {
        return Err(());
    }
    Ok(())
}

/// Best-effort removal of a generation's source + bytecode chunks
/// (post-commit cleanup, or full delete when paired with meta removal).
async fn remove_chunks(
    af: &mut AsyncFlash<'_>,
    range: Range<u32>,
    buf: &mut [u8],
    seq: u32,
    gen: u8,
    count: u8,
    bc_count: u8,
) {
    let cache = store_cache();
    for c in 0..count {
        let key = chunk_key(seq, gen, c);
        let _ = map::remove_item::<u32, _>(af, range.clone(), cache, buf, &key).await;
    }
    for c in 0..bc_count {
        let key = bc_chunk_key(seq, gen, c);
        let _ = map::remove_item::<u32, _>(af, range.clone(), cache, buf, &key).await;
    }
}

/// Resolve the `storage` partition and build the RAM index. Disables the
/// store (REGION = 0) if the partition is absent (old table).
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
    map_raw(start);

    let entries = with_store!(start, |af, range, buf| {
        // Format check: wipe storage if the on-flash layout isn't ours.
        let cache = store_cache();
        let fmt = match map::fetch_item::<u32, &[u8], _>(
            &mut af, range.clone(), cache, buf, &FORMAT_KEY,
        )
        .await
        {
            Ok(Some(b)) if b.len() == 4 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            _ => 0,
        };
        if fmt != FORMAT_VERSION {
            println!("patterns: format {} != {}, wiping storage", fmt, FORMAT_VERSION);
            let _ = anf::NorFlash::erase(&mut af, range.start, range.end).await;
            // erased behind sequential-storage's back — the shared cache
            // still describes the old contents
            *store_cache() = PageStateCache::new();
            let ver = FORMAT_VERSION.to_le_bytes();
            let vslice: &[u8] = &ver;
            let c2 = store_cache();
            let _ = map::store_item(&mut af, range.clone(), c2, buf, &FORMAT_KEY, &vslice).await;
            return Vec::new();
        }

        // Discover meta seqs by scanning, then read each authoritative meta.
        let mut seqs: Vec<u32> = Vec::new();
        let cache = store_cache();
        if let Ok(mut iter) =
            map::fetch_all_items::<u32, _, _>(&mut af, range.clone(), cache, buf).await
        {
            while let Ok(Some((key, _))) = iter.next::<u32, &[u8]>(buf).await {
                if key & CHUNK_FLAG == 0 && key != FORMAT_KEY && !seqs.contains(&key) {
                    seqs.push(key); // a meta key
                }
            }
        }
        let mut out: Vec<Entry> = Vec::new();
        for seq in seqs {
            if let Some((gen, count, bc_count, name)) =
                read_meta(&mut af, range.clone(), buf, seq).await
            {
                out.push(Entry { seq, gen, count, bc_count, name });
            }
        }
        out
    })
    .unwrap_or_default();

    let mut next = 0u32;
    for e in &entries {
        next = next.max(e.seq.wrapping_add(1));
    }
    NEXT_SEQ.store(next, Ordering::Relaxed);
    println!("patterns: {} stored (storage @ {:#x})", entries.len(), start);
    INDEX.lock(|c| *c.borrow_mut() = entries);
    arena_init();
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

/// Look up a pattern's (gen, count, bc_count, name) in the RAM index by id.
fn lookup(id: &str) -> Option<(u32, u8, u8, u8, String)> {
    let seq = seq_of(id)?;
    INDEX.lock(|c| {
        c.borrow()
            .iter()
            .find(|e| e.seq == seq)
            .map(|e| (e.seq, e.gen, e.count, e.bc_count, e.name.clone()))
    })
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
pub fn get_json(id: &str) -> Option<String> {
    let (seq, gen, count, _, name) = lookup(id)?;
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return None;
    }
    let source = with_store!(start, |af, range, buf| {
        read_source(&mut af, range, buf, seq, gen, count).await
    })
    .flatten()?;
    // Build the response in ONE pre-sized allocation, escaping in place. A
    // large pattern's source made the old `format!(json_escape(..))` path —
    // an ~11 KB intermediate plus a doubling result buffer — request a ~22 KB
    // contiguous block and OOM on a fragmented heap.
    let mut out = String::with_capacity(source.len() + source.len() / 8 + name.len() + 48);
    push_piece(&mut out, "{\"id\":\"");
    push_piece(&mut out, id);
    push_piece(&mut out, "\",\"name\":\"");
    escape_into(&mut out, &name);
    push_piece(&mut out, "\",\"source\":\"");
    escape_into(&mut out, &source);
    push_piece(&mut out, "\"}");
    Some(out)
}

/// Read a stored pattern's source (editor read-back, sync envelope).
pub fn source_of(id: &str) -> Option<String> {
    let (seq, gen, count, _, _) = lookup(id)?;
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return None;
    }
    with_store!(start, |af, range, buf| {
        read_source(&mut af, range, buf, seq, gen, count).await
    })
    .flatten()
}

/// Read a stored pattern's LXBC bytecode (what activation executes).
pub fn bytecode_of(id: &str) -> Option<Vec<u8>> {
    let (seq, gen, _, bc_count, _) = lookup(id)?;
    if bc_count == 0 {
        return None;
    }
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return None;
    }
    with_store!(start, |af, range, buf| {
        read_bc(&mut af, range, buf, seq, gen, bc_count).await
    })
    .flatten()
}

/// Upper bound on a stored pattern's source + bytecode bytes, from the TOC
/// chunk counts alone — no flash reads, no allocation. For heap pre-flights.
pub fn stored_size_hint(id: &str) -> Option<usize> {
    lookup(id).map(|(_, _, count, bc_count, _)| (count as usize + bc_count as usize) * CHUNK)
}

/// Human name of a stored pattern.
pub fn name_of(id: &str) -> Option<String> {
    lookup(id).map(|(_, _, _, _, name)| name)
}

// --- small reserved-key blobs (playlist definition + playback state) ---
// These live in the meta-space (< CHUNK_FLAG), above any real seq, alongside
// FORMAT_KEY. Each must fit one flash page.
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
    if bytes.len() > CHUNK {
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

// --- running-pattern read-back slot (replaces the old RAM copies) ---
//
// The source + blob of the *currently running* pattern used to live in two
// standing heap Vecs (shared::PATTERN_SRC / PATTERN_BC) purely for read-back
// (GET /api/pattern, the sync envelope, engine rebuilds). On a heap already
// dominated by that same running pattern they were the single largest
// resident cost, so they now live in flash and stream on demand.
//
// NOT map items: RAW PAGES in the reserved upper half of the partition
// (STORE_LEN..PAT_LEN -- sequential-storage never touches it). A first cut
// stored them as reserved-key map items, and every chunk read was then a
// NoCache scan over the whole 512 KiB store: dozens of back-to-back
// cache-off flash reads per chunk starved the WiFi task long enough to
// drop the NEXT connection's handshake (observed on-device as intermittent
// dead requests right after a readback). Raw pages make reads direct
// offset reads -- one short cache-off window per 4 KiB, the exact
// discipline assets.rs::read_chunk + FlashAsset have soak-proven -- and
// writes skip the map's GC entirely.
//
// The whole raw half is memory-mapped read-only through the cache MMU at
// boot (flashmap.rs, docs/research/flash-mmap.md "The VM consumer"): the
// engine executes the running pattern's bytecode IN PLACE from these pages
// (`current_code`), so no blob Vec exists on the device for a swap or a
// rebuild. Every writer below invalidates what it wrote before publishing
// it (cache lines do not watch the flash controller).
//
// Layout (offsets relative to the partition base):
//   CUR_OFF          header page: [magic u32][src_len u32][bc_len u32]
//                    [bc_side u32], written LAST so a power loss mid-store
//                    leaves a stale header at worst (the slot is ignored at
//                    boot anyway -- RAM meta rules within a session)
//   CUR_SRC_OFF      ad-hoc source bytes (up to CUR_SRC_MAX)
//   CUR_BC_OFF       ad-hoc LXBC bytes, TWO sides of CUR_BC_MAX: a swap
//                    writes the side the running engine is NOT executing
//                    from, so a mapped engine never sees its code change
//   ARENA_OFF        the library code arena: ARENA_PAGES 4 KiB pages carved
//                    into contiguous, 4-byte-aligned EXTENTS by the
//                    page-granular allocator in extents.rs, one stored
//                    pattern's bytecode each; the directory lives under
//                    ARENA_KEY in the map
const RAW_OFF: u32 = STORE_LEN;
const RAW_LEN: u32 = PAT_LEN - STORE_LEN;
const CUR_OFF: u32 = RAW_OFF;
const PAGE: u32 = 4096;
/// 32 KiB — one page past MAX_SOURCE (30 KB), which is the hard cap on a
/// stored source and therefore on anything read-back can ever want. It was
/// 96 KiB until 2026-09-06; the 64 KiB that freed went to the arena
/// (Gitea #281 — the ad-hoc region is one pattern, the arena is all of them).
const CUR_SRC_MAX: u32 = 8 * PAGE;
const CUR_BC_MAX: u32 = 16 * PAGE; // 64 KiB per side
const CUR_SRC_OFF: u32 = CUR_OFF + PAGE;
const CUR_BC_OFF: u32 = CUR_SRC_OFF + CUR_SRC_MAX;
const CUR_MAGIC: u32 = 0x4C58_4350; // "LXCP"
/// The bc side holding the RUNNING ad-hoc blob (the last successful
/// store_current); the next store writes the other one.
static CUR_BC_SIDE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
/// The arena: everything left in the raw half after the ad-hoc regions.
/// 0xA9000..0x100000 = 348 KiB = 87 pages, enough for every pattern a
/// device can hold (MAX_PATTERNS = 24) many times over at the ~1 KB the
/// median library blob actually costs.
const ARENA_OFF: u32 = CUR_BC_OFF + 2 * CUR_BC_MAX;
const ARENA_PAGES: usize = ((RAW_OFF + RAW_LEN - ARENA_OFF) / PAGE) as usize;
/// Extent directory (reserved-key map item, see the blobs above).
const ARENA_KEY: u32 = 0x7FFF_FFF9;
const _: () = assert!(CUR_SRC_MAX as usize >= MAX_SOURCE);
const _: () = assert!(ARENA_OFF + (ARENA_PAGES as u32) * PAGE <= RAW_OFF + RAW_LEN);
const _: () = assert!(ARENA_PAGES <= crate::extents::MAX_PAGES);
const _: () = assert!(ARENA_PAGES * crate::extents::PAGE >= MAX_BC);
const _: () = assert!(PAGE as usize == crate::extents::PAGE);

fn cur_bc_off(side: u8) -> u32 {
    CUR_BC_OFF + side as u32 * CUR_BC_MAX
}

/// Absolute flash offsets of the slot's (src, bc) data, or None when the
/// storage partition is absent. Lengths come from the RAM meta
/// (shared::SrcLoc/BcLoc) -- the header page is for future boot-time use.
pub fn current_slot_abs() -> Option<(u32, u32)> {
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return None;
    }
    let side = CUR_BC_SIDE.load(Ordering::Relaxed);
    Some((start + CUR_SRC_OFF, start + cur_bc_off(side)))
}

/// Write one raw region (erase + word-aligned page writes). Every flash op
/// borrows the driver via [crate::ota::with_flash] for just that one op --
/// the driver never leaves the global, so concurrent flash users (asset
/// serving/pushes, an incoming OTA begin, a store transaction) interleave
/// between pages instead of reading busy for the whole burst. The previous
/// take-for-the-whole-burst design starved them: with a 5 s playlist
/// churning swaps, asset pushes failed 5/6, /api/ota misreported "update
/// already in progress", and served assets truncated mid-body (measured on
/// the Athom, 2026-08-15). The 1 ms yields between ops keep WiFi airtime
/// AND give waiting HTTP tasks a real window to grab the driver.
/// Best-effort: false when an OTA begins or a store transaction leases the
/// driver away mid-burst -- the caller degrades (never a panic).
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

/// Persist the running pattern's source + blob to the read-back slot. Runs
/// on the render task's swap path for AD-HOC pushes ONLY (library swaps --
/// playlist/activate/resume/MQTT -- read back from the pattern store
/// instead and never touch this slot: the flash-wear fix; see
/// main.rs::persist_current_pattern). A brief hitch (a few dozen page
/// erases/writes with yields between them), borrowing the driver per op
/// (see [write_raw]) instead of monopolizing it. Best-effort: false if
/// storage is absent, an OTA is in progress, a store transaction leases
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

/// Read the running pattern's whole blob back from flash into a TRANSIENT
/// fallible Vec -- the engine rebuild (pixel-count / map change) needs it
/// contiguous to deserialize, then drops it immediately. `len` comes from
/// the RAM meta. None on a failed reservation or a flash-busy read; the
/// caller keeps the engine paused rather than panicking.
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

/// `POST /api/patterns` (LXP1 envelope) → `{"ok":true,"id"}`.
/// Upserts by name. Caller decode-validates the bytecode first.
pub fn save(name: &str, source: &str, bc: &[u8]) -> String {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_NAME {
        return String::from("{\"ok\":false,\"error\":\"name must be 1..=64 bytes\"}");
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
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return String::from(
            "{\"ok\":false,\"error\":\"pattern storage unavailable (device needs reflash)\"}",
        );
    }

    // Decide seq + generation under the index lock.
    enum Plan {
        Update { seq: u32, new_gen: u8, old_gen: u8, old_count: u8, old_bc: u8 },
        New { seq: u32 },
        Full,
    }
    let plan = INDEX.lock(|c| {
        let idx = c.borrow();
        if let Some(e) = idx.iter().find(|e| e.name == name) {
            Plan::Update {
                seq: e.seq,
                new_gen: 1 - e.gen,
                old_gen: e.gen,
                old_count: e.count,
                old_bc: e.bc_count,
            }
        } else if idx.len() >= MAX_PATTERNS {
            Plan::Full
        } else {
            Plan::New { seq: NEXT_SEQ.load(Ordering::Relaxed) }
        }
    });
    let (seq, new_gen, old_gen, old_count, old_bc, is_new) = match plan {
        Plan::Full => {
            let mut out = String::new();
            push_piece(&mut out, "{\"ok\":false,\"error\":\"the device library is full (");
            push_u32(&mut out, MAX_PATTERNS as u32);
            push_piece(&mut out, " patterns) — delete one first\"}");
            return out;
        }
        Plan::New { seq } => (seq, 0u8, 0u8, 0u8, 0u8, true),
        Plan::Update { seq, new_gen, old_gen, old_count, old_bc } => {
            (seq, new_gen, old_gen, old_count, old_bc, false)
        }
    };
    let count = source.len().div_ceil(CHUNK) as u8;
    let bc_count = bc.len().div_ceil(CHUNK) as u8;

    let committed = with_store!(start, |af, range, buf| {
        if write_pattern(
            &mut af, range.clone(), buf, seq, new_gen, count, bc_count, name, source, bc,
        )
        .await
        .is_err()
        {
            return false;
        }
        // commit done — old generation is now unreferenced; reclaim it.
        remove_chunks(&mut af, range, buf, seq, old_gen, old_count, old_bc).await;
        true
    })
    .unwrap_or(false);

    if !committed {
        return String::from(
            "{\"ok\":false,\"error\":\"couldn't write the pattern to flash — the store may be full; delete some patterns and retry\"}",
        );
    }

    INDEX.lock(|c| {
        let mut idx = c.borrow_mut();
        if let Some(e) = idx.iter_mut().find(|e| e.seq == seq) {
            e.gen = new_gen;
            e.count = count;
            e.bc_count = bc_count;
            e.name = String::from(name);
        } else {
            idx.push(Entry { seq, gen: new_gen, count, bc_count, name: String::from(name) });
        }
    });
    if is_new {
        NEXT_SEQ.store(seq.wrapping_add(1), Ordering::Relaxed);
    }
    let mut out = String::new();
    push_piece(&mut out, "{\"ok\":true,\"id\":\"");
    push_piece(&mut out, &id_hex(seq));
    push_piece(&mut out, "\"}");
    out
}

/// `DELETE /api/patterns/<id>` → `{"ok":true}` | `{"ok":false,…}`.
pub fn delete(id: &str) -> String {
    let Some((seq, gen, count, bc_count, _)) = lookup(id) else {
        return String::from("{\"ok\":false,\"error\":\"no such pattern\"}");
    };
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return String::from("{\"ok\":false,\"error\":\"no such pattern\"}");
    }
    let ok = with_store!(start, |af, range, buf| {
        let cache = store_cache();
        let meta_ok =
            map::remove_item::<u32, _>(&mut af, range.clone(), cache, buf, &meta_key(seq))
                .await
                .is_ok();
        remove_chunks(&mut af, range, buf, seq, gen, count, bc_count).await;
        meta_ok
    })
    .unwrap_or(false);

    if !ok {
        return String::from("{\"ok\":false,\"error\":\"flash error\"}");
    }
    INDEX.lock(|c| c.borrow_mut().retain(|e| e.seq != seq));
    arena_forget(seq);
    String::from("{\"ok\":true}")
}

// --- the mapped raw half (flashmap.rs) ---

static RAW_BASE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static RAW_MAPPED_LEN: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// The raw half of the partition as memory, if the boot-time mapping
/// succeeded; None = every reader takes the read_nor path it always had
/// (and the code arena is off).
pub fn raw() -> Option<&'static [u8]> {
    let base = RAW_BASE.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    let len = RAW_MAPPED_LEN.load(Ordering::Acquire);
    // SAFETY: base/len come from a flashmap::Mapped leaked in map_raw.
    Some(unsafe { core::slice::from_raw_parts(base as *const u8, len) })
}

/// Map the raw half read-only through the cache MMU and prove it (the
/// header page read both ways must agree) — same discipline as
/// assets::map_region. Once, from init, right after REGION resolves.
#[inline(never)]
fn map_raw(start: u32) {
    let m = match crate::flashmap::map(start + RAW_OFF, RAW_LEN) {
        Ok(m) => m,
        Err(e) => {
            println!("flashmap: pattern code not mapped ({:?}) — flash-controller reads", e);
            return;
        }
    };
    let mut via_nor = alloc::vec![0u8; PAGE as usize];
    let ok = crate::assets::read_chunk(start + RAW_OFF, &mut via_nor)
        && via_nor[..] == m.bytes()[..PAGE as usize];
    drop(via_nor);
    if !ok {
        println!(
            "flashmap: pattern code self-check FAILED at 0x{:x} — unmapping, flash-controller reads",
            m.vaddr()
        );
        crate::flashmap::unmap(m);
        return;
    }
    println!(
        "flashmap: pattern code 0x{:x}+0x{:x} -> 0x{:x} ({} x {} KiB pages from entry {}), self-check ok",
        m.phys(),
        RAW_LEN,
        m.vaddr(),
        m.pages(),
        crate::flashmap::page_size() / 1024,
        m.first_entry()
    );
    let b = m.leak();
    RAW_MAPPED_LEN.store(b.len(), Ordering::Release);
    RAW_BASE.store(b.as_ptr() as usize, Ordering::Release);
}

/// `len` bytes at partition-relative `rel` (inside the raw half) as mapped
/// memory.
fn raw_slice(rel: u32, len: usize) -> Option<&'static [u8]> {
    let r = raw()?;
    let s = rel.checked_sub(RAW_OFF)? as usize;
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

// --- the library code arena ---
//
// A page-granular EXTENT allocator over the arena region: one stored
// pattern's LXBC per extent, a contiguous run of 4 KiB erase pages,
// contiguous and 4-byte aligned so a library activation hands the engine a
// mapped slice instead of reassembling <=3,840-byte chunk items into a Vec.
// The planning half (bitmap, first-fit, compaction plan, directory format)
// is `extents.rs` -- pure, allocation-free and host-tested
// (tools/extent-check); everything below is the flash half.
//
// Why not fixed slots (what PR #276 shipped, replaced 2026-09-06, #281):
// seven 40 KiB slots cached seven patterns and wasted ~90 % of the region,
// because the median library blob is under 1 KB. Page extents cache every
// pattern a device can hold (MAX_PATTERNS = 24) in a fraction of the same
// space, so eviction -- and with it the LRU bookkeeping and the "a save may
// throw out someone else's cache" surprise -- is gone entirely.
//
// Rules the code enforces:
//   * Never write, free or move the RUNNING pattern's extent. A re-save of
//     the running pattern publishes a new extent and leaves the old
//     generation's in the directory; it is swept once something else runs.
//   * A save allocates a new extent, writes it, invalidates, hash-checks
//     the mapped bytes, publishes the directory, and only then frees the
//     superseded extent -- never in place.
//   * Compaction only when no contiguous hole fits a SAVE (a user action).
//     An activation-time fill never compacts and never evicts: playlist
//     churn writes flash at most once per pattern, then never (the
//     2026-08-15 wear rule). A save's compaction slides live extents
//     toward page 0, one at a time, each with the same
//     write/invalidate/hash discipline and the directory updated per move,
//     so a power cut mid-compaction costs at most the extent in flight
//     (re-cached from its chunks on its next activation).
//   * Boot rebuilds the bitmap from the persisted directory and drops any
//     extent the pattern index or the mapped bytes do not vouch for.

use crate::extents;

/// Zero pages until [arena_init] installs the real size — an all-zero
/// initializer keeps the ~470-byte directory in .bss instead of .data (it
/// is image bytes otherwise, and the OTA slot is the scarce resource). A
/// 0-page directory refuses every allocation and reports `[0, 0]`, which is
/// exactly what a device whose arena never came up should say.
static ARENA: BlockingMutex<CriticalSectionRawMutex, RefCell<extents::Dir>> =
    BlockingMutex::new(RefCell::new(extents::Dir::new(0)));
/// One allocator transaction at a time. `cache_code` is reachable from the
/// render task and from an HTTP task, and a compaction move leaves the
/// directory and the flash briefly out of step -- two of them interleaving
/// at an `.await` could hand the same pages to both.
static ARENA_BUSY: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

struct ArenaGuard;

impl ArenaGuard {
    /// None when another transaction is in flight (the caller degrades to
    /// the chunk path, never blocks).
    fn take() -> Option<ArenaGuard> {
        // ARENA.lock is the critical section that makes this test-and-set
        // atomic -- no extra primitive needed.
        ARENA.lock(|_| {
            if ARENA_BUSY.load(Ordering::Relaxed) {
                return None;
            }
            ARENA_BUSY.store(true, Ordering::Relaxed);
            Some(ArenaGuard)
        })
    }
}

impl Drop for ArenaGuard {
    fn drop(&mut self) {
        ARENA_BUSY.store(false, Ordering::Relaxed);
    }
}

/// The arena only exists through the mapping — that is its whole point.
fn arena_on() -> bool {
    raw().is_some()
}

/// Partition-relative offset of an arena page.
fn arena_off(page: u16) -> u32 {
    ARENA_OFF + page as u32 * PAGE
}

/// An extent's bytes, mapped.
fn extent_bytes(e: &extents::Extent) -> Option<&'static [u8]> {
    raw_slice(arena_off(e.start), e.len as usize)
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

/// Write the directory to its reserved map key. The staging buffer is a
/// heap Vec, never a stack array: this runs at picoserve depth on the
/// shared main-task stack.
fn persist_arena() -> bool {
    let mut buf = alloc::vec![0u8; extents::SER_MAX];
    let n = ARENA.lock(|c| c.borrow().to_bytes(&mut buf));
    n > 0 && store_blob(ARENA_KEY, &buf[..n])
}

/// The (seq, bytecode generation) pairs the index currently knows.
fn live_gens() -> Vec<(u32, u8)> {
    INDEX.lock(|c| {
        c.borrow().iter().filter(|e| e.bc_count > 0).map(|e| (e.seq, e.gen)).collect()
    })
}

/// Boot: load the directory, rebuild the bitmap, and keep only extents the
/// index AND the mapped bytes both vouch for. A torn write, a stale
/// generation or a layout change is dropped, never executed.
#[inline(never)]
fn arena_init() {
    if !arena_on() {
        println!("patterns: code arena off (raw half not mapped)");
        return;
    }
    let stored = read_blob(ARENA_KEY);
    let (mut dir, mut dropped) = match stored.as_deref() {
        Some(b) => extents::Dir::from_bytes(b, ARENA_PAGES as u16),
        None => (extents::Dir::new(ARENA_PAGES as u16), 0),
    };
    let live = live_gens();
    dropped += dir.retain(|e| {
        e.len as usize <= MAX_BC
            && live.contains(&(e.seq, e.gen))
            && extent_bytes(e).is_some_and(|b| fnv1a(b) == e.hash)
    }) as u32;
    let (valid, used) = (dir.count(), dir.used_pages());
    ARENA.lock(|c| *c.borrow_mut() = dir);
    println!(
        "patterns: code arena {} pages, {} extents valid ({} dropped), {} pages used",
        ARENA_PAGES, valid, dropped, used
    );
    if dropped > 0 {
        persist_arena();
    }
}

/// A stored pattern's executable bytes, mapped — if an extent holds its
/// CURRENT bytecode generation. The engine executes these IN PLACE
/// (`deserialize_lean_static`), so the slice must stay valid for as long as
/// the `Program` does: pin the pattern ([pin_code] / [pin_running] /
/// [pin_prev_from_running]) before taking one and hold the pin until the
/// engine is dropped. The store never writes, moves or frees a pinned
/// extent.
pub fn code_of(id: &str) -> Option<&'static [u8]> {
    if !arena_on() {
        return None;
    }
    let (seq, gen, _, bc_count, _) = lookup(id)?;
    if bc_count == 0 {
        return None;
    }
    let e = ARENA.lock(|c| c.borrow().find(seq, gen).map(|(_, e)| e))?;
    extent_bytes(&e)
}
/// Run `f` over a stored pattern's bytecode: the mapped slot when there is
/// one (no copy), else a transient Vec from the chunk store.
pub fn with_code<R>(id: &str, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
    if let Some(c) = code_of(id) {
        return Some(f(c));
    }
    bytecode_of(id).map(|v| f(&v))
}

/// Does a stored pattern's bytecode still decode on this firmware?
/// None = no such pattern / no bytecode.
pub fn validate_stored(id: &str) -> Option<Result<(), luxel_core::bytecode::BcError>> {
    with_code(id, |bc| luxel_core::bytecode::validate(bc).map(|_| ()))
}

/// (source length, FNV-1a of the source) streamed chunk by chunk — the
/// identity hash + Content-Length a library swap stamps, without ever
/// materializing the source.
pub fn source_stat(id: &str) -> Option<(usize, u32)> {
    let (seq, gen, count, _, _) = lookup(id)?;
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 || count > MC {
        return None;
    }
    with_store!(start, |af, range, buf| {
        let cache = store_cache();
        let mut len = 0usize;
        let mut h = FNV_INIT;
        for c in 0..count {
            let key = chunk_key(seq, gen, c);
            match map::fetch_item::<u32, &[u8], _>(&mut af, range.clone(), cache, buf, &key)
                .await
            {
                Ok(Some(b)) => {
                    len += b.len();
                    h = fnv1a_update(h, b);
                }
                _ => return None,
            }
        }
        Some((len, h))
    })
    .flatten()
}

// --- the engine pin set (Gitea #260) ---
//
// The engine executes a library pattern's LXBC **in place** from its arena
// extent: `deserialize_lean_static` makes the `Program`'s code and constant
// pool a `&'static [u32]` INTO THE MAPPING, not a heap copy. So an extent an
// engine still holds must never be written, moved (compaction) or freed
// while it holds it — the render task would be executing erased flash, and
// on a dual-core board the store runs on the OTHER core, in parallel.
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

/// One compaction step: slide an extent down to `mv.to` so the free pages
/// coalesce. The extent is UNPUBLISHED first (which is also what frees the
/// destination when the two ranges overlap), so a power cut anywhere in
/// here costs exactly this one extent — never a torn one handed to the
/// engine — and its pattern re-caches from its chunks on the next
/// activation. Pages are copied one at a time, each an
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
    persist_arena();
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
    if ok {
        raw_invalidate(dst, e.len as usize);
        ok = raw_slice(dst, e.len as usize).is_some_and(|b| fnv1a(b) == e.hash);
    }
    if !ok {
        println!("patterns: arena compaction dropped seq {} (page {} -> {})", e.seq, e.start, mv.to);
        return false;
    }
    let placed =
        ARENA.lock(|c| c.borrow_mut().insert(extents::Extent { start: mv.to, ..e }).is_some());
    persist_arena();
    placed
}

/// Put a stored pattern's bytecode into an arena extent so its next
/// activation executes in place. `may_compact`: a SAVE (a user action) may
/// slide other extents down to open a run; an activation-time fill takes
/// what already fits or gives up — bounded to one write per pattern per
/// library state, so playlist churn is wear-free. Never evicts anything.
/// Best-effort; false leaves the pattern on the chunk path (never a panic).
pub async fn cache_code(id: &str, bc: &[u8], may_compact: bool) -> bool {
    if !arena_on() || bc.is_empty() || bc.len() > MAX_BC {
        return false;
    }
    let Some((seq, gen, _, bc_count, _)) = lookup(id) else {
        return false;
    };
    let region = REGION.load(Ordering::Relaxed);
    if bc_count == 0 || region == 0 || crate::ota::ota_active() {
        return false;
    }
    let Some(_guard) = ArenaGuard::take() else {
        return false; // another save/compaction is in flight
    };
    let hash = fnv1a(bc);
    let need = extents::pages_for(bc.len() as u32);
    let (pin_buf, pin_n) = pins();
    let held = &pin_buf[..pin_n]; // seqs an engine is executing from

    // Reclaim extents the index no longer knows (deleted or superseded
    // patterns), except any an engine is still executing from — it may hold
    // a generation the store has already replaced, or a pattern that has
    // been deleted mid-crossfade. THIS pattern's own stale generation is
    // spared here and freed after the new extent is published, so a power
    // cut never loses both.
    let live = live_gens();
    let swept = ARENA.lock(|c| {
        c.borrow_mut()
            .retain(|e| e.seq == seq || held.contains(&e.seq) || live.contains(&(e.seq, e.gen)))
    });
    if swept > 0 {
        persist_arena();
    }
    // already cached at this generation?
    if ARENA.lock(|c| {
        c.borrow()
            .find(seq, gen)
            .is_some_and(|(_, e)| e.len as usize == bc.len() && e.hash == hash)
    }) {
        return true;
    }

    let mut start_page = ARENA.lock(|c| c.borrow().first_fit(need));
    if start_page.is_none() && may_compact {
        let (reachable, hole, used) = ARENA.lock(|c| {
            let d = c.borrow();
            (d.compacted_free_run(held), d.largest_hole(), d.used_pages())
        });
        if reachable < need {
            println!(
                "patterns: code arena full — {} needs {} pages, {} used, best hole {}",
                id, need, used, hole
            );
            return false;
        }
        println!("patterns: code arena compacting for {} ({} pages)", id, need);
        for _ in 0..2 * extents::MAX_EXTENTS {
            let Some(mv) = ARENA.lock(|c| c.borrow().next_move(held)) else {
                break;
            };
            if !compact_move(mv, region).await {
                break;
            }
            start_page = ARENA.lock(|c| c.borrow().first_fit(need));
            if start_page.is_some() {
                break;
            }
        }
        start_page = start_page.or_else(|| ARENA.lock(|c| c.borrow().first_fit(need)));
    }
    let Some(start_page) = start_page else {
        return false;
    };

    let rel = arena_off(start_page);
    let ok = write_raw(region + rel, bc).await;
    if ok {
        raw_invalidate(rel, bc.len());
    }
    let verified = ok && raw_slice(rel, bc.len()).is_some_and(|b| fnv1a(b) == hash);
    if !verified {
        println!("patterns: code arena write of {} failed (page {})", id, start_page);
        return false;
    }
    let e = extents::Extent { seq, gen, start: start_page, len: bc.len() as u32, hash };
    if !ARENA.lock(|c| c.borrow_mut().insert(e).is_some()) {
        println!("patterns: code arena directory full — {} stays on the chunk path", id);
        return false;
    }
    if !persist_arena() {
        println!("patterns: code arena directory write failed — {} lives this session only", id);
    }
    // published; now the superseded generation may go (unless an engine
    // is still executing from it)
    if !pinned(seq) {
        if let Some((i, _)) = ARENA.lock(|c| c.borrow().find_other_gen(seq, gen)) {
            ARENA.lock(|c| {
                c.borrow_mut().remove(i);
            });
            persist_arena();
        }
    }
    println!("patterns: code arena page {} <- {} ({} B)", start_page, id, bc.len());
    true
}

/// Forget every extent of `seq` (delete). A pattern an engine is still
/// executing from KEEPS its extent: dropping it here would hand its pages
/// to the next save, which would erase them under the running VM. The
/// pattern is gone from the index, so `cache_code`'s sweep reclaims the
/// extent as soon as the engine lets go of it.
fn arena_forget(seq: u32) {
    if pinned(seq) {
        println!("patterns: extent of {} kept — an engine is executing it", id_hex(seq));
        return;
    }
    let n = ARENA.lock(|c| c.borrow_mut().remove_seq(seq));
    if n > 0 {
        persist_arena();
    }
}

/// (arena pages in use, arena pages total) for /api/status. `[0, 0]` means
/// the arena is off (no mapping) — there is no other way to see a zero
/// total.
pub fn arena_stats() -> (u32, u32) {
    let (used, total) = ARENA.lock(|c| {
        let d = c.borrow();
        (d.used_pages(), d.total_pages())
    });
    (used as u32, total as u32)
}
/// The running pattern's executable bytes as mapped memory, per the VM
/// consumer contract (docs/research/flash-mmap.md): rodata for the built-in
/// default (the bootloader's own mapping), the ad-hoc slot side the last
/// store_current wrote, or the library pattern's arena slot. None = read it
/// through a Vec (read_current_bc / bytecode_of) — the fallback when the
/// mapping is absent or the pattern has no slot.
pub fn current_code() -> Option<&'static [u8]> {
    use crate::shared::BcLoc;
    match crate::shared::current_bc() {
        BcLoc::Default(b) => Some(b),
        BcLoc::Flash(len) => current_slot_code(len),
        BcLoc::Library(_) => code_of(&crate::shared::get_current_pattern_id()),
        BcLoc::Gone => None,
    }
}
