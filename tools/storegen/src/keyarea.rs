//! The store's KEY AREA — the first `KEY_AREA_LEN` bytes of the `storage`
//! partition — written and read with the **real** `sequential-storage`
//! crate at the version `firmware/Cargo.toml` pins.
//!
//! This is the half of the store the migration carries across by *value*
//! (`patterns::read_blob_at` → `patterns::store_blob_at`, key by key, over
//! the whole reserved range), so the post-migration bytes are NOT a copy of
//! the pre-migration ones — they are a fresh sequential-storage region
//! holding the same items. Asserting on it therefore needs a real reader,
//! which is exactly what this module is.
//!
//! The `MemFlash` below mirrors `esp-storage`'s `FlashStorage` word sizes
//! (`WORD_SIZE = 4`, `SECTOR_SIZE = 4096`, and `READ_SIZE = WORD_SIZE`
//! because the firmware does not enable esp-storage's `bytewise-read`
//! feature). sequential-storage derives its on-flash alignment from
//! `max(WRITE_SIZE, READ_SIZE)`, so those three constants are what make a
//! host-written key area byte-identical to a device-written one. It also
//! enforces NOR semantics — erase sets 0xFF, a write only clears bits, an
//! unaligned write panics — so a divergence shows up here rather than as a
//! mystery on the emulator.

use embassy_futures::block_on;
use embedded_storage_async::nor_flash as anf;
use sequential_storage::cache::PageStateCache;
use sequential_storage::map;

/// `patterns::STORE_LEN` / `patterns::KEY_AREA_LEN` — fixed on every layout,
/// which is what lets one `PageStateCache<PAGES>` serve both the old and the
/// new region during a migration.
pub const KEY_AREA_LEN: u32 = 0x2_0000;
pub const PAGE: u32 = 4096;
pub const PAGES: usize = (KEY_AREA_LEN / PAGE) as usize;
/// `patterns::BUF` — sequential-storage scratch, one whole page.
const BUF: usize = 4096;

#[derive(Debug)]
pub struct MemErr;
impl anf::NorFlashError for MemErr {
    fn kind(&self) -> embedded_storage_async::nor_flash::NorFlashErrorKind {
        embedded_storage_async::nor_flash::NorFlashErrorKind::Other
    }
}

/// A NOR flash in a `Vec<u8>`, addressed from 0.
pub struct MemFlash {
    pub mem: Vec<u8>,
}

impl MemFlash {
    pub fn erased(len: u32) -> MemFlash {
        MemFlash { mem: vec![0xFF; len as usize] }
    }
    pub fn from_bytes(bytes: &[u8]) -> MemFlash {
        MemFlash { mem: bytes.to_vec() }
    }
}

impl anf::ErrorType for MemFlash {
    type Error = MemErr;
}

impl anf::ReadNorFlash for MemFlash {
    // esp-storage: READ_SIZE == WORD_SIZE == 4 without `bytewise-read`.
    const READ_SIZE: usize = 4;
    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), MemErr> {
        assert_eq!(offset % 4, 0, "read offset {:#x} is not word aligned", offset);
        assert_eq!(bytes.len() % 4, 0, "read length {} is not a word multiple", bytes.len());
        let at = offset as usize;
        if at + bytes.len() > self.mem.len() {
            return Err(MemErr);
        }
        bytes.copy_from_slice(&self.mem[at..at + bytes.len()]);
        Ok(())
    }
    fn capacity(&self) -> usize {
        self.mem.len()
    }
}

impl anf::NorFlash for MemFlash {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = PAGE as usize;
    async fn erase(&mut self, from: u32, to: u32) -> Result<(), MemErr> {
        assert_eq!(from % PAGE, 0, "erase from {:#x} is not sector aligned", from);
        assert_eq!(to % PAGE, 0, "erase to {:#x} is not sector aligned", to);
        if to as usize > self.mem.len() || from > to {
            return Err(MemErr);
        }
        self.mem[from as usize..to as usize].fill(0xFF);
        Ok(())
    }
    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), MemErr> {
        assert_eq!(offset % 4, 0, "write offset {:#x} is not word aligned", offset);
        assert_eq!(bytes.len() % 4, 0, "write length {} is not a word multiple", bytes.len());
        let at = offset as usize;
        if at + bytes.len() > self.mem.len() {
            return Err(MemErr);
        }
        for (i, &b) in bytes.iter().enumerate() {
            // NOR: a program can only clear bits.
            self.mem[at + i] &= b;
        }
        Ok(())
    }
}

impl anf::MultiwriteNorFlash for MemFlash {}

/// Write one reserved-key blob, the way `patterns::store_blob_at` does.
pub fn store_blob(f: &mut MemFlash, cache: &mut PageStateCache<PAGES>, key: u32, value: &[u8]) {
    let mut buf = vec![0u8; BUF];
    let v: &[u8] = value;
    block_on(map::store_item(f, 0..KEY_AREA_LEN, cache, &mut buf, &key, &v))
        .unwrap_or_else(|e| panic!("store_item({:#x}): {:?}", key, e));
}

/// Read one back, the way `patterns::read_blob_at` does. `None` = no item.
pub fn read_blob(f: &mut MemFlash, key: u32) -> Option<Vec<u8>> {
    let mut cache = PageStateCache::<PAGES>::new();
    let mut buf = vec![0u8; BUF];
    match block_on(map::fetch_item::<u32, &[u8], _>(
        f,
        0..KEY_AREA_LEN,
        &mut cache,
        &mut buf,
        &key,
    )) {
        Ok(Some(b)) => Some(b.to_vec()),
        _ => None,
    }
}
