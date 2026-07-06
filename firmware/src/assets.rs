//! Web assets served from flash. The playground bundle is packed host-side
//! (web/tools/pack-assets.mjs) into a "LUXA" archive and uploaded to a raw
//! flash region behind the OTA slots — independent of app images, so a
//! firmware OTA never touches the UI and vice versa.
//!
//! Region: 0x310000..0x400000 (after ota_1; also declared in partitions.csv
//! for future serial flashes — the running code uses absolute offsets and
//! doesn't need the table entry).
//!
//! Archive layout (little-endian):
//!   "LUXA" u32-count { u8 path_len, path, u8 ctype_len, ctype,
//!                      u8 gzip, u32 len, u32 offset } … blobs
//! Offsets are relative to the region start.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embedded_storage::ReadStorage;
use esp_println::println;

pub const REGION_START: u32 = 0x31_0000;
pub const REGION_LEN: u32 = 0x0F_0000;

#[derive(Clone)]
pub struct AssetEntry {
    pub path: String,
    pub ctype: String,
    pub gzip: bool,
    pub len: u32,
    /// absolute flash offset of the blob
    pub offset: u32,
}

static TOC: BlockingMutex<CriticalSectionRawMutex, RefCell<Vec<AssetEntry>>> =
    BlockingMutex::new(RefCell::new(Vec::new()));

pub fn lookup(path: &str) -> Option<AssetEntry> {
    TOC.lock(|c| c.borrow().iter().find(|e| e.path == path).cloned())
}

pub fn count() -> usize {
    TOC.lock(|c| c.borrow().len())
}

pub fn read_chunk(offset: u32, buf: &mut [u8]) -> bool {
    crate::ota::with_flash(|f| f.read(offset, buf).is_ok()).unwrap_or(false)
}

/// (Re)parse the archive TOC from flash into RAM. Called at boot and after
/// an asset upload.
pub fn init() {
    let mut header = [0u8; 8];
    if !read_chunk(REGION_START, &mut header) || &header[0..4] != b"LUXA" {
        println!("assets: none installed");
        TOC.lock(|c| c.borrow_mut().clear());
        return;
    }
    let n = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
    if n > 64 {
        println!("assets: implausible entry count {}", n);
        return;
    }
    let mut entries = Vec::new();
    let mut at = REGION_START + 8;
    for _ in 0..n {
        let mut b1 = [0u8; 1];
        if !read_chunk(at, &mut b1) {
            return;
        }
        let plen = b1[0] as u32;
        let mut pbuf = alloc::vec![0u8; plen as usize];
        read_chunk(at + 1, &mut pbuf);
        let path = String::from_utf8_lossy(&pbuf).into_owned();
        at += 1 + plen;
        if !read_chunk(at, &mut b1) {
            return;
        }
        let clen = b1[0] as u32;
        let mut cbuf = alloc::vec![0u8; clen as usize];
        read_chunk(at + 1, &mut cbuf);
        let ctype = String::from_utf8_lossy(&cbuf).into_owned();
        at += 1 + clen;
        let mut meta = [0u8; 9];
        if !read_chunk(at, &mut meta) {
            return;
        }
        at += 9;
        let gzip = meta[0] == 1;
        let len = u32::from_le_bytes(meta[1..5].try_into().unwrap());
        let rel = u32::from_le_bytes(meta[5..9].try_into().unwrap());
        entries.push(AssetEntry {
            path,
            ctype,
            gzip,
            len,
            offset: REGION_START + rel,
        });
    }
    println!("assets: {} files installed", entries.len());
    TOC.lock(|c| *c.borrow_mut() = entries);
}

/// Streamed upload of a new archive, mirroring the OTA writer: pre-erase
/// (sized by Content-Length), plain NOR writes, TOC re-parse on success.
pub struct AssetWriter {
    written: u32,
    expected: u32,
}

pub async fn begin(expected: u32) -> Result<AssetWriter, &'static str> {
    if expected == 0 || expected > REGION_LEN {
        return Err("archive larger than the assets region");
    }
    // invalidate the TOC while the region is inconsistent
    TOC.lock(|c| c.borrow_mut().clear());
    const SECTOR: u32 = 4096;
    let end = REGION_START + expected.div_ceil(SECTOR) * SECTOR;
    let mut at = REGION_START;
    while at < end {
        let ok = crate::ota::with_flash(|f| {
            embedded_storage::nor_flash::NorFlash::erase(f, at, at + SECTOR).is_ok()
        })
        .unwrap_or(false);
        if !ok {
            return Err("flash erase failed");
        }
        at += SECTOR;
        embassy_futures::yield_now().await;
    }
    Ok(AssetWriter {
        written: 0,
        expected,
    })
}

impl AssetWriter {
    pub fn write(&mut self, chunk: &[u8]) -> Result<(), &'static str> {
        if self.written == 0 && !chunk.starts_with(b"LUXA") {
            return Err("not a LUXA archive (pack with web/tools/pack-assets.mjs)");
        }
        if self.written + chunk.len() as u32 > self.expected {
            return Err("archive exceeds declared length");
        }
        let at = REGION_START + self.written;
        let whole = chunk.len() & !3;
        let ok = crate::ota::with_flash(|f| {
            use embedded_storage::nor_flash::NorFlash;
            if whole > 0 && NorFlash::write(f, at, &chunk[..whole]).is_err() {
                return false;
            }
            if whole < chunk.len() {
                let mut tail = [0xFFu8; 4];
                tail[..chunk.len() - whole].copy_from_slice(&chunk[whole..]);
                if NorFlash::write(f, at + whole as u32, &tail).is_err() {
                    return false;
                }
            }
            true
        })
        .unwrap_or(false);
        if !ok {
            return Err("flash write failed");
        }
        self.written += chunk.len() as u32;
        Ok(())
    }

    pub fn commit(self) -> Result<u32, &'static str> {
        if self.written != self.expected {
            return Err("incomplete archive; not installing");
        }
        init();
        if count() == 0 {
            return Err("archive did not parse after write");
        }
        Ok(self.written)
    }
}
