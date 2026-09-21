//! Self-applied partition-layout migration (Gitea #501).
//!
//! A device flashed before #501 runs the 1 MiB-slot table. This image
//! embeds a different one ([crate::parttab::EMBEDDED]). On the first boot
//! of the *migrating release* this module notices the difference, moves
//! the device's data into the new layout, installs the new table and
//! reboots — so no device needs Jeremy's hands on a serial port, which is
//! the whole point: nothing on the bench has a serial path today.
//!
//! # What has to move
//!
//! | region  | old (1 MiB slots)      | 4 MB layout          | 16 MB layout |
//! |---------|------------------------|----------------------|--------------|
//! | ota_0   | 0x10000 + 1 MiB        | same offset, 1.25 MiB| same, 3 MiB  |
//! | ota_1   | 0x110000 + 1 MiB       | 0x150000 + 1.25 MiB  | 0x310000     |
//! | storage | 0x210000 + 1 MiB       | 0x290000 + 512 KiB   | 0x610000 + 4 MiB |
//! | assets  | 0x310000 + 960 KiB     | UNCHANGED            | 0xA10000 + ~3.9 MiB |
//!
//! Every one of those numbers is read from a partition table — the live one
//! for "old", [crate::parttab::EMBEDDED] for "new". This module contains no
//! partition offsets.
//!
//! The store's own geometry makes this a byte move rather than a format
//! change: the key area is [crate::patterns::KEY_AREA_LEN] on every layout
//! and the packed file log always starts at [crate::patterns::LOG_AT], so
//! only the log's *length* differs. The migration therefore re-uses the
//! store's own machinery — `patlog::plan` + `patlog::build_page` place and
//! rewrite the live records exactly as a compaction does, and the reserved
//! blobs (playlist, playback state, pixel map, resume record, palette,
//! layout, device name) go through `patterns::{read,store}_blob_at`.
//!
//! # Why a staging area
//!
//! On the 4 MB layout the NEW store region sits *inside* the OLD log
//! (0x290000..0x310000 vs 0x259000..0x310000). Writing the repacked log
//! straight to its new home would overwrite source bytes it has not read
//! yet. So the repacked image is built first into the old `ota_1` slot —
//! free scratch, because the device is deliberately single-image from here
//! until its next OTA — and only then written to its new home. The 16 MB
//! layout does not overlap, but takes the same path: one code path costs
//! fewer bytes of an OTA slot than two.
//!
//! # Power-cut safety
//!
//! Every stage is re-runnable and marked in a staging header (`LXMG`) at
//! the head of the staging area, keyed to the target table so a header
//! left by a different release is ignored:
//!
//! ```text
//!   (none)  → copy ourselves into the new ota_0 if we are not there, point
//!             the bootloader at it, reboot.  Idempotent: the copy is
//!             skipped when the image is already in place.
//!   STAGED  → the repacked log image is complete in staging.  Until this
//!             mark the OLD store is untouched, so a cut just re-stages.
//!   STORED  → the new storage region holds the key area and the log.  The
//!             old log is gone from here on; a cut re-runs this stage from
//!             staging, which is still intact.
//!   ASSETS  → the web bundle is at its new home (16 MB layout only).
//!   -       → the table is written, LAST, as ONE sector erase+write, and
//!             the device reboots into the new layout.
//! ```
//!
//! The residual risk is that single 4 KiB table write: a cut *inside* it
//! leaves a table whose MD5 row does not verify, and the ESP-IDF
//! second-stage bootloader will not boot that — serial recovery only. It
//! is milliseconds, it is the same window the WLED takeover has always
//! had, and there is no second table copy on stock IDF bootloaders to hide
//! behind. docs/firmware.md says so out loud.
//!
//! # Refusing rather than losing data
//!
//! The 4 MB layout's log is 220 KiB against the old 732 KiB. If the live
//! records do not fit, the migration **does not start**: the old table
//! stays, the device keeps working exactly as it did, and `/api/status`
//! reports `migration_blocked` with the numbers so the user can delete
//! patterns and reboot. Losing a pattern silently is not on the menu.
//!
//! The same applies to a flash write that fails mid-run: every stage that
//! gives up records why through [block], because no device in this fleet
//! has a serial console — which is the whole reason the migration is
//! self-applied. A stage that reported only to `println!` made "it failed"
//! and "it was never attempted" the same three fields on `/api/status`,
//! which is exactly what the Seengreat's 2026-09-21 decline looked like
//! (Gitea #634). Mid-run failures still leave the old table intact and
//! still retry on the next boot; they are now just visible while they do.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;
use luxel_core::jsonview::{push_piece, push_u32};

use crate::parttab::{self, Part, SECTOR};
use crate::patlog;

/// Staging-header magic: "LXMG".
const HDR_MAGIC: u32 = 0x474D_584C;
const HDR_VER: u32 = 1;
const HDR_WORDS: usize = 6;

const S_NONE: u32 = 0;
const S_STAGED: u32 = 1;
const S_STORED: u32 = 2;
const S_ASSETS: u32 = 3;

/// Why the migration refused to start, for `/api/status`. Empty = no
/// refusal (either it is not needed, it is not applicable, or it ran).
static BLOCKED: BlockingMutex<CriticalSectionRawMutex, core::cell::RefCell<&'static str>> =
    BlockingMutex::new(core::cell::RefCell::new(""));
/// Two diagnostic numbers whose meaning depends on the refusal: bytes
/// needed vs bytes offered for the "too large" family, and the flash
/// offsets involved for the mid-run flash failures. Zero when the reason
/// needs no numbers.
static BLOCK_NEED: AtomicU32 = AtomicU32::new(0);
static BLOCK_HAVE: AtomicU32 = AtomicU32::new(0);

fn block(why: &'static str, need: u32, have: u32) {
    BLOCKED.lock(|c| *c.borrow_mut() = why);
    BLOCK_NEED.store(need, Ordering::Relaxed);
    BLOCK_HAVE.store(have, Ordering::Relaxed);
    println!("migrate: BLOCKED — {} (need {} B, have {} B)", why, need, have);
}

// ---------------------------------------------------------------- header

struct Hdr {
    stage: u32,
    log_bytes: u32,
}

/// FNV-1a of the target table: a staging header only applies to the layout
/// it was written for.
fn target_tag() -> u32 {
    patlog::fnv1a(parttab::EMBEDDED)
}

fn read_hdr(at: u32) -> Option<Hdr> {
    let mut b = [0u8; HDR_WORDS * 4];
    if !crate::assets::read_chunk(at, &mut b) {
        return None;
    }
    let w = |i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
    if w(0) != HDR_MAGIC || w(1) != HDR_VER || w(4) != target_tag() {
        return None;
    }
    if w(5) != patlog::fnv1a(&b[..(HDR_WORDS - 1) * 4]) {
        return None;
    }
    Some(Hdr { stage: w(2), log_bytes: w(3) })
}

fn write_hdr(at: u32, stage: u32, log_bytes: u32) -> bool {
    let mut b = [0u8; HDR_WORDS * 4];
    for (i, v) in [HDR_MAGIC, HDR_VER, stage, log_bytes, target_tag()]
        .into_iter()
        .enumerate()
    {
        b[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    let crc = patlog::fnv1a(&b[..(HDR_WORDS - 1) * 4]);
    b[(HDR_WORDS - 1) * 4..].copy_from_slice(&crc.to_le_bytes());
    parttab::write_sector_verified(at, &b, "migrate")
}

// ------------------------------------------------------------- reporting

/// The live table's headline sizes, snapshotted ONCE at boot. `/api/status`
/// is polled continuously by the playground, and re-reading + re-parsing a
/// 3 KiB table (and heap-allocating it) per poll would put a flash read on
/// the response path for three numbers that cannot change without a reboot.
static LIVE_SLOT: AtomicU32 = AtomicU32::new(0);
static LIVE_STORE: AtomicU32 = AtomicU32::new(0);
static LIVE_ASSETS: AtomicU32 = AtomicU32::new(0);

fn snapshot(table: &[u8]) {
    LIVE_SLOT.store(
        parttab::app_slot(table, parttab::SUBTYPE_OTA0).map(|p| p.len).unwrap_or(0),
        Ordering::Relaxed,
    );
    LIVE_STORE.store(
        parttab::data_labelled(table, "storage").map(|p| p.len).unwrap_or(0),
        Ordering::Relaxed,
    );
    LIVE_ASSETS.store(
        parttab::data_labelled(table, "assets").map(|p| p.len).unwrap_or(0),
        Ordering::Relaxed,
    );
}

/// `,"partitions":{…}` for `/api/status` — what layout this device is
/// actually running, so a fleet tool can tell migrated from un-migrated
/// without a serial console.
pub fn push_status(out: &mut alloc::string::String) {
    let (slot, store, assets) = (
        LIVE_SLOT.load(Ordering::Relaxed),
        LIVE_STORE.load(Ordering::Relaxed),
        LIVE_ASSETS.load(Ordering::Relaxed),
    );
    push_piece(out, ",\"partitions\":{\"layout\":\"");
    push_piece(out, parttab::TABLE_NAME);
    push_piece(out, "\",\"migrated\":");
    push_piece(out, if parttab::matches_flash() { "true" } else { "false" });
    push_piece(out, ",\"ota_slot_bytes\":");
    push_u32(out, slot);
    push_piece(out, ",\"storage_bytes\":");
    push_u32(out, store);
    push_piece(out, ",\"assets_bytes\":");
    push_u32(out, assets);
    let why = BLOCKED.lock(|c| *c.borrow());
    if !why.is_empty() {
        push_piece(out, ",\"migration_blocked\":\"");
        push_piece(out, why);
        push_piece(out, "\",\"blocked_need_bytes\":");
        push_u32(out, BLOCK_NEED.load(Ordering::Relaxed));
        push_piece(out, ",\"blocked_have_bytes\":");
        push_u32(out, BLOCK_HAVE.load(Ordering::Relaxed));
    }
    push_piece(out, "}");
}

// --------------------------------------------------------------- the run

/// Called once at boot, after `patterns::init` (which resolves and scans
/// the OLD store, so the records this reads are the ones the device was
/// already serving) and before anything that writes to it.
///
/// No-op — one 3 KiB flash read — when the table on flash is already the
/// one this image embeds, which is every boot after the first.
pub fn maybe_migrate() {
    let Some(live) = parttab::live_table() else { return };
    snapshot(&live);
    // `migrate-off`: the retirement knob. This module is ~12 KB of image
    // that every device needs exactly ONCE, ever. Once the fleet has
    // moved (and the release notes say the migrating release is a
    // prerequisite), a later release drops it and gets the slot back —
    // tools/image-check.sh asserts the marker is present unless the
    // feature is named, so retiring it has to be deliberate.
    if cfg!(feature = "migrate-off") {
        return;
    }
    if live == parttab::EMBEDDED {
        return;
    }
    // A FOREIGN table (WLED's, or anything that is not ours) is the
    // takeover's job, not ours: it has no Luxel store to carry across and
    // it needs the config wipe this deliberately never does.
    if !parttab::is_luxel(&live) {
        return;
    }
    println!(
        "migrate: partition table on flash is an older Luxel layout — moving to {}",
        parttab::TABLE_NAME
    );

    // Never write a table this BOARD cannot back — the chip has to be big
    // enough AND the bootloader has to accept a table that reaches that
    // far, which on the Seengreat it did not (Gitea #634). Either way the
    // result of getting it wrong is a board that will not boot and has no
    // serial console, so this is the one check that runs before any other.
    if let Some((why, need, have)) = parttab::flash_refusal(parttab::EMBEDDED) {
        block(why, need, have);
        return;
    }

    let (Some(old_store), Some(new_store)) = (
        parttab::data_labelled(&live, "storage"),
        parttab::data_labelled(parttab::EMBEDDED, "storage"),
    ) else {
        block("no storage partition", 0, 0);
        return;
    };
    let (Some(old_ota1), Some(new_ota0)) = (
        parttab::app_slot(&live, parttab::SUBTYPE_OTA1),
        parttab::app_slot(parttab::EMBEDDED, parttab::SUBTYPE_OTA0),
    ) else {
        block("no OTA slots", 0, 0);
        return;
    };

    let staging = old_ota1;
    let new_log_len = new_store.len.saturating_sub(crate::patterns::LOG_AT);

    // 1 — WILL it fit? Decided before anything is written, so a device whose
    // library is too large for the smaller log keeps its old table, its
    // otadata and its data exactly as they were, and simply says so on
    // /api/status every boot until the user frees some space.
    //
    // Reading the staging header first is safe under either slot: if we are
    // executing out of the staging slot, the read finds our own image rather
    // than an LXMG header and correctly reports "not staged".
    let hdr = read_hdr(staging.offset);
    let resuming = hdr.as_ref().is_some_and(|h| h.stage >= S_STAGED);
    let plan = if resuming {
        // The old log is already gone or going; re-planning it would be
        // planning from rubble. The staged image is the source of truth.
        None
    } else {
        match plan_log(staging, new_log_len) {
            Some(p) => Some(p),
            None => return, // blocked; nothing has been written
        }
    };

    // 2 — run from the slot the new table calls ota_0. Everything below
    // erases the old ota_1, so we must not be executing out of it.
    if !settle_into_ota0(&live, new_ota0) {
        return; // rebooted, or aborted with the old table intact
    }

    // 3 — stage (or resume).
    let log_bytes = match hdr {
        Some(h) if h.stage >= S_STAGED => {
            println!("migrate: resuming at stage {} ({} B of log staged)", h.stage, h.log_bytes);
            h.log_bytes
        }
        _ => match plan.and_then(|p| stage_log(staging, p, new_log_len)) {
            Some(n) => n,
            None => return, // blocked or failed; old table untouched
        },
    };
    let stage = read_hdr(staging.offset).map(|h| h.stage).unwrap_or(S_NONE);
    if stage < S_STAGED {
        block("staging header did not stick", 0, 0);
        return;
    }

    // 4 — build the new store region.
    if stage < S_STORED {
        if !write_new_store(old_store, new_store, staging, log_bytes) {
            // write_new_store has already recorded WHICH step failed.
            println!("migrate: store relocation failed — old table intact, will retry next boot");
            return;
        }
        if !write_hdr(staging.offset, S_STORED, log_bytes) {
            block("staging header write failed", 0, 0);
            return;
        }
    }

    // 5 — assets (16 MB layout only; the 4 MB one keeps the offset).
    if read_hdr(staging.offset).map(|h| h.stage).unwrap_or(S_NONE) < S_ASSETS {
        if !move_assets(&live) {
            // move_assets has already recorded WHICH step failed.
            println!("migrate: asset move failed — old table intact, will retry next boot");
            return;
        }
        if !write_hdr(staging.offset, S_ASSETS, log_bytes) {
            block("staging header write failed", 0, 0);
            return;
        }
    }

    // 6 — the point of no return.
    println!("migrate: installing the new partition table");
    if !parttab::install(parttab::EMBEDDED, "migrate") {
        block("partition table write failed", parttab::TABLE_OFFSET, 0);
        return;
    }
    println!("migrate: partition table installed — rebooting into the new layout");
    crate::ota::clear_boot_attempts();
    esp_hal::system::software_reset()
}

/// Make sure we are running from — and the bootloader points at — the slot
/// the NEW table calls ota_0, so the old ota_1 is free scratch for
/// staging. Reboots (and never returns) when a self-copy was needed;
/// returns false when it had to abort with the old table intact.
fn settle_into_ota0(live: &[u8], new_ota0: Part) -> bool {
    // Where we are EXECUTING from, per otadata as it read at ota::init —
    // not "is our image at that offset", because under the old table both
    // slots can hold byte-identical images and erasing the one under our
    // feet is fatal.
    let exec = current_exec_slot(live);
    if exec == u32::MAX {
        block("cannot tell which slot is running", 0, 0);
        return false;
    }

    if exec != new_ota0.offset {
        if parttab::image_is_ours(new_ota0.offset) {
            // An earlier attempt already landed the copy; do not repeat it.
            println!("migrate: image already at {:#x}", new_ota0.offset);
        } else {
            let Some(len) = parttab::image_len(exec) else {
                block("cannot size this image", 0, 0);
                return false;
            };
            if len > new_ota0.len {
                block("image larger than the new ota_0", len, new_ota0.len);
                return false;
            }
            // Overlap guard: the copy must never erase the region we are
            // executing from. It writes exactly `len` bytes at
            // `new_ota0.offset`, so `len` is what goes on BOTH sides —
            // using the destination SLOT length instead would compare
            // against 1.25 MiB of slot that the copy never touches, and
            // since the old ota_1 (0x110000) sits inside the new ota_0
            // (0x10000 + 1.25 MiB) it would refuse every device whose last
            // OTA happened to land in ota_1, i.e. half the fleet. In
            // practice this can never trip — an image small enough to be
            // running from the old 1 MiB ota_1 is small enough to fit below
            // it — but the next table change is exactly what it is for.
            if exec < new_ota0.offset + len && new_ota0.offset < exec + len {
                block("image overlaps the new ota_0", exec, new_ota0.offset);
                return false;
            }
            println!("migrate: copying {} B {:#x} → {:#x}", len, exec, new_ota0.offset);
            if !parttab::copy_region(exec, new_ota0.offset, len, "migrate") {
                println!("migrate: self-copy failed — old table intact, will retry next boot");
                return false;
            }
        }
    }

    // Point the bootloader at ota_0 by ERASING otadata: with no factory
    // partition an empty otadata is exactly "boot ota_0". Deliberately NOT
    // the takeover's config wipe — nvs holds the WiFi credentials and the
    // device settings, and a device that came back without them would be
    // unreachable (nothing on the bench has serial).
    //
    // It also disarms one failure mode of the boot-loop guard for the rest
    // of the migration: from the next stage on, the old ota_1 is staging
    // scratch rather than a bootable image, and the guard must not flip to
    // it.
    let Some(otadata) = parttab::entries(live)
        .into_iter()
        .find(|p| p.labelled("otadata"))
    else {
        block("no otadata partition", 0, 0);
        return false;
    };
    if !parttab::erase_range(otadata.offset, otadata.end()) {
        block("otadata erase failed", 0, 0);
        return false;
    }
    crate::ota::clear_boot_attempts();

    if exec != new_ota0.offset {
        // We are executing out of the slot the next stage erases.
        println!("migrate: rebooting into ota_0 to free the staging slot");
        esp_hal::system::software_reset()
    }
    true
}

/// Offset of the app slot we are executing from, as the LIVE table sees
/// it. `u32::MAX` when it cannot be established (which only makes the
/// caller more conservative).
fn current_exec_slot(live: &[u8]) -> u32 {
    let name = crate::ota::booted_slot();
    let sub = match name {
        "ota_0" => parttab::SUBTYPE_OTA0,
        "ota_1" => parttab::SUBTYPE_OTA1,
        _ => return u32::MAX,
    };
    parttab::app_slot(live, sub).map(|p| p.offset).unwrap_or(u32::MAX)
}

/// Plan the repack of the live pattern log, and refuse loudly if the result
/// will not fit. Every reason this can say no is a `block`: this is the ONLY
/// place the migration decides it cannot proceed on the device's data, and it
/// runs before a single byte is written, so a refusal costs the device
/// nothing at all.
///
/// Returns the records, their placements, and the staged length in bytes.
fn plan_log(staging: Part, new_log_len: u32) -> Option<(Vec<patlog::Rec>, Vec<patlog::Place>, u32)> {
    if !crate::patterns::store_ready() {
        // An empty RAM index here would mean "no patterns" when what it
        // actually means is "the store did not come up" — and we would
        // cheerfully migrate the library away.
        block("pattern store did not come up", 0, 0);
        return None;
    }
    if crate::patterns::index_overfull() {
        block("pattern index incomplete — delete patterns and reboot", 0, 0);
        return None;
    }
    let recs = crate::patterns::index_snapshot();
    let mut places: Vec<patlog::Place> = Vec::new();
    // Nothing is pinned: no engine is executing out of the log yet (this
    // runs before the render task starts), so the repack is a clean pack
    // toward offset 0.
    let Some(cursor) = patlog::plan(&recs, &[], &mut |p| places.push(p)) else {
        block("cannot repack the pattern log", 0, 0);
        return None;
    };
    let bytes = patlog::align_page(cursor);
    if bytes > new_log_len {
        block("pattern library too large for the new layout", bytes, new_log_len);
        return None;
    }
    // The staging area holds the header sector plus the image.
    if SECTOR + bytes > staging.len {
        block("staging slot too small for the pattern log", SECTOR + bytes, staging.len);
        return None;
    }
    Some((recs, places, bytes))
}

/// Repack the live pattern log into the staging area. Returns the staged
/// length in bytes, or None on a flash failure (the OLD store is untouched
/// either way — this only ever reads it).
///
/// Safe against the image we are executing from by construction: staging
/// starts at the OLD ota_1 offset, which is one old slot above ota_0, and a
/// running image had to fit that old slot — so it ends at or below the
/// staging header's sector and the two never meet. [settle_into_ota0] has
/// already guaranteed we are running from ota_0 by the time this is called.
fn stage_log(staging: Part, plan: (Vec<patlog::Rec>, Vec<patlog::Place>, u32), new_log_len: u32) -> Option<u32> {
    let (recs, places, bytes) = plan;
    println!(
        "migrate: staging {} live pattern(s), {} B of log → {:#x} (new log holds {} B)",
        recs.len(),
        bytes,
        staging.offset + SECTOR,
        new_log_len
    );

    let mut buf = alloc::vec![0u8; SECTOR as usize];
    for page in 0..(bytes / patlog::PAGE) {
        let built = crate::patterns::with_log_arena(|a| {
            patlog::build_page(a, page, &recs, &places, &mut buf)
        });
        if built != Some(true) {
            println!("migrate: could not build log page {} — aborting (old store intact)", page);
            block("could not build the staged log", page * patlog::PAGE, bytes);
            return None;
        }
        let at = staging.offset + SECTOR + page * patlog::PAGE;
        if !parttab::write_sector_verified(at, &buf, "migrate") {
            println!("migrate: staging write failed at page {} — aborting (old store intact)", page);
            block("staging write failed", at, bytes);
            return None;
        }
    }
    if !write_hdr(staging.offset, S_STAGED, bytes) {
        block("staging header write failed", 0, 0);
        return None;
    }
    Some(bytes)
}

/// Erase the new storage partition and fill it: reserved blobs through the
/// store's own API, then the staged log image.
fn write_new_store(old_store: Part, new_store: Part, staging: Part, log_bytes: u32) -> bool {
    println!(
        "migrate: new storage {:#x} + {} KiB — erasing",
        new_store.offset,
        new_store.len / 1024
    );
    // Safety net for a future table: the erase below must not touch the
    // old key area, which the blob copy is about to read.
    if new_store.offset < old_store.offset + crate::patterns::KEY_AREA_LEN
        && old_store.offset < new_store.end()
    {
        block("new storage overlaps the old key area", new_store.offset, old_store.offset);
        return false;
    }
    if !parttab::erase_range(new_store.offset, new_store.end()) {
        block("new storage erase failed", new_store.offset, new_store.len);
        return false;
    }

    // Reserved blobs: playlist, playback state, pixel map, resume record,
    // output palette, Layout, device name — the whole reserved key range,
    // so a key this firmware does not know about still travels.
    let mut moved = 0u32;
    for key in crate::patterns::RESERVED_LO..=crate::patterns::RESERVED_HI {
        let Some(v) = crate::patterns::read_blob_at(old_store.offset, key) else {
            continue;
        };
        if crate::patterns::store_blob_at(new_store.offset, key, &v) {
            moved += 1;
        } else {
            println!("migrate: blob {:#x} ({} B) did not re-store — aborting", key, v.len());
            block("reserved blob did not re-store", key, v.len() as u32);
            return false;
        }
    }
    println!("migrate: {} reserved blob(s) carried over", moved);

    // The repacked log.
    if log_bytes > 0
        && !parttab::copy_region(
            staging.offset + SECTOR,
            new_store.offset + crate::patterns::LOG_AT,
            log_bytes,
            "migrate",
        )
    {
        block(
            "staged log copy failed",
            staging.offset + SECTOR,
            new_store.offset + crate::patterns::LOG_AT,
        );
        return false;
    }
    println!("migrate: store relocated");
    true
}

/// Move the web-asset bundle when the new layout puts it somewhere else
/// (16 MB boards). A no-op when the offset is unchanged, which is the 4 MB
/// layout's entire point — the bundle survives untouched.
fn move_assets(live: &[u8]) -> bool {
    let (Some(old), Some(new)) = (
        parttab::data_labelled(live, "assets"),
        parttab::data_labelled(parttab::EMBEDDED, "assets"),
    ) else {
        println!("migrate: no assets partition to move");
        return true;
    };
    let len = old.len.min(new.len);
    if old.offset == new.offset {
        println!("migrate: assets stay at {:#x} — nothing to move", old.offset);
        return true;
    }
    if old.offset < new.offset + len && new.offset < old.offset + len {
        // Cannot happen with either shipped table; a guard for the next one.
        println!("migrate: asset regions overlap {:#x}/{:#x} — not moving", old.offset, new.offset);
        block("asset regions overlap", old.offset, new.offset);
        return false;
    }
    println!("migrate: moving assets {:#x} → {:#x}", old.offset, new.offset);
    if !parttab::copy_region(old.offset, new.offset, len, "migrate") {
        block("asset copy failed", old.offset, new.offset);
        return false;
    }
    true
}
