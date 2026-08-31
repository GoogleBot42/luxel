//! Web assets served from flash. The playground bundle is packed host-side
//! (web/tools/pack-assets.mjs) into a "LUXA" archive and uploaded to a raw
//! flash region behind the OTA slots — independent of app images, so a
//! firmware OTA never touches the UI and vice versa.
//!
//! Region: 0x310000..0x400000 (after ota_1; also declared in partitions.csv
//! for future serial flashes — the running code uses absolute offsets and
//! doesn't need the table entry).
//!
//! Archive layout (little-endian). Format v2 ("LUX2") adds an 8-byte content
//! hash per entry, served as a strong ETag for browser revalidation (304s):
//!   "LUX2" u32-count { u8 path_len, path, u8 ctype_len, ctype,
//!                      u8 gzip, u32 len, u32 offset, u8[8] etag } … blobs
//! Legacy "LUXA" archives (no etag field) are still read — such assets get no
//! ETag and simply revalidate to a full 200.
//! Offsets are relative to the region start.
//!
//! With the `hosted-ui` cargo feature (Gitea #11) everything below except
//! [read_chunk] compiles out: the device never reads, serves or accepts an
//! asset archive, `/` serves the embedded fallback page (which links to the
//! hosted playground), and the assets partition is left unwritten. [init]
//! stays as a one-line announcement so the mode is visible on the serial
//! console — and so tools/image-check.sh can assert which mode an image is.
//! [read_chunk] is NOT asset-specific: ota.rs and takeover.rs use it as the
//! tree's stack-safe flash reader, so it is always built.

#[cfg(not(feature = "hosted-ui"))]
use alloc::string::String;
#[cfg(not(feature = "hosted-ui"))]
use alloc::vec::Vec;
#[cfg(not(feature = "hosted-ui"))]
use core::cell::RefCell;

#[cfg(not(feature = "hosted-ui"))]
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
#[cfg(not(feature = "hosted-ui"))]
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;

#[cfg(not(feature = "hosted-ui"))]
pub const REGION_START: u32 = 0x31_0000;
#[cfg(not(feature = "hosted-ui"))]
pub const REGION_LEN: u32 = 0x0F_0000;

#[cfg(not(feature = "hosted-ui"))]
#[derive(Clone)]
pub struct AssetEntry {
    pub path: String,
    pub ctype: String,
    pub gzip: bool,
    pub len: u32,
    /// absolute flash offset of the blob
    pub offset: u32,
    /// strong ETag incl. quotes (e.g. `"a1b2c3d4e5f60718"`), or empty for a
    /// legacy archive with no hash
    pub etag: String,
}

#[cfg(not(feature = "hosted-ui"))]
static TOC: BlockingMutex<CriticalSectionRawMutex, RefCell<Vec<AssetEntry>>> =
    BlockingMutex::new(RefCell::new(Vec::new()));

#[cfg(not(feature = "hosted-ui"))]
pub fn lookup(path: &str) -> Option<AssetEntry> {
    TOC.lock(|c| c.borrow().iter().find(|e| e.path == path).cloned())
}

#[cfg(not(feature = "hosted-ui"))]
pub fn count() -> usize {
    TOC.lock(|c| c.borrow().len())
}

pub fn read_chunk(offset: u32, buf: &mut [u8]) -> bool {
    // read_nor, NOT FlashStorage::read: the Storage::read convenience path
    // puts a 4 KiB sector bounce-buffer on the CALLER's stack for every
    // call. These reads happen at maximum picoserve depth on the shared
    // main-task stack, and (with a WiFi NMI frame on top) that buffer was
    // the #1 source of the stack-guard panics. read_nor with a word-aligned
    // offset/length/buffer reads straight into the destination — zero stack
    // cost — so stage through a word-aligned heap buffer.
    let start = offset & !3;
    let head = (offset - start) as usize;
    let aligned_len = (head + buf.len() + 3) & !3;
    let mut stage = alloc::vec![0u32; aligned_len / 4];
    let stage_bytes = unsafe {
        core::slice::from_raw_parts_mut(stage.as_mut_ptr().cast::<u8>(), aligned_len)
    };
    let ok = crate::ota::with_flash(|f| f.read_nor(start, stage_bytes).is_ok()).unwrap_or(false);
    if ok {
        buf.copy_from_slice(&stage_bytes[head..head + buf.len()]);
    }
    ok
}

/// `hosted-ui` build: no on-device playground at all. One line so the mode is
/// unambiguous on the serial console and in the image (image-check asserts it).
#[cfg(feature = "hosted-ui")]
pub fn init() {
    println!("assets: hosted-ui build, no on-device web app");
}

/// (Re)parse the archive TOC from flash into RAM. Called at boot and after
/// an asset upload.
#[cfg(not(feature = "hosted-ui"))]
pub fn init() {
    let mut header = [0u8; 8];
    let magic = read_chunk(REGION_START, &mut header);
    let has_etag = &header[0..4] == b"LUX2";
    if !magic || !(has_etag || &header[0..4] == b"LUXA") {
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
        let mut etag = String::new();
        if has_etag {
            let mut hash = [0u8; 8];
            if !read_chunk(at, &mut hash) {
                return;
            }
            at += 8;
            const HEX: &[u8; 16] = b"0123456789abcdef";
            etag.push('"');
            for b in hash {
                etag.push(HEX[(b >> 4) as usize] as char);
                etag.push(HEX[(b & 0xf) as usize] as char);
            }
            etag.push('"');
        }
        entries.push(AssetEntry {
            path,
            ctype,
            gzip,
            len,
            offset: REGION_START + rel,
            etag,
        });
    }
    println!("assets: {} files installed", entries.len());
    TOC.lock(|c| *c.borrow_mut() = entries);
}

/// Streamed upload of a new archive. Erases each sector just before writing
/// it (interleaved with the caller's network reads — see ota.rs for why a
/// pre-erase burst trips the watchdog); TOC re-parse on commit.
#[cfg(not(feature = "hosted-ui"))]
pub struct AssetWriter {
    written: u32,
    expected: u32,
    erased_end: u32,
}

#[cfg(not(feature = "hosted-ui"))]
pub async fn begin(expected: u32) -> Result<AssetWriter, &'static str> {
    if expected == 0 || expected > REGION_LEN {
        return Err("archive larger than the assets region");
    }
    // invalidate the TOC while the region is inconsistent
    TOC.lock(|c| c.borrow_mut().clear());
    Ok(AssetWriter {
        written: 0,
        expected,
        erased_end: 0,
    })
}

#[cfg(not(feature = "hosted-ui"))]
impl AssetWriter {
    pub async fn write(&mut self, chunk: &[u8]) -> Result<(), &'static str> {
        if self.written == 0 && !(chunk.starts_with(b"LUXA") || chunk.starts_with(b"LUX2")) {
            return Err("not a LUXA archive (pack with web/tools/pack-assets.mjs)");
        }
        if self.written + chunk.len() as u32 > self.expected {
            return Err("archive exceeds declared length");
        }
        const SECTOR: u32 = 4096;
        let at = REGION_START + self.written;
        let end = at + chunk.len() as u32;
        let mut s = self.erased_end.max(at & !(SECTOR - 1));
        while s < end {
            let ok = crate::ota::with_flash(|f| {
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
