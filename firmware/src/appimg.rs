//! App-image sanity for `/api/ota`: the checks the ESP-IDF second-stage
//! bootloader makes before it jumps, run by the firmware BEFORE `otadata`
//! is pointed at a freshly written slot (Gitea #655).
//!
//! Pure — `alloc` and nothing else, no flash, no esp-hal — so
//! `tools/parttab-check` compiles this exact file for the host and tests it
//! against synthetic images, including the one that bricked the Seengreat
//! panel: a slot whose segment table is one image's head over another's
//! tail. The bootloader does not fail such an image gracefully; it hits
//! `assert(load_end > load_addr)` in `verify_load_addresses`, resets, and
//! loops forever without ever trying the other slot. So an image must be
//! proven whole here, where a refusal costs one HTTP error.
//!
//! What is checked (the on-flash format is ESP-IDF's `esp_image_header_t`):
//!
//! * 24-byte header: `0xE9` magic, 1..=16 segments, `hash_appended` flag;
//!   the `esp_app_desc_t` magic word at image offset `0x20`.
//! * every 8-byte segment header (`load_addr`, `data_len`) lies inside the
//!   image, its data does too, and `load_addr + data_len` does not wrap —
//!   the bootloader's assertion, as an error.
//! * the image's byte length is EXACTLY what the segment table implies:
//!   segments, one checksum byte padded to 16, plus the 32-byte SHA-256
//!   when the header says one is appended. A truncated upload, or one with
//!   trailing junk, fails here even if every segment parses.
//! * the checksum byte: XOR of every segment data byte seeded with `0xEF`
//!   (`ESP_ROM_CHECKSUM_INITIAL`), which is what `process_checksum` in
//!   `esp_image_format.c` verifies. Not the SHA-256 — that costs a hash
//!   implementation the image cannot afford, and the checksum already
//!   catches every torn or stale sector.

const MAGIC: u8 = 0xE9;
const DESC_MAGIC: [u8; 4] = [0x32, 0x54, 0xCD, 0xAB];
const HDR_LEN: u32 = 24;
const SEG_HDR_LEN: u32 = 8;
const MAX_SEGMENTS: u8 = 16;
const CHECKSUM_SEED: u8 = 0xEF;
const HASH_LEN: u32 = 32;
/// Read granularity for the checksum pass — one flash sector per fenced read.
const CHUNK: usize = 4096;

/// Does `head` (the first bytes of an upload) start like an app image?
/// Image magic AND the `esp_app_desc` magic word at file offset `0x20` —
/// a lone `0xE9` first byte let garbage through once.
pub fn header_ok(head: &[u8]) -> bool {
    head.len() >= 0x24 && head[0] == MAGIC && head[0x20..0x24] == DESC_MAGIC
}

/// Verify the `len`-byte image that `read(offset, buf)` serves (offsets are
/// image-relative; `read` answers false on a failed flash read).
///
/// Every refusal is a `&'static str` for the HTTP error, and none of them
/// has been written yet when this runs — the caller activates only on `Ok`.
pub fn verify(len: u32, read: &mut dyn FnMut(u32, &mut [u8]) -> bool) -> Result<(), &'static str> {
    let mut hdr = [0u8; 0x24];
    if len < 0x24 || !read(0, &mut hdr) {
        return Err("image unreadable after write");
    }
    if !header_ok(&hdr) {
        return Err(NOT_AN_IMAGE);
    }
    let segments = hdr[1];
    if segments == 0 || segments > MAX_SEGMENTS {
        return Err(CORRUPT);
    }
    let hash_appended = hdr[23] == 1;

    let mut buf = alloc::vec![0u8; CHUNK];
    let mut sum = CHECKSUM_SEED;
    let mut pos = HDR_LEN;
    for _ in 0..segments {
        let mut seg = [0u8; 8];
        if pos + SEG_HDR_LEN > len || !read(pos, &mut seg) {
            return Err(CORRUPT);
        }
        let load = u32::from_le_bytes(seg[0..4].try_into().unwrap());
        let size = u32::from_le_bytes(seg[4..8].try_into().unwrap());
        let data = pos + SEG_HDR_LEN;
        // the bootloader's `load_end > load_addr` assertion, and the data
        // must lie inside what was actually uploaded
        let Some(end) = data.checked_add(size) else { return Err(CORRUPT) };
        if end > len || load.checked_add(size).is_none() {
            return Err(CORRUPT);
        }
        let mut at = data;
        while at < end {
            let n = ((end - at) as usize).min(CHUNK);
            if !read(at, &mut buf[..n]) {
                return Err("image unreadable after write");
            }
            for b in &buf[..n] {
                sum ^= *b;
            }
            at += n as u32;
        }
        pos = end;
    }
    // one checksum byte, padded so the image ends on a 16-byte boundary
    let Some(padded) = pos.checked_add(1 + 15).map(|p| p & !15) else { return Err(CORRUPT) };
    let total = if hash_appended { padded + HASH_LEN } else { padded };
    if total != len {
        return Err("image length does not match its segment table");
    }
    let mut ck = [0u8; 1];
    if !read(padded - 1, &mut ck) {
        return Err("image unreadable after write");
    }
    if ck[0] != sum {
        return Err("image checksum mismatch");
    }
    Ok(())
}

/// The one refusal `/api/ota` has always had for a body that is not an
/// app image, shared so the literal is linked once.
pub const NOT_AN_IMAGE: &str =
    "not an app image (send espflash save-image output, not the merged image)";
const CORRUPT: &str = "corrupt segment table";
