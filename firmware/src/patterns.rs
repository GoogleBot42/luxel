//! On-device pattern library — the firmware half of the `/api/patterns`
//! CRUD contract the native mirror (crates/luxel-cli/src/serve.rs) and the
//! playground already speak.
//!
//! # Storage: `sequential-storage` over the `storage` partition
//!
//! An established, power-loss-safe, wear-leveled key→value store over NOR
//! flash (PLAN.md's storage model) rather than a hand-rolled blob. Each
//! pattern is ONE map item: key = the u32 id, value = `[name_len:u8][name]
//! [source]`. A store/remove is atomic at the item level, so a power loss
//! mid-save loses at most the one in-flight pattern, never the library.
//!
//! The `storage` partition (0x210000, 1 MB) was freed by dropping factory
//! (partitions.csv). We resolve it from the *live* partition table at boot
//! and disable the store if it is absent — so this firmware is safe even on
//! the old table, where 0x210000 is still the live ota_1 app slot.
//!
//! ## One-page item limit
//!
//! sequential-storage items must fit a single flash page (erase sector = 4
//! KiB here → ~4 KB usable), so a single pattern's source is capped near 3.5
//! KB ([MAX_SOURCE]). That covers the great majority of patterns; larger
//! ones stay in the browser library. (Chunking across keys could lift this
//! later if needed.)
//!
//! ## Flash access
//!
//! sequential-storage is async; esp-storage's `FlashStorage` is blocking, so
//! we wrap it in `BlockingAsync` and drive each op to completion with
//! `block_on` — the adapter never truly pends, so this just blocks the
//! executor for the (short) duration like any synchronous flash work. We
//! *take* the driver out of the OTA module for the transaction (never
//! holding its critical-section mutex across the erases) and return it via a
//! Drop guard.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_futures::block_on;
use esp_println::println;
use esp_storage::FlashStorage;
use luxel_core::jsonview::json_escape;
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
    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        BlockingRead::read(self.0, offset, bytes)
    }
    fn capacity(&self) -> usize {
        BlockingRead::capacity(self.0)
    }
}
impl anf::NorFlash for AsyncFlash<'_> {
    const WRITE_SIZE: usize = <FlashStorage<'static> as BlockingNorFlash>::WRITE_SIZE;
    const ERASE_SIZE: usize = <FlashStorage<'static> as BlockingNorFlash>::ERASE_SIZE;
    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        BlockingNorFlash::erase(self.0, from, to)
    }
    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        BlockingNorFlash::write(self.0, offset, bytes)
    }
}
impl anf::MultiwriteNorFlash for AsyncFlash<'_> {}

/// The `storage` partition — ours exclusively (partitions.csv). Expected
/// bounds; the actual region is resolved from the live table at boot.
pub const PAT_START: u32 = 0x21_0000;
pub const PAT_LEN: u32 = 0x10_0000;

/// Flash range sequential-storage actually manages: the first 256 KiB of
/// the partition. With NoCache each op scans the managed pages, so we don't
/// hand it the whole 1 MiB (256 pages) — 64 pages comfortably hold 24
/// max-size patterns plus wear-leveling headroom, and keep scans cheap. The
/// rest of the partition stays reserved for future growth.
const STORE_LEN: u32 = 0x4_0000;

/// Resolved flash offset of the `storage` partition, or 0 if absent (old
/// table) — every op then refuses / reads empty. Set once in [init].
static REGION: AtomicU32 = AtomicU32::new(0);
/// Next id sequence; ids are `seq ^ ID_MASK` (mirrors serve.rs so the two
/// backends mint interchangeable-looking ids). Seeded at boot from stored ids.
static NEXT_SEQ: AtomicU32 = AtomicU32::new(0);
const ID_MASK: u32 = 0x5eed_1e55;

const MAX_NAME: usize = 64;
/// Fits one 4 KiB flash page alongside the name + key + item header.
const MAX_SOURCE: usize = 3584;
const MAX_PATTERNS: usize = 24;
/// Scratch buffer size for sequential-storage: ≥ any single item (≤ one page).
const BUF: usize = 4096;

fn id_hex(id: u32) -> String {
    alloc::format!("{:08x}", id)
}

fn parse_id(id: &str) -> Option<u32> {
    u32::from_str_radix(id, 16).ok()
}

/// value bytes: `[name_len:u8][name][source]`
fn deserialize_value(bytes: &[u8]) -> Option<(&str, &str)> {
    let nlen = *bytes.first()? as usize;
    if bytes.len() < 1 + nlen {
        return None;
    }
    let name = core::str::from_utf8(&bytes[1..1 + nlen]).ok()?;
    let source = core::str::from_utf8(&bytes[1 + nlen..]).ok()?;
    Some((name, source))
}

/// Leases the flash driver out of the OTA module and returns it on drop
/// (panic/normal), so a taken driver is never lost.
struct FlashLease(Option<FlashStorage<'static>>);
impl Drop for FlashLease {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            crate::ota::give_flash(f);
        }
    }
}

/// Run a sequential-storage transaction: lease flash, wrap it async, hand
/// the closure the async flash + the partition range + a scratch buffer,
/// drive it to completion. `None` if an OTA currently owns the flash.
///
/// `$body` is an async block (it `.await`s the map ops). It must copy any
/// borrowed value out to owned data before returning (the buffer is dropped
/// with the lease).
macro_rules! with_store {
    ($start:expr, |$af:ident, $range:ident, $buf:ident| $body:block) => {{
        let mut lease = FlashLease(crate::ota::take_flash());
        match lease.0.as_mut() {
            None => None,
            Some(flash) => {
                #[allow(unused_mut)]
                let mut $af = AsyncFlash(flash);
                let $range: Range<u32> = $start..($start + STORE_LEN);
                let mut $buf = alloc::vec![0u8; BUF];
                let $buf = $buf.as_mut_slice();
                Some(block_on(async move { $body }))
            }
        }
    }};
}

/// Collect every stored pattern's (id, name) — sources skipped. Used by list,
/// upsert-by-name, and the boot scan.
///
/// `fetch_all_items` yields *raw* items, including superseded versions from
/// an upsert (a re-`store_item` of the same key appends; the old copy lives
/// until GC). It does skip removed keys. So we dedup by key here — the
/// (id, name) mapping is a stable bijection (a new name always mints a new
/// id; upsert reuses the id), so keeping the first-seen name per key is
/// correct. `fetch_item` already returns the latest value for reads.
async fn collect_index(
    af: &mut AsyncFlash<'_>,
    range: Range<u32>,
    buf: &mut [u8],
) -> Vec<(u32, String)> {
    let mut cache = NoCache::new();
    let mut out: Vec<(u32, String)> = Vec::new();
    let Ok(mut iter) = map::fetch_all_items::<u32, _, _>(af, range, &mut cache, buf).await else {
        return out;
    };
    while let Ok(Some((key, val))) = iter.next::<u32, &[u8]>(buf).await {
        if out.iter().any(|(k, _)| *k == key) {
            continue; // superseded duplicate
        }
        if let Some((name, _)) = deserialize_value(val) {
            out.push((key, String::from(name)));
        }
    }
    out
}

/// Resolve the `storage` partition and seed the id sequence. Disables the
/// store (REGION = 0) if the partition is absent — never aims writes at a
/// live app slot on the old table.
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

    // seed NEXT_SEQ past the highest stored id
    let index = with_store!(start, |af, range, buf| { collect_index(&mut af, range, buf).await })
        .unwrap_or_default();
    let mut next = 0u32;
    for (id, _) in &index {
        next = next.max((id ^ ID_MASK).wrapping_add(1));
    }
    NEXT_SEQ.store(next, Ordering::Relaxed);
    println!("patterns: {} stored (storage @ {:#x})", index.len(), start);
}

/// `GET /api/patterns` → `{"patterns":[{"id","name"},…]}`
pub fn list_json() -> String {
    let start = REGION.load(Ordering::Relaxed);
    let index = if start == 0 {
        Vec::new()
    } else {
        with_store!(start, |af, range, buf| { collect_index(&mut af, range, buf).await })
            .unwrap_or_default()
    };
    let items: Vec<String> = index
        .iter()
        .map(|(id, name)| {
            alloc::format!("{{\"id\":\"{}\",\"name\":\"{}\"}}", id_hex(*id), json_escape(name))
        })
        .collect();
    alloc::format!("{{\"patterns\":[{}]}}", items.join(","))
}

/// Fetch one pattern's (name, source) from flash by id.
fn fetch(id: u32) -> Option<(String, String)> {
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return None;
    }
    with_store!(start, |af, range, buf| {
        let mut cache = NoCache::new();
        match map::fetch_item::<u32, &[u8], _>(&mut af, range, &mut cache, buf, &id).await {
            Ok(Some(bytes)) => {
                deserialize_value(bytes).map(|(n, s)| (String::from(n), String::from(s)))
            }
            _ => None,
        }
    })
    .flatten()
}

/// `GET /api/patterns/<id>` → `{"id","name","source"}` | None (→ mirror's
/// 200 + `{"ok":false,…}` is applied by the caller).
pub fn get_json(id: &str) -> Option<String> {
    let key = parse_id(id)?;
    let (name, source) = fetch(key)?;
    Some(alloc::format!(
        "{{\"id\":\"{}\",\"name\":\"{}\",\"source\":\"{}\"}}",
        id,
        json_escape(&name),
        json_escape(&source)
    ))
}

/// Read a stored pattern's source (for activation).
pub fn source_of(id: &str) -> Option<String> {
    fetch(parse_id(id)?).map(|(_, s)| s)
}

/// `POST /api/patterns` body `"name\nsource"` → `{"ok":true,"id"}`.
/// Upserts by name (matches the mirror). Caller compile-checks first.
pub fn save(name: &str, source: &str) -> String {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_NAME {
        return String::from("{\"ok\":false,\"error\":\"name must be 1..=64 bytes\"}");
    }
    if source.is_empty() || source.len() > MAX_SOURCE {
        return alloc::format!(
            "{{\"ok\":false,\"error\":\"source must be 1..={} bytes (larger patterns stay in the browser library)\"}}",
            MAX_SOURCE
        );
    }
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return String::from(
            "{\"ok\":false,\"error\":\"pattern storage unavailable (device needs reflash)\"}",
        );
    }

    let outcome = with_store!(start, |af, range, buf| {
        // upsert by name: reuse an existing id, else mint one
        let index = collect_index(&mut af, range.clone(), buf).await;
        let existing = index.iter().find(|(_, n)| n == name).map(|(id, _)| *id);
        if existing.is_none() && index.len() >= MAX_PATTERNS {
            return Err(String::from("library full"));
        }
        // No fetch_add: riscv32imc (esp32c3) lacks atomic RMW. This runs
        // inside block_on with the flash leased (executor blocked, single
        // owner), so a plain load/store increment has no race.
        let id = match existing {
            Some(id) => id,
            None => {
                let seq = NEXT_SEQ.load(Ordering::Relaxed);
                NEXT_SEQ.store(seq.wrapping_add(1), Ordering::Relaxed);
                seq ^ ID_MASK
            }
        };

        let mut val: Vec<u8> = Vec::with_capacity(1 + name.len() + source.len());
        val.push(name.len() as u8);
        val.extend_from_slice(name.as_bytes());
        val.extend_from_slice(source.as_bytes());
        let vslice: &[u8] = &val;

        let mut cache = NoCache::new();
        match map::store_item(&mut af, range, &mut cache, buf, &id, &vslice).await {
            Ok(()) => Ok(id),
            Err(_) => Err(String::from("flash write failed")),
        }
    });

    match outcome {
        None => String::from("{\"ok\":false,\"error\":\"busy (update in progress)\"}"),
        Some(Ok(id)) => alloc::format!("{{\"ok\":true,\"id\":\"{}\"}}", id_hex(id)),
        Some(Err(e)) => alloc::format!("{{\"ok\":false,\"error\":\"{}\"}}", json_escape(&e)),
    }
}

/// `DELETE /api/patterns/<id>` → `{"ok":true}` | `{"ok":false,…}`.
pub fn delete(id: &str) -> String {
    let Some(key) = parse_id(id) else {
        return String::from("{\"ok\":false,\"error\":\"no such pattern\"}");
    };
    let start = REGION.load(Ordering::Relaxed);
    if start == 0 {
        return String::from("{\"ok\":false,\"error\":\"no such pattern\"}");
    }

    let outcome = with_store!(start, |af, range, buf| {
        let mut cache = NoCache::new();
        // distinguish missing (→ "no such pattern") from present, to match
        // the mirror — remove_item alone succeeds even on an absent key.
        match map::fetch_item::<u32, &[u8], _>(&mut af, range.clone(), &mut cache, buf, &key).await {
            Ok(Some(_)) => {}
            Ok(None) => return Ok(false),
            Err(_) => return Err(()),
        }
        match map::remove_item::<u32, _>(&mut af, range, &mut cache, buf, &key).await {
            Ok(()) => Ok(true),
            Err(_) => Err(()),
        }
    });

    match outcome {
        None => String::from("{\"ok\":false,\"error\":\"busy (update in progress)\"}"),
        Some(Ok(true)) => String::from("{\"ok\":true}"),
        Some(Ok(false)) => String::from("{\"ok\":false,\"error\":\"no such pattern\"}"),
        Some(Err(())) => String::from("{\"ok\":false,\"error\":\"flash error\"}"),
    }
}
