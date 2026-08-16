//! Over-the-air updates. `POST /api/ota` streams an app image (espflash
//! save-image output, NOT the merged full-flash image) sector-by-sector into
//! the inactive OTA slot, activates it, and reboots. Pure A/B: there is no
//! factory partition (see partitions.csv) — ota_0/ota_1 alternate, and if
//! both hold broken images (or otadata is erased) the bootloader falls back
//! to ota_0, its default when no factory partition exists. Serial flashes
//! land in ota_0.
//!
//! Division of labor with esp-bootloader-esp-idf (reviewed 2026-07-06 after
//! the "is this reinventing a library wheel?" question): the library does
//! everything stateful — slot selection (`OtaUpdater::next_partition`),
//! otadata activation, image state — and only the streaming erase+write
//! loop is ours, for two reasons. (1) `next_partition()`'s `FlashRegion`
//! borrows the updater, which borrows flash + a table buffer — holding one
//! across await points while the driver must remain claimable fights the
//! borrow checker and our driver-handoff pattern; reopening it per chunk
//! would re-read the partition table ~200× per upload. (2) The write loop
//! is where the flash-vs-WiFi timing constraints live (erase-on-write,
//! yields), which is policy the library rightly doesn't own. The crashes
//! that prompted the question were the task-stack architecture (see
//! assets::read_chunk), not this loop.

use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::ota_updater::OtaUpdater;
use esp_bootloader_esp_idf::partitions::{
    self, AppPartitionSubType, PartitionType, PARTITION_TABLE_MAX_LEN,
};
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
    let booted = match partitions::read_partition_table(&mut flash, &mut buffer) {
        Ok(pt) => match pt.booted_partition() {
            Ok(Some(p)) => match p.partition_type() {
                PartitionType::App(sub) => slot_name(sub),
                _ => "unknown",
            },
            _ => "unknown",
        },
        Err(_) => "unknown",
    };
    BOOTED.lock(|c| *c.borrow_mut() = booted);
    println!("booted from: {}", booted);
    FLASH.lock(|c| *c.borrow_mut() = Some(flash));
}

pub fn booted_slot() -> &'static str {
    BOOTED.lock(|c| *c.borrow())
}

/// Borrow the flash driver briefly (reads, or writes outside an OTA).
/// Returns None while an OTA holds the driver.
pub fn with_flash<T>(f: impl FnOnce(&mut FlashStorage<'static>) -> T) -> Option<T> {
    FLASH.lock(|c| c.borrow_mut().as_mut().map(f))
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

fn read_guard() -> (u8, bool) {
    let mut rec = [0u8; 8];
    if !crate::assets::read_chunk(GUARD_OFFSET, &mut rec) || &rec[0..4] != GUARD_MAGIC {
        return (0, false);
    }
    (rec[4], rec[5] == 1)
}

fn read_boot_attempts() -> u8 {
    read_guard().0
}

/// One-shot "boot into the provisioning AP next time" flag (byte 5 of the
/// guard record). One-shot on purpose: if the AP path ever crashes, the
/// following boot reads no flag and comes up as a normal station — a bad
/// AP build can't strand the device off-network.
pub fn set_force_ap() {
    write_guard(read_boot_attempts(), true);
}

/// Read AND clear the force-AP flag.
pub fn take_force_ap() -> bool {
    let (count, ap) = read_guard();
    if ap {
        write_guard(count, false);
    }
    ap
}

fn write_boot_attempts(n: u8) {
    // preserve the force-AP flag: the boot counter moves before the WiFi
    // path consumes the flag with take_force_ap
    write_guard(n, read_guard().1);
}

fn write_guard(n: u8, force_ap: bool) {
    let mut rec = [0u8; 8];
    rec[0..4].copy_from_slice(GUARD_MAGIC);
    rec[4] = n;
    rec[5] = force_ap as u8;
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

/// Call early in boot, after [init] but before WiFi/radio setup. On the
/// third consecutive boot that never reached [boot_ok], activates the other
/// OTA slot and resets.
pub fn boot_guard() {
    let attempts = read_boot_attempts();
    if attempts >= 2 {
        println!(
            "boot guard: {} consecutive failed boots — rolling back to the other OTA slot",
            attempts
        );
        write_boot_attempts(0);
        let ok = with_flash(|flash| {
            let mut buffer: alloc::boxed::Box<[u8; PARTITION_TABLE_MAX_LEN]> = alloc::vec![
                    0u8;
                    PARTITION_TABLE_MAX_LEN
                ]
            .into_boxed_slice()
            .try_into()
            .unwrap();
            let Ok(mut ota) = OtaUpdater::new(flash, &mut *buffer) else {
                return false;
            };
            ota.activate_next_partition().is_ok()
        })
        .unwrap_or(false);
        if ok {
            esp_hal::system::software_reset();
        }
        println!("boot guard: rollback failed; continuing with this slot");
    }
    write_boot_attempts(attempts + 1);
}

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
}

impl Drop for OtaWriter {
    fn drop(&mut self) {
        OTA_ACTIVE.store(false, Ordering::Release);
    }
}

/// Begin an update: marks the OTA active (blocking [take_flash] users) and
/// locates the inactive slot. Stream sectors with [OtaWriter::write];
/// [OtaWriter::commit] activates. Dropping without commit leaves otadata
/// untouched (the half-written slot stays inactive).
pub fn begin() -> Result<OtaWriter, &'static str> {
    // claim flag + driver together inside the FLASH critical section (the
    // C3 target has no atomic swap, so the mutex provides the atomicity)
    let claimed = FLASH.lock(|c| {
        if OTA_ACTIVE.load(Ordering::Relaxed) {
            return None;
        }
        let f = c.borrow_mut().take()?;
        OTA_ACTIVE.store(true, Ordering::Relaxed);
        Some(f)
    });
    let Some(mut flash) = claimed else {
        return Err("update already in progress");
    };
    // released on every early-out below; OtaWriter's Drop takes over after
    struct ActiveGuard(bool);
    impl Drop for ActiveGuard {
        fn drop(&mut self) {
            if self.0 {
                OTA_ACTIVE.store(false, Ordering::Release);
            }
        }
    }
    let mut guard = ActiveGuard(true);
    // Heap, not stack (3 KiB frames + WiFi level-6 NMIs on the current task
    // stack overflowed the main task during flash ops). into_boxed_slice,
    // NOT Box::new([..]): the latter builds the array on the stack first
    // before moving it to the heap — caught by clippy::large_stack_arrays.
    let mut buffer: alloc::boxed::Box<[u8; PARTITION_TABLE_MAX_LEN]> =
        alloc::vec![0u8; PARTITION_TABLE_MAX_LEN].into_boxed_slice().try_into().unwrap();

    // which slot is next?
    let next = {
        let mut ota = match OtaUpdater::new(&mut flash, &mut *buffer) {
            Ok(o) => o,
            Err(_) => {
                FLASH.lock(|c| *c.borrow_mut() = Some(flash));
                return Err("no OTA partitions (old partition table? reflash serially)");
            }
        };
        match ota.next_partition() {
            Ok((_, sub)) => sub,
            Err(_) => {
                FLASH.lock(|c| *c.borrow_mut() = Some(flash));
                return Err("cannot determine next OTA slot");
            }
        }
    };

    // raw offset/size of that slot, so chunks write at absolute addresses
    let entry = partitions::read_partition_table(&mut flash, &mut *buffer)
        .ok()
        .and_then(|pt| pt.find_partition(PartitionType::App(next)).ok().flatten())
        .map(|p| (p.offset(), p.len()));
    let Some((offset, capacity)) = entry else {
        FLASH.lock(|c| *c.borrow_mut() = Some(flash));
        return Err("next OTA slot missing from partition table");
    };

    let slot = slot_name(next);
    println!("ota: writing {} at {:#x} (capacity {})", slot, offset, capacity);
    // the driver goes back to the global — the writer borrows it per op
    FLASH.lock(|c| *c.borrow_mut() = Some(flash));
    guard.0 = false; // the OtaWriter's Drop owns the flag from here
    Ok(OtaWriter {
        partition_offset: offset,
        capacity,
        written: 0,
        erased_end: 0,
        slot,
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
        if self.written == 0 {
            // image header magic AND the esp_app_desc magic word at file
            // offset 0x20 — a lone 0xE9 first byte let garbage through once
            let desc_ok = chunk.len() >= 0x24 && chunk[0x20..0x24] == [0x32, 0x54, 0xCD, 0xAB];
            if chunk.first() != Some(&0xE9) || !desc_ok {
                return Err("not an app image (send espflash save-image output, not the merged image)");
            }
        }
        if self.written + chunk.len() as u32 > self.capacity {
            return Err("image larger than OTA slot");
        }
        const SECTOR: u32 = 4096;
        let at = self.partition_offset + self.written;
        let end = at + chunk.len() as u32;
        // erase every sector in [at, end) not yet erased — borrow-per-op
        // via with_flash, byte-for-byte the assets writer's shape
        let mut s = self.erased_end.max(at & !(SECTOR - 1));
        while s < end {
            let ok = with_flash(|f| {
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
        let whole = chunk.len() & !3;
        if whole > 0 {
            let ok = with_flash(|f| {
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
            let ok = with_flash(|f| {
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

    /// Activate the freshly written slot. The caller reboots afterwards.
    /// `expected` is the request's Content-Length: a short body (client
    /// aborted but the reads drained cleanly) must never activate.
    pub fn commit(self, expected: u32) -> Result<u32, &'static str> {
        if self.written == 0 || self.written != expected {
            return Err("incomplete image; not activating");
        }
        // heap, not stack: 3 KiB frames + WiFi level-6 NMIs (which run on the
        // current task stack) overflowed the main task during flash ops.
        // into_boxed_slice, NOT Box::new([..]): the latter builds the ~3 KiB
        // array on the stack before moving it to the heap (caught by
        // clippy::large_stack_arrays) — exactly the transient stack pressure
        // the OTA path must avoid. This allocates straight on the heap.
        let mut buffer: alloc::boxed::Box<[u8; PARTITION_TABLE_MAX_LEN]> =
            alloc::vec![0u8; PARTITION_TABLE_MAX_LEN].into_boxed_slice().try_into().unwrap();
        with_flash(|f| {
            let mut ota =
                OtaUpdater::new(f, &mut *buffer).map_err(|_| "ota reopen failed")?;
            ota.activate_next_partition()
                .and_then(|_| ota.set_current_ota_state(OtaImageState::New))
                .map_err(|_| "activate failed")?;
            Ok(self.written)
        })
        .unwrap_or(Err("flash driver unavailable"))
    }
}
