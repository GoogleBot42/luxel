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
use core::cell::RefCell;
use core::ops::Range;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_futures::block_on;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;
use esp_storage::FlashStorage;
use luxel_core::jsonview::{json_escape, push_hex, push_piece, push_u32};
use sequential_storage::cache::NoCache;
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

struct AsyncFlash<'a>(&'a mut FlashStorage<'static>);

impl anf::ErrorType for AsyncFlash<'_> {
    type Error = <FlashStorage<'static> as BlockingErrorType>::Error;
}
impl anf::ReadNorFlash for AsyncFlash<'_> {
    const READ_SIZE: usize = <FlashStorage<'static> as BlockingRead>::READ_SIZE;
    // Every op is fenced individually (dual-core: the other core parks for
    // the op — core1.rs): this adapter drives a LEASED driver, outside
    // ota::with_flash's fence.
    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        crate::core1::fenced(|| BlockingRead::read(self.0, offset, bytes))
    }
    fn capacity(&self) -> usize {
        BlockingRead::capacity(self.0)
    }
}
impl anf::NorFlash for AsyncFlash<'_> {
    const WRITE_SIZE: usize = <FlashStorage<'static> as BlockingNorFlash>::WRITE_SIZE;
    const ERASE_SIZE: usize = <FlashStorage<'static> as BlockingNorFlash>::ERASE_SIZE;
    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        crate::core1::fenced(|| BlockingNorFlash::erase(self.0, from, to))
    }
    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        crate::core1::fenced(|| BlockingNorFlash::write(self.0, offset, bytes))
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
                let mut $af = AsyncFlash(flash);
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
    let mut cache = NoCache::new();
    match map::fetch_item::<u32, &[u8], _>(af, range, &mut cache, buf, &meta_key(seq)).await {
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
    let mut cache = NoCache::new();
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
        match map::fetch_item::<u32, &[u8], _>(af, range.clone(), &mut cache, buf, &key).await {
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
    let mut cache = NoCache::new();
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
        match map::fetch_item::<u32, &[u8], _>(af, range.clone(), &mut cache, buf, &key).await {
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
    let mut cache = NoCache::new();
    let bytes = source.as_bytes();
    for c in 0..count {
        let s = c as usize * CHUNK;
        let e = (s + CHUNK).min(bytes.len());
        let chunk: &[u8] = &bytes[s..e];
        let key = chunk_key(seq, gen, c);
        if map::store_item(af, range.clone(), &mut cache, buf, &key, &chunk).await.is_err() {
            return Err(());
        }
    }
    for c in 0..bc_count {
        let s = c as usize * CHUNK;
        let e = (s + CHUNK).min(bc.len());
        let chunk: &[u8] = &bc[s..e];
        let key = bc_chunk_key(seq, gen, c);
        if map::store_item(af, range.clone(), &mut cache, buf, &key, &chunk).await.is_err() {
            return Err(());
        }
    }
    let meta = serialize_meta(gen, count, bc_count, name);
    let mslice: &[u8] = &meta;
    if map::store_item(af, range.clone(), &mut cache, buf, &meta_key(seq), &mslice).await.is_err() {
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
    let mut cache = NoCache::new();
    for c in 0..count {
        let key = chunk_key(seq, gen, c);
        let _ = map::remove_item::<u32, _>(af, range.clone(), &mut cache, buf, &key).await;
    }
    for c in 0..bc_count {
        let key = bc_chunk_key(seq, gen, c);
        let _ = map::remove_item::<u32, _>(af, range.clone(), &mut cache, buf, &key).await;
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
        let mut cache = NoCache::new();
        let fmt = match map::fetch_item::<u32, &[u8], _>(
            &mut af, range.clone(), &mut cache, buf, &FORMAT_KEY,
        )
        .await
        {
            Ok(Some(b)) if b.len() == 4 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            _ => 0,
        };
        if fmt != FORMAT_VERSION {
            println!("patterns: format {} != {}, wiping storage", fmt, FORMAT_VERSION);
            let _ = anf::NorFlash::erase(&mut af, range.start, range.end).await;
            let ver = FORMAT_VERSION.to_le_bytes();
            let vslice: &[u8] = &ver;
            let mut c2 = NoCache::new();
            let _ = map::store_item(&mut af, range.clone(), &mut c2, buf, &FORMAT_KEY, &vslice).await;
            return Vec::new();
        }

        // Discover meta seqs by scanning, then read each authoritative meta.
        let mut seqs: Vec<u32> = Vec::new();
        let mut cache = NoCache::new();
        if let Ok(mut iter) =
            map::fetch_all_items::<u32, _, _>(&mut af, range.clone(), &mut cache, buf).await
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
        let mut cache = NoCache::new();
        let b: &[u8] = bytes;
        map::store_item(&mut af, range.clone(), &mut cache, buf, &key, &b)
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
        let mut cache = NoCache::new();
        match map::fetch_item::<u32, &[u8], _>(&mut af, range, &mut cache, buf, &key).await {
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
//   ARENA_OFF        the library code arena: ARENA_SLOTS page-aligned slots
//                    of ARENA_SLOT bytes, one stored pattern's bytecode
//                    each, contiguous and 4-byte aligned for the VM; the
//                    slot table lives under ARENA_KEY in the map
const RAW_OFF: u32 = STORE_LEN;
const RAW_LEN: u32 = PAT_LEN - STORE_LEN;
const CUR_OFF: u32 = RAW_OFF;
const PAGE: u32 = 4096;
const CUR_SRC_MAX: u32 = 24 * PAGE; // 96 KiB, past every upload cap
const CUR_BC_MAX: u32 = 16 * PAGE; // 64 KiB per side
const CUR_SRC_OFF: u32 = CUR_OFF + PAGE;
const CUR_BC_OFF: u32 = CUR_SRC_OFF + CUR_SRC_MAX;
const CUR_MAGIC: u32 = 0x4C58_4350; // "LXCP"
/// The bc side holding the RUNNING ad-hoc blob (the last successful
/// store_current); the next store writes the other one.
static CUR_BC_SIDE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
const ARENA_OFF: u32 = CUR_BC_OFF + 2 * CUR_BC_MAX;
const ARENA_SLOT: u32 = 10 * PAGE; // 40 KiB, >= MAX_BC
const ARENA_SLOTS: usize = 7;
/// Arena slot table (reserved-key map item, see the blobs above).
const ARENA_KEY: u32 = 0x7FFF_FFF9;
const _: () = assert!(ARENA_OFF + ARENA_SLOTS as u32 * ARENA_SLOT <= RAW_OFF + RAW_LEN);
const _: () = assert!(ARENA_SLOT as usize >= MAX_BC);

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
        let r = crate::ota::with_flash(|f| BlockingNorFlash::erase(f, at, at + PAGE).is_ok());
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
        let r = crate::ota::with_flash(|f| {
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
        let mut cache = NoCache::new();
        let meta_ok =
            map::remove_item::<u32, _>(&mut af, range.clone(), &mut cache, buf, &meta_key(seq))
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
// One stored pattern's LXBC per slot, contiguous and page-aligned, so a
// library activation hands the engine a mapped slice instead of
// reassembling ≤3,840-byte chunk items into a Vec. Filled by save() (may
// evict the least-recently-activated slot — a user action) and by an
// activation that found no slot (free or stale slots only: playlist churn
// writes flash at most ARENA_SLOTS times per library state, then never —
// the 2026-08-15 wear rule). The running pattern's slot is never written.
// The table (seq, bc generation, length, FNV-1a of the bytes per slot) is
// a reserved-key map item; at boot every entry is checked against the
// index AND the mapped bytes, so a torn write or a stale generation is
// dropped, never executed.

#[derive(Clone, Copy)]
struct Slot {
    seq: u32,
    gen: u8,
    /// 0 = empty
    len: u32,
    hash: u32,
    /// activation tick, for LRU (session-local)
    last_use: u32,
}

const EMPTY: Slot = Slot { seq: 0, gen: 0, len: 0, hash: 0, last_use: 0 };

static ARENA: BlockingMutex<CriticalSectionRawMutex, RefCell<[Slot; ARENA_SLOTS]>> =
    BlockingMutex::new(RefCell::new([EMPTY; ARENA_SLOTS]));
static ARENA_TICK: AtomicU32 = AtomicU32::new(1);

/// Next LRU tick. load+store, not fetch_add: the C3's riscv32imc has no
/// atomic RMW and every caller runs on the one executor thread anyway.
fn arena_tick() -> u32 {
    let t = ARENA_TICK.load(Ordering::Relaxed).wrapping_add(1);
    ARENA_TICK.store(t, Ordering::Relaxed);
    t
}

/// The arena only exists through the mapping — that is its whole point.
fn arena_on() -> bool {
    raw().is_some()
}

fn slot_off(i: usize) -> u32 {
    ARENA_OFF + i as u32 * ARENA_SLOT
}

fn slot_bytes(i: usize, len: u32) -> Option<&'static [u8]> {
    raw_slice(slot_off(i), len as usize)
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

const ARENA_TABLE_VER: u8 = 1;

fn serialize_arena(slots: &[Slot; ARENA_SLOTS]) -> Vec<u8> {
    let mut v = Vec::with_capacity(2 + ARENA_SLOTS * 13);
    v.push(ARENA_TABLE_VER);
    v.push(ARENA_SLOTS as u8);
    for s in slots {
        v.extend_from_slice(&s.seq.to_le_bytes());
        v.push(s.gen);
        v.extend_from_slice(&s.len.to_le_bytes());
        v.extend_from_slice(&s.hash.to_le_bytes());
    }
    v
}

fn deserialize_arena(b: &[u8]) -> Option<[Slot; ARENA_SLOTS]> {
    if b.len() < 2 || b[0] != ARENA_TABLE_VER {
        return None;
    }
    let n = (b[1] as usize).min(ARENA_SLOTS);
    if b.len() < 2 + n * 13 {
        return None;
    }
    let mut out = [EMPTY; ARENA_SLOTS];
    for (i, s) in out.iter_mut().enumerate().take(n) {
        let r = &b[2 + i * 13..2 + i * 13 + 13];
        *s = Slot {
            seq: u32::from_le_bytes([r[0], r[1], r[2], r[3]]),
            gen: r[4],
            len: u32::from_le_bytes([r[5], r[6], r[7], r[8]]),
            hash: u32::from_le_bytes([r[9], r[10], r[11], r[12]]),
            last_use: 0,
        };
    }
    Some(out)
}

fn persist_arena() -> bool {
    let slots = ARENA.lock(|c| *c.borrow());
    store_blob(ARENA_KEY, &serialize_arena(&slots))
}

/// Boot: load the slot table and keep only entries the index and the mapped
/// bytes both vouch for.
#[inline(never)]
fn arena_init() {
    if !arena_on() {
        println!("patterns: code arena off (raw half not mapped)");
        return;
    }
    let mut slots = read_blob(ARENA_KEY)
        .and_then(|b| deserialize_arena(&b))
        .unwrap_or([EMPTY; ARENA_SLOTS]);
    let (mut valid, mut dropped) = (0u32, 0u32);
    for (i, s) in slots.iter_mut().enumerate() {
        if s.len == 0 {
            continue;
        }
        let indexed = INDEX.lock(|c| {
            c.borrow().iter().any(|e| e.seq == s.seq && e.gen == s.gen && e.bc_count > 0)
        });
        let ok = s.len <= ARENA_SLOT
            && indexed
            && slot_bytes(i, s.len).is_some_and(|b| fnv1a(b) == s.hash);
        if ok {
            valid += 1;
        } else {
            *s = EMPTY;
            dropped += 1;
        }
    }
    ARENA.lock(|c| *c.borrow_mut() = slots);
    println!(
        "patterns: code arena {}/{} slots valid ({} dropped), {} KiB each",
        valid,
        ARENA_SLOTS,
        dropped,
        ARENA_SLOT / 1024
    );
    if dropped > 0 {
        persist_arena();
    }
}

/// A stored pattern's executable bytes, mapped — if an arena slot holds its
/// CURRENT bytecode generation. Bumps the slot's LRU tick. Valid until the
/// store overwrites the slot, which it never does for the running pattern.
pub fn code_of(id: &str) -> Option<&'static [u8]> {
    if !arena_on() {
        return None;
    }
    let (seq, gen, _, bc_count, _) = lookup(id)?;
    if bc_count == 0 {
        return None;
    }
    let tick = arena_tick();
    let (i, len) = ARENA.lock(|c| {
        let mut a = c.borrow_mut();
        let i = a.iter().position(|s| s.len > 0 && s.seq == seq && s.gen == gen)?;
        a[i].last_use = tick;
        Some((i, a[i].len))
    })?;
    slot_bytes(i, len)
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
        let mut cache = NoCache::new();
        let mut len = 0usize;
        let mut h = FNV_INIT;
        for c in 0..count {
            let key = chunk_key(seq, gen, c);
            match map::fetch_item::<u32, &[u8], _>(&mut af, range.clone(), &mut cache, buf, &key)
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

/// The running pattern's seq — its slot is never written or evicted.
fn running_seq() -> Option<u32> {
    seq_of(&crate::shared::get_current_pattern_id())
}

/// Put a stored pattern's bytecode into an arena slot so its next
/// activation executes in place. `evict`: may reclaim the least-recently-
/// activated slot (save(): a user action). `!evict`: free or stale slots
/// only — an activation-time fill, bounded by ARENA_SLOTS writes per
/// library state, so playlist churn is wear-free. Best-effort; false
/// leaves the pattern on the chunk path (never a panic).
pub async fn cache_code(id: &str, bc: &[u8], evict: bool) -> bool {
    if !arena_on() || bc.is_empty() || bc.len() > ARENA_SLOT as usize {
        return false;
    }
    let Some((seq, gen, _, bc_count, _)) = lookup(id) else {
        return false;
    };
    let start = REGION.load(Ordering::Relaxed);
    if bc_count == 0 || start == 0 || crate::ota::ota_active() {
        return false;
    }
    let hash = fnv1a(bc);
    let running = running_seq();
    let tick = arena_tick();
    let live: Vec<(u32, u8)> =
        INDEX.lock(|c| c.borrow().iter().map(|e| (e.seq, e.gen)).collect());
    let pick: Result<usize, bool> = ARENA.lock(|c| {
        let mut a = c.borrow_mut();
        if a.iter().any(|s| {
            s.len as usize == bc.len() && s.seq == seq && s.gen == gen && s.hash == hash
        }) {
            return Err(true); // already cached
        }
        let not_running = |s: &Slot| Some(s.seq) != running;
        let stale = |s: &Slot| s.len > 0 && !live.contains(&(s.seq, s.gen));
        let i = a
            .iter()
            .position(|s| s.len == 0)
            .or_else(|| a.iter().position(|s| stale(s) && not_running(s)))
            .or_else(|| {
                if !evict {
                    return None;
                }
                a.iter()
                    .enumerate()
                    .filter(|(_, s)| not_running(s))
                    .min_by_key(|(_, s)| s.last_use)
                    .map(|(i, _)| i)
            });
        let Some(i) = i else {
            return Err(false);
        };
        a[i] = EMPTY; // unpublish before the bytes change under it
        Ok(i)
    });
    let i = match pick {
        Ok(i) => i,
        Err(r) => return r,
    };
    let rel = slot_off(i);
    let ok = write_raw(start + rel, bc).await;
    if ok {
        raw_invalidate(rel, bc.len());
    }
    let verified = ok && slot_bytes(i, bc.len() as u32).is_some_and(|b| fnv1a(b) == hash);
    if !verified {
        println!("patterns: code arena write of {} failed (slot {})", id, i);
        return false;
    }
    ARENA.lock(|c| {
        c.borrow_mut()[i] = Slot { seq, gen, len: bc.len() as u32, hash, last_use: tick }
    });
    if !persist_arena() {
        println!("patterns: code arena table write failed — slot {} lives this session only", i);
    }
    println!("patterns: code arena slot {} <- {} ({} B)", i, id, bc.len());
    true
}

/// Forget any slot holding `seq` (delete).
fn arena_forget(seq: u32) {
    let changed = ARENA.lock(|c| {
        let mut a = c.borrow_mut();
        let mut changed = false;
        for s in a.iter_mut() {
            if s.len > 0 && s.seq == seq {
                *s = EMPTY;
                changed = true;
            }
        }
        changed
    });
    if changed {
        persist_arena();
    }
}

/// (slots in use, slots total) for /api/status.
pub fn arena_stats() -> (u32, u32) {
    let used = ARENA.lock(|c| c.borrow().iter().filter(|s| s.len > 0).count());
    (used as u32, ARENA_SLOTS as u32)
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
