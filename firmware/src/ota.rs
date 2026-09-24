//! Over-the-air updates. `POST /api/ota` streams an app image (espflash
//! save-image output, NOT the merged full-flash image) sector-by-sector into
//! the OTA slot the device is NOT executing from, verifies it, activates
//! it, and reboots. Pure A/B: there is no factory partition (see
//! partitions.csv) — ota_0/ota_1 alternate, and if both hold broken images
//! (or otadata is erased) the bootloader falls back to ota_0, its default
//! when no factory partition exists. Serial flashes land in ota_0.
//!
//! # The invariant (Gitea #655)
//!
//! **An interrupted update must never leave the device without a bootable
//! slot.** Three things hold it up, and each one exists because the panel
//! lost it on 2026-09-21:
//!
//! 1. The target slot is chosen from where the running image is
//!    MMU-mapped ([BOOTED_AT]), never from `otadata`
//!    (`parttab::ota_target`). esp-bootloader-esp-idf's `next_partition()`
//!    answers `ota_0` for a device executing from `ota_0` whenever `otadata`
//!    is erased — a state the layout migration leaves behind on any boot
//!    where it runs and then fails after `settle_into_ota0`, and one the
//!    bootloader repairs only on the NEXT boot — and the update then erased
//!    the code it was running. The device wedged mid-upload and rebooted
//!    into a slot holding one image's head over another's tail.
//! 2. The first sector of the image stays in RAM until the whole upload is
//!    on flash and verified ([OtaWriter::commit]). A slot whose head is
//!    still erased cannot be mistaken for an image by anyone — not the
//!    bootloader's "try the other partitions" fallback, not
//!    [preboot_guard]'s rollback — however far a wedged upload got.
//! 3. Before `otadata` moves, the image is verified where it sits
//!    (`appimg::verify`: segment table, exact length, checksum — the
//!    bootloader's own checks). The bootloader does NOT fail a torn image
//!    gracefully: a garbage segment header trips an `assert`, and the
//!    board resets and loops without ever trying the other slot.
//!
//! Division of labor with esp-bootloader-esp-idf (reviewed 2026-07-06 after
//! the "is this reinventing a library wheel?" question): the library owns
//! the `otadata` format — activation and image state — and the streaming
//! erase+write loop is ours because that is where the flash-vs-WiFi timing
//! constraints live (erase-on-write, yields), policy the library rightly
//! doesn't own. Slot SELECTION moved out of the library with #655 (point 1
//! above). The crashes that prompted the original question were the
//! task-stack architecture (see assets::read_chunk), not this loop.

use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::ota_updater::OtaUpdater;
use esp_bootloader_esp_idf::partitions::{
    self, AppPartitionSubType, PartitionType, PARTITION_TABLE_MAX_LEN,
};

use crate::parttab::{self, SECTOR, SUBTYPE_OTA0};
use esp_println::println;
use esp_storage::FlashStorage;

static FLASH: BlockingMutex<CriticalSectionRawMutex, RefCell<Option<FlashStorage<'static>>>> =
    BlockingMutex::new(RefCell::new(None));

/// True while an OTA upload owns the flash write path. Blocks a second OTA
/// and [take_flash] users (pattern/playlist stores) for the duration — the
/// OTA writer itself goes through [with_flash] per operation, exactly like
/// the assets writer, so the driver stays in the global. (The previous
/// design *took* the driver for the whole upload and ran the flash ops on
/// it bare; that path crashed the Athom mid-erase-burst 5/5 while the
/// borrow-per-op assets path was clean 4/4 — see UPDATES.md 2026-07-27.)
static OTA_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Which app partition is running, for /api/status. Set once at boot.
static BOOTED: BlockingMutex<CriticalSectionRawMutex, RefCell<&'static str>> =
    BlockingMutex::new(RefCell::new("unknown"));
/// Flash offset of that partition — where the running image is MMU-mapped
/// from, per the bootloader's own MMU entry, NOT per otadata. `u32::MAX`
/// when it could not be established, which refuses every update.
static BOOTED_AT: AtomicU32 = AtomicU32::new(u32::MAX);

fn slot_name(sub: AppPartitionSubType) -> &'static str {
    match sub {
        AppPartitionSubType::Factory => "factory",
        AppPartitionSubType::Ota0 => "ota_0",
        AppPartitionSubType::Ota1 => "ota_1",
        _ => "ota_?",
    }
}

pub fn init(mut flash: FlashStorage<'static>) {
    // heap, not stack: 3 KiB frames + WiFi level-6 NMIs (which run on the
    // current task stack) overflowed the main task during flash ops
    let mut buffer = alloc::vec![0u8; PARTITION_TABLE_MAX_LEN];
    let (booted, at) = match partitions::read_partition_table(&mut flash, &mut buffer) {
        Ok(pt) => match pt.booted_partition() {
            Ok(Some(p)) => match p.partition_type() {
                PartitionType::App(sub) => (slot_name(sub), p.offset()),
                _ => ("unknown", u32::MAX),
            },
            _ => ("unknown", u32::MAX),
        },
        Err(_) => ("unknown", u32::MAX),
    };
    BOOTED.lock(|c| *c.borrow_mut() = booted);
    BOOTED_AT.store(at, Ordering::Relaxed);
    println!("booted from: {}", booted);
    FLASH.lock(|c| *c.borrow_mut() = Some(flash));
    // Say where the next update would go, on every boot: the one thing the
    // emulator can assert about slot selection without a network (no OTA
    // reaches a QEMU guest), and the line to read on a device whose
    // migration has just declined — it must never name the running slot.
    match target() {
        Ok((off, len, name)) => println!("ota: updates go to {} at {:#x} ({} B)", name, off, len),
        Err(e) => println!("ota: no update target — {}", e),
    }
}

pub fn booted_slot() -> &'static str {
    BOOTED.lock(|c| *c.borrow())
}

/// (offset, len, name) of the slot an update writes — see the module doc.
/// Reads on the shared driver; the table cannot change without a reboot and
/// nobody writes the running slot, so the answer is stable for the boot.
fn target() -> Result<(u32, u32, &'static str), &'static str> {
    let live = parttab::live_table().ok_or("cannot read the partition table")?;
    let booted = BOOTED_AT.load(Ordering::Relaxed);
    let running = if booted == u32::MAX {
        0
    } else {
        parttab::image_len(booted).ok_or("cannot size the running image")?
    };
    let t = parttab::ota_target(&live, booted, running)?;
    Ok((t.offset, t.len, if t.subtype == SUBTYPE_OTA0 { "ota_0" } else { "ota_1" }))
}

/// Borrow the flash driver briefly (reads, or writes outside an OTA).
/// Returns None while an OTA holds the driver.
pub fn with_flash<T>(f: impl FnOnce(&mut FlashStorage<'static>) -> T) -> Option<T> {
    with_flash_as(crate::core1::tag::OTHER, f)
}

/// [with_flash], attributing the fence it takes to a call site
/// (`core1::tag`) so `/api/status` can report the per-writer fence rate —
/// the variable that predicts the #292 wedge.
pub fn with_flash_as<T>(tag: usize, f: impl FnOnce(&mut FlashStorage<'static>) -> T) -> Option<T> {
    // The flash fence (dual-core: park the other core for the op) sits
    // OUTSIDE the critical section on purpose — its spin-waits must run
    // with interrupts enabled so the other core can park us in turn. See
    // core1.rs.
    crate::core1::fenced_as(tag, || {
        FLASH.lock(|c| {
            c.borrow_mut().as_mut().map(|fl| {
                crate::core1::bb_phase(6);
                let r = f(fl);
                crate::core1::bb_phase(7);
                r
            })
        })
    })
}

/// Take the flash driver out for a self-contained multi-op transaction (the
/// pattern store's sequential-storage calls). Unlike [with_flash], this does
/// NOT keep the driver behind a critical section for the whole operation —
/// holding one across sequential-storage's scans + page erases would disable
/// interrupts far too long. The caller runs its (blocking) flash work with
/// interrupts enabled, then returns the driver via [give_flash]. Returns None
/// while an OTA upload is in progress (its slot writes must not interleave
/// with a store transaction). Pair with a Drop guard for panic safety.
pub fn take_flash() -> Option<FlashStorage<'static>> {
    if OTA_ACTIVE.load(Ordering::Acquire) {
        return None;
    }
    FLASH.lock(|c| c.borrow_mut().take())
}

/// Return a driver taken by [take_flash].
pub fn give_flash(flash: FlashStorage<'static>) {
    FLASH.lock(|c| *c.borrow_mut() = Some(flash));
}

/// True while an OTA upload owns the flash write path. Borrow-per-op
/// writers outside the OTA (patterns::store_current) check this themselves —
/// [with_flash] deliberately doesn't, because OTA's own per-op writes go
/// through it.
pub fn ota_active() -> bool {
    OTA_ACTIVE.load(Ordering::Acquire)
}

// ---- boot-loop guard: app-level OTA rollback ----
// The stock (espflash) bootloader has no auto-rollback, so a freshly OTA'd
// image that crashes during boot wedges the device until a serial reflash
// (this happened: v0.1.19's first cut overflowed the main stack in WiFi
// init). The guard runs BEFORE the risky part of boot: it counts boot
// attempts in the fourth nvs sector, and on the 3rd consecutive failed
// boot flips otadata back to the other slot and reboots. main() calls
// [boot_ok] once the device has been demonstrably healthy for a while,
// which resets the counter. Rapid manual power-cycling can theoretically
// trip it — that's benign (the other slot also boots) and self-corrects.

const GUARD_OFFSET: u32 = 0xC000;
const GUARD_MAGIC: &[u8; 4] = b"LXBG";

/// The guard record: magic + byte 4 = failed-boot counter, byte 5 =
/// one-shot force-AP flag, byte 6 = takeover reboot-to-retry counter
/// (issue #35). One sector, one record; every writer preserves the
/// fields it doesn't own.
#[derive(Clone, Copy, Default)]
struct Guard {
    attempts: u8,
    force_ap: bool,
    takeover_retries: u8,
}

fn read_guard() -> Guard {
    let mut rec = [0u8; 8];
    if !crate::assets::read_chunk(GUARD_OFFSET, &mut rec) || &rec[0..4] != GUARD_MAGIC {
        return Guard::default();
    }
    Guard {
        attempts: rec[4],
        force_ap: rec[5] == 1,
        takeover_retries: rec[6],
    }
}


/// One-shot "boot into the provisioning AP next time" flag (byte 5 of the
/// guard record). One-shot on purpose: if the AP path ever crashes, the
/// following boot reads no flag and comes up as a normal station — a bad
/// AP build can't strand the device off-network.
pub fn set_force_ap() {
    write_guard(Guard {
        force_ap: true,
        ..read_guard()
    });
}

/// Read AND clear the force-AP flag.
pub fn take_force_ap() -> bool {
    let g = read_guard();
    if g.force_ap {
        write_guard(Guard {
            force_ap: false,
            ..g
        });
    }
    g.force_ap
}

fn write_boot_attempts(n: u8) {
    // preserve the other fields: the boot counter moves before the WiFi
    // path consumes the force-AP flag, and mid-takeover-retry
    write_guard(Guard {
        attempts: n,
        ..read_guard()
    });
}

/// Zero the failed-boot counter before a DELIBERATE reboot. The layout
/// migrator (migrate.rs) reboots up to twice on its way through, and a
/// migration that resumes across a power cut must not look like a
/// crash-loop to [preboot_guard] — which, once staging has begun, would
/// roll back to an ota_1 that is no longer a bootable image (Gitea #501).
pub fn clear_boot_attempts() {
    write_boot_attempts(0);
}

/// How many aborted takeover attempts have already rebooted to retry.
#[cfg_attr(not(feature = "wled-takeover"), allow(dead_code))]
pub fn takeover_retries() -> u8 {
    read_guard().takeover_retries
}

/// Record one more aborted takeover attempt, just before a deliberate
/// reboot-to-retry. Also zeroes the failed-boot counter: this boot ran to
/// a controlled abort, it didn't crash — without the clear, two retry
/// reboots would trip [boot_guard]'s rollback-to-the-other-slot (i.e.
/// straight back to WLED) before the takeover's own retry cap is reached.
#[cfg_attr(not(feature = "wled-takeover"), allow(dead_code))]
pub fn bump_takeover_retries() {
    let g = read_guard();
    write_guard(Guard {
        attempts: 0,
        takeover_retries: g.takeover_retries.saturating_add(1),
        ..g
    });
}

/// Retry budget exhausted (or takeover no longer applicable): forget the
/// counter so a later manual power cycle starts with a fresh budget.
#[cfg_attr(not(feature = "wled-takeover"), allow(dead_code))]
pub fn clear_takeover_retries() {
    let g = read_guard();
    if g.takeover_retries != 0 {
        write_guard(Guard {
            takeover_retries: 0,
            ..g
        });
    }
}

fn write_guard(g: Guard) {
    let mut rec = [0u8; 8];
    rec[0..4].copy_from_slice(GUARD_MAGIC);
    rec[4] = g.attempts;
    rec[5] = g.force_ap as u8;
    rec[6] = g.takeover_retries;
    // word-aligned stage (see config.rs for why unaligned paths are off limits)
    let mut stage = [0u32; 2];
    let bytes = unsafe { core::slice::from_raw_parts_mut(stage.as_mut_ptr().cast::<u8>(), 8) };
    bytes.copy_from_slice(&rec);
    let _ = with_flash(|f| {
        use embedded_storage::nor_flash::NorFlash;
        let _ = NorFlash::erase(f, GUARD_OFFSET, GUARD_OFFSET + 4096);
        let _ = NorFlash::write(f, GUARD_OFFSET, bytes);
    });
}

// ---- heap-free guard I/O (used by [preboot_guard], before the allocators) ----
// The record encoding matches [read_guard]/[write_guard]; only the plumbing
// differs — a borrowed FlashStorage and a stack buffer, no heap, no FLASH
// global (neither exists yet in the pre-guard window).

fn decode_guard(rec: &[u8; 8]) -> Guard {
    if &rec[0..4] != GUARD_MAGIC {
        return Guard::default();
    }
    Guard {
        attempts: rec[4],
        force_ap: rec[5] == 1,
        takeover_retries: rec[6],
    }
}

fn read_guard_raw(flash: &mut FlashStorage<'static>) -> Guard {
    // read_nor straight into a word-aligned stack buffer (GUARD_OFFSET and the
    // 8-byte length are both word-aligned), matching assets::read_chunk's
    // zero-bounce-buffer discipline.
    let mut stage = [0u32; 2];
    let bytes = unsafe { core::slice::from_raw_parts_mut(stage.as_mut_ptr().cast::<u8>(), 8) };
    if flash.read_nor(GUARD_OFFSET, bytes).is_err() {
        return Guard::default();
    }
    let mut rec = [0u8; 8];
    rec.copy_from_slice(bytes);
    decode_guard(&rec)
}

fn write_guard_raw(flash: &mut FlashStorage<'static>, g: Guard) {
    let mut rec = [0u8; 8];
    rec[0..4].copy_from_slice(GUARD_MAGIC);
    rec[4] = g.attempts;
    rec[5] = g.force_ap as u8;
    rec[6] = g.takeover_retries;
    let mut stage = [0u32; 2];
    let bytes = unsafe { core::slice::from_raw_parts_mut(stage.as_mut_ptr().cast::<u8>(), 8) };
    bytes.copy_from_slice(&rec);
    use embedded_storage::nor_flash::NorFlash;
    let _ = NorFlash::erase(flash, GUARD_OFFSET, GUARD_OFFSET + 4096);
    let _ = NorFlash::write(flash, GUARD_OFFSET, bytes);
}

/// Boot-loop guard armed BEFORE the heap allocators run.
///
/// It must run before the `heap_allocator!` calls and esp_rtos::start (which
/// itself allocates). A panic in that pre-guard window — most notably
/// esp-alloc's "Exceeded the maximum of 3 heap memory regions", which a
/// flash-read flake corrupting the HEAP static's `.data` slot array can
/// trigger on a WLED-bootloader takeover boot (reproduced under QEMU; see
/// docs/research/qemu-emulation-spike.md) — reboots via custom_halt but,
/// before this guard existed, never reached the post-[init] rollback, so a
/// *deterministic* such panic would loop forever and never roll back to the
/// working slot (WLED, on a takeover device).
///
/// This closes that gap: it increments the failed-boot counter (byte 4 of the
/// guard record), and on the third consecutive boot that never reached
/// [boot_ok] it rolls back to the other OTA slot and resets — heap-free, so
/// it is safe to call before any allocation. [boot_ok] still clears the
/// counter once the device has served for a while, and the takeover retry
/// logic still zeroes it on a deliberate retry reboot.
///
/// The flash driver is borrowed, not consumed, so it can be handed to [init]
/// once the heap is up.
pub fn preboot_guard(flash: &mut FlashStorage<'static>) {
    let g = read_guard_raw(flash);
    if g.attempts >= 2 {
        println!(
            "preboot guard: {} consecutive failed boots — rolling back to the other OTA slot",
            g.attempts
        );
        write_guard_raw(
            flash,
            Guard {
                attempts: 0,
                ..g
            },
        );
        // The stack is the ONLY option here: preboot_guard runs before the
        // `heap_allocator!` calls, so `alloc::vec!` (what every other
        // OtaUpdater site in this file uses) would allocate from a heap that
        // does not exist yet. PARTITION_TABLE_MAX_LEN is 3 KiB and this frame
        // is transient and leaf-ish — well inside the 12 KiB budget that
        // tools/stack-check.sh enforces against the whole linked image.
        #[allow(clippy::large_stack_arrays)]
        let mut buffer = [0u8; PARTITION_TABLE_MAX_LEN];
        let rolled = match OtaUpdater::new(&mut *flash, &mut buffer) {
            Ok(mut ota) => ota.activate_next_partition().is_ok(),
            Err(_) => false,
        };
        if rolled {
            esp_hal::system::software_reset();
        }
        println!("preboot guard: rollback failed; continuing with this slot");
    }
    write_guard_raw(
        flash,
        Guard {
            attempts: g.attempts + 1,
            ..g
        },
    );
}

// The failed-boot counter is incremented and acted on by [preboot_guard],
// which runs before the heap allocators; [boot_ok] clears it once the device
// has served for a while. (The old post-init boot_guard() lived here; it was
// replaced by preboot_guard so a pre-heap panic can still roll back.)

/// The device survived boot and has been serving for a while: clear the
/// failed-boot counter (and mark the image valid for rollback-capable
/// bootloaders, where the state machine expects it).
pub fn boot_ok() {
    write_boot_attempts(0);
    let _ = with_flash(|flash| {
        let mut buffer: alloc::boxed::Box<[u8; PARTITION_TABLE_MAX_LEN]> =
            alloc::vec![0u8; PARTITION_TABLE_MAX_LEN]
                .into_boxed_slice()
                .try_into()
                .unwrap();
        if let Ok(mut ota) = OtaUpdater::new(flash, &mut *buffer) {
            let _ = ota.set_current_ota_state(OtaImageState::Valid);
        }
    });
    println!("boot guard: healthy — counter cleared");
}

/// Locate a data partition by label → (offset, len). The pattern store
/// calls this to confirm its `storage` partition actually exists before
/// touching that flash: a device still carrying the old (factory) table
/// maps that address to a live app slot, where an erase would be fatal.
/// Matching by label AND data-type means the check fails safely on the old
/// table (where 0x210000 is the ota_1 *app* partition).
pub fn data_partition(label: &str) -> Option<(u32, u32)> {
    with_flash(|flash| {
        let mut buffer = alloc::vec![0u8; PARTITION_TABLE_MAX_LEN];
        let pt = partitions::read_partition_table(flash, &mut buffer).ok()?;
        // bind to a local so the iterator temporary (which borrows `pt`/
        // `buffer`) drops before this block's locals — Rust 2024 capture
        // rules otherwise extend the borrow past `buffer`'s scope.
        let found = pt
            .iter()
            .find(|e| {
                e.label_as_str() == label
                    && matches!(e.partition_type(), PartitionType::Data(_))
            })
            .map(|e| (e.offset(), e.len()));
        found
    })
    .flatten()
}

pub struct OtaWriter {
    partition_offset: u32,
    capacity: u32,
    written: u32,
    /// absolute flash offset erased so far (erase-on-write bookkeeping)
    erased_end: u32,
    slot: &'static str,
    /// The image's first sector, held back until [OtaWriter::commit] has
    /// verified everything after it — point 2 of the module doc.
    head: alloc::vec::Vec<u8>,
}

impl Drop for OtaWriter {
    fn drop(&mut self) {
        OTA_ACTIVE.store(false, Ordering::Release);
    }
}

/// Begin an update: marks the OTA active (blocking [take_flash] users) and
/// locates the slot to write — the one the device is NOT executing from
/// ([target]). Stream sectors with [OtaWriter::write]; [OtaWriter::commit]
/// verifies and activates. Dropping without commit leaves otadata untouched
/// and the half-written slot headless (its first sector is never on flash
/// before commit), so nothing can boot it by accident.
///
/// `expected` is the request's Content-Length (0 when there isn't one). The
/// slot size comes from the table ON FLASH, never from the one this image was
/// built with — those differ on a device that has not migrated, and again on
/// a 16 MB board running the 4 MB fallback layout because its bootloader was
/// flashed for a smaller part (Gitea #634). An image too big for that slot is
/// refused HERE, before a single sector is erased, and
/// [crate::parttab::oversize_message] says WHICH of the three situations it
/// is: a release image is up to 1.25 MiB (3 MiB on the Seengreat's nominal
/// table) and "it just failed" would be a mystery on exactly the devices that
/// have no serial console (Gitea #501).
pub fn begin(expected: u32) -> Result<OtaWriter, &'static str> {
    // Where to, and may we? Both are reads on the shared driver, before the
    // claim below takes it — and both refuse before a sector is touched.
    let (offset, capacity, slot) = target()?;
    if expected > capacity {
        return Err(crate::parttab::oversize_message());
    }
    if crate::migrate::ota_hold() {
        // the free slot is the migration's staging area, and right now it
        // holds the only copy of the pattern library
        return Err("layout migration is mid-flight — reboot to let it finish, then retry");
    }
    let mut head = alloc::vec::Vec::new();
    if head.try_reserve_exact(SECTOR as usize).is_err() {
        return Err("out of memory");
    }
    // claim flag + driver together inside the FLASH critical section (the
    // C3 target has no atomic swap, so the mutex provides the atomicity);
    // the driver goes straight back — the writer borrows it per op
    let claimed = FLASH.lock(|c| {
        if OTA_ACTIVE.load(Ordering::Relaxed) || c.borrow().is_none() {
            return false;
        }
        OTA_ACTIVE.store(true, Ordering::Relaxed);
        true
    });
    if !claimed {
        return Err("update already in progress");
    }
    println!("ota: writing {} at {:#x} (capacity {})", slot, offset, capacity);
    Ok(OtaWriter {
        partition_offset: offset,
        capacity,
        written: 0,
        erased_end: 0,
        slot,
        head,
    })
}

impl OtaWriter {
    #[allow(dead_code)]
    pub fn slot(&self) -> &'static str {
        self.slot
    }

    #[allow(dead_code)]
    pub fn written(&self) -> u32 {
        self.written
    }

    /// Erase the region for an incoming image of `len` bytes, yielding to
    /// the executor between sectors so the network stack keeps breathing.
    /// Write a chunk, erasing any sectors it newly touches *just before*
    /// writing them. Erasing lazily like this (rather than one long
    /// pre-erase burst) keeps each flash op sandwiched between the caller's
    /// network reads, so WiFi and the interrupt watchdog stay serviced — a
    /// tight erase burst tripped the watchdog and reset the device.
    /// Chunks should be sector-aligned in length except the last.
    pub async fn write(&mut self, chunk: &[u8]) -> Result<(), &'static str> {
        if self.written == 0 && !crate::appimg::header_ok(chunk) {
            return Err(crate::appimg::NOT_AN_IMAGE);
        }
        if self.written + chunk.len() as u32 > self.capacity {
            return Err("image larger than OTA slot");
        }
        let mut at = self.partition_offset + self.written;
        let end = at + chunk.len() as u32;
        // erase every sector in [at, end) not yet erased — borrow-per-op
        // via with_flash, byte-for-byte the assets writer's shape
        let mut s = self.erased_end.max(at & !(SECTOR - 1));
        while s < end {
            let ok = with_flash_as(crate::core1::tag::OTA_ERASE, |f| {
                embedded_storage::nor_flash::NorFlash::erase(f, s, s + SECTOR).is_ok()
            })
            .unwrap_or(false);
            if !ok {
                return Err("flash erase failed");
            }
            s += SECTOR;
            self.erased_end = s;
            embassy_futures::yield_now().await;
        }
        // The first sector stays in RAM until commit (module doc, point 2);
        // it is erased above like every other, so a wedge anywhere from here
        // on leaves a slot with no image magic in it.
        let mut chunk = chunk;
        if self.written < SECTOR {
            let take = ((SECTOR - self.written) as usize).min(chunk.len());
            self.head.extend_from_slice(&chunk[..take]);
            self.written += take as u32;
            chunk = &chunk[take..];
            at += take as u32;
        }
        let whole = chunk.len() & !3;
        if whole > 0 {
            let ok = with_flash_as(crate::core1::tag::OTA_WRITE, |f| {
                embedded_storage::nor_flash::NorFlash::write(f, at, &chunk[..whole]).is_ok()
            })
            .unwrap_or(false);
            if !ok {
                return Err("flash write failed");
            }
        }
        if whole < chunk.len() {
            let mut tail = [0xFFu8; 4];
            tail[..chunk.len() - whole].copy_from_slice(&chunk[whole..]);
            let ok = with_flash_as(crate::core1::tag::OTA_WRITE, |f| {
                embedded_storage::nor_flash::NorFlash::write(f, at + whole as u32, &tail).is_ok()
            })
            .unwrap_or(false);
            if !ok {
                return Err("flash write failed");
            }
        }
        self.written += chunk.len() as u32;
        Ok(())
    }

    /// Verify the written image, land its first sector, and only then point
    /// otadata at it. The caller reboots afterwards. `expected` is the
    /// request's Content-Length: a short body (client aborted but the reads
    /// drained cleanly) must never activate.
    pub fn commit(self, expected: u32) -> Result<u32, &'static str> {
        if self.written == 0 || self.written != expected || self.head.len() < SECTOR as usize {
            return Err("incomplete image; not activating");
        }
        // 1. the bootloader's checks, on the image as it sits: head from
        //    RAM, everything after it from the slot
        let base = self.partition_offset;
        let head = &self.head;
        crate::appimg::verify(self.written, &mut |off, buf| {
            let mut n = 0usize;
            if (off as usize) < head.len() {
                n = (head.len() - off as usize).min(buf.len());
                buf[..n].copy_from_slice(&head[off as usize..off as usize + n]);
            }
            n == buf.len() || crate::assets::read_chunk(base + off + n as u32, &mut buf[n..])
        })?;
        // 2. the head, read-back verified — the slot becomes an image here
        if !parttab::write_sector_verified(base, head, "ota") {
            return Err("flash write failed");
        }
        // 3. otadata — explicitly THIS slot, never "the next one" recomputed
        //    from otadata (with otadata erased that answers ota_0 whatever
        //    was just written, and the device would reboot into the old
        //    image believing itself updated). If otadata is erased, first
        //    record the slot we are running from so the sequence numbers
        //    start from a real slot rather than the library's factory
        //    arithmetic, then move to the target and read the choice back.
        // heap, not stack: 3 KiB frames + WiFi level-6 NMIs (which run on the
        // current task stack) overflowed the main task during flash ops.
        // into_boxed_slice, NOT Box::new([..]): the latter builds the ~3 KiB
        // array on the stack before moving it to the heap (caught by
        // clippy::large_stack_arrays) — exactly the transient stack pressure
        // the OTA path must avoid. This allocates straight on the heap.
        let mut buffer: alloc::boxed::Box<[u8; PARTITION_TABLE_MAX_LEN]> =
            alloc::vec![0u8; PARTITION_TABLE_MAX_LEN].into_boxed_slice().try_into().unwrap();
        let want = if self.slot == "ota_0" {
            AppPartitionSubType::Ota0
        } else {
            AppPartitionSubType::Ota1
        };
        let other = if want == AppPartitionSubType::Ota0 {
            AppPartitionSubType::Ota1
        } else {
            AppPartitionSubType::Ota0
        };
        with_flash(|f| {
            let mut ota =
                OtaUpdater::new(f, &mut *buffer).map_err(|_| "ota reopen failed")?;
            let mut od = ota.ota_data().map_err(|_| "ota reopen failed")?;
            let activated = (|| {
                if od.current_app_partition()? == AppPartitionSubType::Factory {
                    od.set_current_app_partition(other)?;
                }
                od.set_current_app_partition(want)?;
                if od.current_app_partition()? != want {
                    return Err(partitions::Error::Invalid);
                }
                od.set_current_ota_state(OtaImageState::New)
            })();
            activated.map_err(|_| "activate failed")?;
            Ok(self.written)
        })
        .unwrap_or(Err("flash driver unavailable"))
    }
}
