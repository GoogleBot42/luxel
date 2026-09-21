//! Partition-table plumbing: the raw on-flash table, and the primitives
//! that rewrite it.
//!
//! Two callers, one mechanism (Gitea #501):
//!
//! * [crate::takeover] (`wled-takeover` boards) — boot under WLED's
//!   *foreign* table, copy ourselves into what will become ota_0, wipe
//!   WLED's config, install our table.
//! * [crate::migrate] (every board) — boot under an *older Luxel* table and
//!   move to the one this image embeds, carrying the pattern store across.
//!
//! Before #501 all of this lived inside takeover.rs, which meant the two
//! boards that do not ship the WLED installer (`board-pixelblaze-v3`,
//! `board-seengreat-hub75`) carried no table-writing code at all — and
//! those are exactly the boards that still need to migrate. So the
//! WLED-specific half (littlefs, credential inheritance, the config wipe)
//! stays behind `wled-takeover` and everything a table rewrite needs is
//! here, always built.
//!
//! Nothing in this module knows a partition offset: every address comes
//! either from the table on flash or from [EMBEDDED], the table this image
//! was built with. `tools/ci.sh` greps the tree for literal partition
//! offsets, so keep it that way.

use alloc::vec::Vec;
use embedded_storage::nor_flash::NorFlash;
use esp_println::println;

/// The partition table this image was built with — entries plus the
/// trailing MD5 row the bootloader verifies, i.e. the exact bytes espflash
/// would write at [TABLE_OFFSET]. `build.rs` serializes it from the board's
/// csv (`firmware/partitions.csv`, or `partitions-16mb.csv` on a 16 MB
/// board) with the same crate espflash uses, so the bytes match.
pub const EMBEDDED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/partition-table.bin"));

/// Which csv [EMBEDDED] came from, for the boot log and `/api/status`.
pub const TABLE_NAME: &str = match option_env!("LUXEL_PARTITIONS") {
    Some(n) => n,
    None => "partitions.csv",
};

/// Where the second-stage bootloader reads the table from. Fixed by the
/// ESP-IDF bootloader, not by our layout — not a partition offset.
pub const TABLE_OFFSET: u32 = 0x8000;
/// NOR erase unit.
pub const SECTOR: u32 = 4096;

mod raw;

// Re-exported so the rest of the tree says `parttab::…` and never has to know
// the codec is a separate file. Which of these are used depends on the board's
// feature set (`wled-takeover` brings TYPE_DATA in, for instance), so the
// allow covers the configurations that do not.
#[allow(unused_imports)]
pub use raw::{
    app_entries, app_slot, data_labelled, entries, flash_needed, is_luxel, Part, SUBTYPE_OTA0,
    SUBTYPE_OTA1, TYPE_APP, TYPE_DATA,
};

/// Read the live table off flash, as many bytes as [EMBEDDED] is long.
/// None on a read failure (never on a *different* table — that is the
/// interesting case and it comes back as bytes).
pub fn live_table() -> Option<Vec<u8>> {
    let mut live = alloc::vec![0u8; EMBEDDED.len()];
    crate::assets::read_chunk(TABLE_OFFSET, &mut live).then_some(live)
}

/// Does the table on flash already equal the one this image embeds?
/// Cached: `/api/status` and the OTA path both ask, and the answer cannot
/// change without a reboot (both writers reboot immediately).
pub fn matches_flash() -> bool {
    use core::sync::atomic::{AtomicU8, Ordering};
    static CACHE: AtomicU8 = AtomicU8::new(0); // 0 unknown, 1 yes, 2 no
    match CACHE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let same = live_table().map(|l| l == EMBEDDED).unwrap_or(false);
            CACHE.store(if same { 1 } else { 2 }, Ordering::Relaxed);
            same
        }
    }
}

/// Flash-size preflight: refuse to install a table this chip cannot back.
pub fn flash_fits(table: &[u8]) -> bool {
    let needed = flash_needed(table);
    let cap = crate::ota::with_flash(|f| f.capacity() as u32).unwrap_or(0);
    if cap < needed {
        println!(
            "partitions: flash too small ({} B < {} B needed) — refusing to repartition",
            cap, needed
        );
        return false;
    }
    true
}

pub fn erase_sector(at: u32) -> bool {
    crate::ota::with_flash(|f| NorFlash::erase(f, at, at + SECTOR).is_ok()).unwrap_or(false)
}

/// Erase `[from, to)`, sector by sector. False on the first failure.
pub fn erase_range(from: u32, to: u32) -> bool {
    let mut at = from;
    while at < to {
        if !erase_sector(at) {
            println!("partitions: erase failed at {:#x}", at);
            return false;
        }
        at += SECTOR;
    }
    true
}

/// Write `bytes` at `at`, staged through a word-aligned heap buffer (see
/// config.rs: unaligned paths are off limits). The sector must already be
/// erased.
pub fn write_aligned(at: u32, bytes: &[u8]) -> bool {
    let mut stage = alloc::vec![0u32; bytes.len().div_ceil(4)];
    let stage_bytes = unsafe {
        core::slice::from_raw_parts_mut(stage.as_mut_ptr().cast::<u8>(), stage.len() * 4)
    };
    stage_bytes[..bytes.len()].copy_from_slice(bytes);
    stage_bytes[bytes.len()..].fill(0xFF);
    crate::ota::with_flash(|f| NorFlash::write(f, at, stage_bytes).is_ok()).unwrap_or(false)
}

/// Erase one sector, write `data` (≤ [SECTOR] bytes) into it and read it
/// back. One in-call retry, because a single flaky program op on a boot is
/// a thing that happens (issue #35) and re-erasing costs milliseconds.
///
/// On failure the log says WHICH op failed (erase / write / read-back /
/// data mismatch, and for a mismatch whether the sector read back as
/// still-erased 0xFF or as stale old data) — the 2026-08-16 bench flake
/// (issue #35) printed only "verify failed" and left the failing stage
/// unknowable after the fact.
pub fn write_sector_verified(at: u32, data: &[u8], tag: &str) -> bool {
    let mut check = alloc::vec![0u8; data.len()];
    for attempt in 0..2 {
        let stage = if !erase_sector(at) {
            "erase op failed"
        } else if !write_aligned(at, data) {
            "write op failed"
        } else if !crate::assets::read_chunk(at, &mut check) {
            "read-back op failed"
        } else if check != data {
            let diffs = data.iter().zip(&check).filter(|(a, b)| a != b).count();
            let first = data.iter().zip(&check).position(|(a, b)| a != b).unwrap_or(0);
            println!(
                "{}:   mismatch detail: {} of {} bytes differ, first at +{:#x} (wrote {:#04x}, read {:#04x}){}",
                tag,
                diffs,
                data.len(),
                first,
                data[first],
                check[first],
                if check.iter().all(|b| *b == 0xFF) {
                    " — read back still erased"
                } else {
                    ""
                }
            );
            "data mismatch"
        } else {
            return true;
        };
        println!("{}:   sector {:#x} attempt {}: {}", tag, at, attempt + 1, stage);
    }
    false
}

/// Copy `len` bytes from `src` to `dst` sector by sector with read-back
/// verification. Runs pre-WiFi on the main task; buffers live on the heap.
/// The caller has already checked that the ranges do not overlap.
pub fn copy_region(src: u32, dst: u32, len: u32, tag: &str) -> bool {
    let sectors = len.div_ceil(SECTOR);
    let mut buf = alloc::vec![0u8; SECTOR as usize];
    for s in 0..sectors {
        let off = s * SECTOR;
        if !crate::assets::read_chunk(src + off, &mut buf) {
            println!("{}: read failed at {:#x}", tag, src + off);
            return false;
        }
        if !write_sector_verified(dst + off, &buf, tag) {
            println!("{}: verify failed at {:#x}", tag, dst + off);
            return false;
        }
        if s % 64 == 0 {
            println!("{}: copied {}/{} KiB", tag, off / 1024, sectors * SECTOR / 1024);
        }
    }
    true
}

/// Total byte length of the app image at `base`, by walking its segment
/// table (header: 0xE9, segment count, …, entry point; then per segment an
/// 8-byte load-addr/size header). The image ends with a checksum byte
/// padded to 16, plus an appended SHA-256 when header byte 23 says so.
pub fn image_len(base: u32) -> Option<u32> {
    let mut hdr = [0u8; 24];
    if !crate::assets::read_chunk(base, &mut hdr) || hdr[0] != 0xE9 {
        return None;
    }
    let segments = hdr[1];
    if segments == 0 || segments > 16 {
        return None;
    }
    let hash_appended = hdr[23] == 1;
    let mut pos: u32 = 24;
    for _ in 0..segments {
        let mut seg = [0u8; 8];
        if !crate::assets::read_chunk(base + pos, &mut seg) {
            return None;
        }
        let size = u32::from_le_bytes(seg[4..8].try_into().unwrap());
        if size > 4 * 1024 * 1024 {
            return None;
        }
        pos += 8 + size;
    }
    pos = (pos + 1 + 15) & !15; // checksum byte, padded to 16
    if hash_appended {
        pos += 32;
    }
    Some(pos)
}

/// This build's app descriptor as raw bytes. Every app image carries it at
/// image offset 0x20 (right behind the 24-byte image header + first 8-byte
/// segment header). Version + project name + toolchain build stamp
/// distinguish it from any other firmware — comparing it against flash
/// finds *our* slot without trusting otadata.
pub fn own_desc_bytes() -> &'static [u8] {
    let d: &'static esp_bootloader_esp_idf::EspAppDesc = &crate::ESP_APP_DESC;
    unsafe {
        core::slice::from_raw_parts(
            (d as *const esp_bootloader_esp_idf::EspAppDesc).cast::<u8>(),
            core::mem::size_of::<esp_bootloader_esp_idf::EspAppDesc>(),
        )
    }
}

/// Does the app slot at `offset` hold an image built from this exact
/// source tree?
pub fn image_is_ours(offset: u32) -> bool {
    let me = own_desc_bytes();
    let mut desc = alloc::vec![0u8; me.len()];
    crate::assets::read_chunk(offset + 0x20, &mut desc) && desc == me
}

/// Find the app slot of `live_table` that holds this image. Candidates are
/// tried with `prefer` first, so an interrupted self-copy that already
/// landed the image at the destination is recognised there and the copy is
/// skipped rather than repeated over the region we might be running from.
pub fn find_own_slot(live_table: &[u8], prefer: u32) -> Option<Part> {
    let mut cands = app_entries(live_table);
    // Stable partition on a 0/1 key, hand-rolled over a handful of entries.
    // `sort_unstable_by_key` instantiates the whole driftsort family for
    // `Part` — 2,415 B of image measured on board-pixelblaze-v3 (Gitea
    // #501) — for a list that is never longer than a table's app slots.
    for i in 1..cands.len() {
        let mut j = i;
        while j > 0 && cands[j - 1].offset != prefer && cands[j].offset == prefer {
            cands.swap(j - 1, j);
            j -= 1;
        }
    }
    cands.into_iter().find(|c| image_is_ours(c.offset))
}

/// Install `table` at [TABLE_OFFSET]: one erase + write + read-back, up to
/// three attempts **in place**. This is the point of no return for both
/// callers — a reboot cannot help once the old table's sector is erased, so
/// the retry never reboots, and the write is verified because a wrong table
/// here is a brick.
pub fn install(table: &[u8], tag: &str) -> bool {
    for _attempt in 0..3 {
        if write_sector_verified(TABLE_OFFSET, table, tag) {
            return true;
        }
        println!("{}: table write attempt failed — retrying in place", tag);
    }
    println!("{}: TABLE WRITE FAILED — device needs a serial reflash", tag);
    false
}
