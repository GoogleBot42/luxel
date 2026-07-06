//! Over-the-air updates. `POST /api/ota` streams an app image (espflash
//! save-image output, NOT the merged full-flash image) sector-by-sector into
//! the inactive OTA slot, activates it, and reboots. The serially-flashed
//! factory partition is never written — it stays the fallback the bootloader
//! uses if both OTA slots hold broken images (or after erasing otadata).

use core::cell::RefCell;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embedded_storage::Storage;
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
    let mut buffer = [0u8; PARTITION_TABLE_MAX_LEN];
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
    let mut buffer = [0u8; PARTITION_TABLE_MAX_LEN];

    // which slot is next?
    let next = {
        let mut ota = match OtaUpdater::new(&mut flash, &mut buffer) {
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
    let entry = partitions::read_partition_table(&mut flash, &mut buffer)
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
    pub fn slot(&self) -> &'static str {
        self.slot
    }

    pub fn written(&self) -> u32 {
        self.written
    }

    pub fn write(&mut self, chunk: &[u8]) -> Result<(), &'static str> {
        if self.written == 0 && chunk.first() != Some(&0xE9) {
            return Err("not an app image (send espflash save-image output, not the merged image)");
        }
        if self.written + chunk.len() as u32 > self.capacity {
            return Err("image larger than OTA slot");
        }
        self.flash
            .as_mut()
            .expect("live until drop")
            .write(self.partition_offset + self.written, chunk)
            .map_err(|_| "flash write failed")?;
        self.written += chunk.len() as u32;
        Ok(())
    }

    /// Activate the freshly written slot. The caller reboots afterwards.
    pub fn commit(mut self) -> Result<u32, &'static str> {
        if self.written == 0 {
            return Err("empty image");
        }
        let flash = self.flash.as_mut().expect("live until drop");
        let mut buffer = [0u8; PARTITION_TABLE_MAX_LEN];
        let mut ota =
            OtaUpdater::new(flash, &mut buffer).map_err(|_| "ota reopen failed")?;
        ota.activate_next_partition()
            .and_then(|_| ota.set_current_ota_state(OtaImageState::New))
            .map_err(|_| "activate failed")?;
        Ok(self.written)
    }
}
