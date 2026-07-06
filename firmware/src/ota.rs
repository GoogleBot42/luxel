//! Over-the-air updates. `POST /api/ota` streams an app image (espflash
//! save-image output, NOT the merged full-flash image) sector-by-sector into
//! the inactive OTA slot, activates it, and reboots. The serially-flashed
//! factory partition is never written — it stays the fallback the bootloader
//! uses if both OTA slots hold broken images (or after erasing otadata).

use core::cell::RefCell;

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

pub struct OtaWriter {
    /// Some until commit/drop; Drop returns the driver to `FLASH`.
    flash: Option<FlashStorage<'static>>,
    partition_offset: u32,
    capacity: u32,
    written: u32,
    slot: &'static str,
}

impl Drop for OtaWriter {
    fn drop(&mut self) {
        if let Some(f) = self.flash.take() {
            FLASH.lock(|c| *c.borrow_mut() = Some(f));
        }
    }
}

/// Begin an update: claims the flash driver and locates the inactive slot.
/// Stream sectors with [OtaWriter::write]; [OtaWriter::commit] activates.
/// Dropping without commit leaves otadata untouched (the half-written slot
/// stays inactive).
pub fn begin() -> Result<OtaWriter, &'static str> {
    let Some(mut flash) = FLASH.lock(|c| c.borrow_mut().take()) else {
        return Err("update already in progress");
    };
    // heap, not stack: 3 KiB frames + WiFi level-6 NMIs (which run on the
    // current task stack) overflowed the main task during flash ops
    let mut buffer = alloc::boxed::Box::new([0u8; PARTITION_TABLE_MAX_LEN]);

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
    Ok(OtaWriter {
        flash: Some(flash),
        partition_offset: offset,
        capacity,
        written: 0,
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
    /// Erasing everything upfront lets [Self::write] use plain NOR writes —
    /// the per-sector read-erase-program cycle otherwise stalls the
    /// executor ~100 ms per 4 KiB and collapses TCP throughput.
    pub async fn erase(&mut self, len: u32) -> Result<(), &'static str> {
        if len == 0 || len > self.capacity {
            return Err("image larger than OTA slot");
        }
        const SECTOR: u32 = 4096;
        let end = self.partition_offset + len.div_ceil(SECTOR) * SECTOR;
        let mut at = self.partition_offset;
        let flash = self.flash.as_mut().expect("live until drop");
        while at < end {
            embedded_storage::nor_flash::NorFlash::erase(flash, at, at + SECTOR)
                .map_err(|_| "flash erase failed")?;
            at += SECTOR;
            embassy_futures::yield_now().await;
        }
        Ok(())
    }

    /// Write a chunk into the pre-erased region. Chunks must be multiples
    /// of 4 bytes except the last (which gets 0xFF-padded).
    pub fn write(&mut self, chunk: &[u8]) -> Result<(), &'static str> {
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
        let flash = self.flash.as_mut().expect("live until drop");
        let at = self.partition_offset + self.written;
        let whole = chunk.len() & !3;
        if whole > 0 {
            embedded_storage::nor_flash::NorFlash::write(flash, at, &chunk[..whole])
                .map_err(|_| "flash write failed")?;
        }
        if whole < chunk.len() {
            let mut tail = [0xFFu8; 4];
            tail[..chunk.len() - whole].copy_from_slice(&chunk[whole..]);
            embedded_storage::nor_flash::NorFlash::write(flash, at + whole as u32, &tail)
                .map_err(|_| "flash write failed")?;
        }
        self.written += chunk.len() as u32;
        Ok(())
    }

    /// Activate the freshly written slot. The caller reboots afterwards.
    /// `expected` is the request's Content-Length: a short body (client
    /// aborted but the reads drained cleanly) must never activate.
    pub fn commit(mut self, expected: u32) -> Result<u32, &'static str> {
        if self.written == 0 || self.written != expected {
            return Err("incomplete image; not activating");
        }
        let flash = self.flash.as_mut().expect("live until drop");
        // heap, not stack: 3 KiB frames + WiFi level-6 NMIs (which run on the
    // current task stack) overflowed the main task during flash ops
    let mut buffer = alloc::boxed::Box::new([0u8; PARTITION_TABLE_MAX_LEN]);
        let mut ota =
            OtaUpdater::new(flash, &mut *buffer).map_err(|_| "ota reopen failed")?;
        ota.activate_next_partition()
            .and_then(|_| ota.set_current_ota_state(OtaImageState::New))
            .map_err(|_| "activate failed")?;
        Ok(self.written)
    }
}
