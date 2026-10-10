//! Minimal read-only ESP-IDF NVS reader — just enough to lift a stock
//! Pixelblaze v3's WiFi station credentials off the `nvs` partition during
//! a Pixelblaze→Luxel takeover (the PB mirror of the WLED inheritance in
//! `wledfs.rs`). PB keeps the creds where every ESP-IDF device does:
//! namespace `nvs.net80211`, blob keys `sta.ssid` / `sta.pswd`.
//!
//! Deliberately best-effort, same posture as `wledfs.rs`: entry/blob CRCs
//! are NOT verified (a torn write can only yield a missing key or garbage
//! bytes, and every failure path lands in the provisioning-AP fallback),
//! and anything that doesn't parse as a clean NVS page is skipped. The
//! whole parser reads through a caller-supplied region-relative read
//! callback and has no esp-hal dependencies, so the exact same file is
//! compiled and tested on the host against a real 4 MiB flash dump
//! (tools/pbnvs-check).
//!
//! The one PB-specific hazard this must survive: on Luxel's first boot
//! `ota::preboot_guard` erases and rewrites absolute flash 0xC000 (one of
//! PB's five NVS pages — partition-relative 0x3000) BEFORE the takeover
//! reads NVS. So at read time one 4096-byte page may be erased (all 0xFF)
//! or carry a non-NVS "LXBG" guard record. The reader scans every page,
//! ignores any that don't look like a written NVS page, and reassembles the
//! creds from whichever pages survive — in a stock dump the station creds
//! live on the 0xA000/0xB000 pages, so the 0xC000 wipe can't touch them.
//!
//! Format notes (ESP-IDF `nvs_flash`): the partition is a run of 4096-byte
//! pages. Each page = a 32-byte header (state u32 @0, seqno u32 @4,
//! version u8 @8; 0xFF=v1, 0xFE=v2) + a 32-byte entry-state bitmap @32
//! (2 bits/entry, 0b11 empty, 0b10 written, 0b00 erased) + 126 × 32-byte
//! entries @64. An entry is {ns_index u8 @0, type u8 @1, span u8 @2,
//! chunk_index u8 @3, crc32 u32 @4, key[16] @8 (NUL-padded ASCII),
//! data[8] @24}. Fixed primitives keep their value in data[8]. STR/BLOB
//! keep {size u16 @24, …} there and the actual bytes in the next `span-1`
//! 32-byte slots. A namespace registration entry has ns_index 0, type U8,
//! key = the namespace NAME, and value = the index assigned to it.

use alloc::string::String;
use alloc::vec::Vec;

/// Region-relative flash read (offset 0 = the NVS partition start, i.e.
/// absolute 0x9000). Returns false on failure. Mirrors `wledfs::ReadFn`.
pub type ReadFn<'a> = &'a mut dyn FnMut(u32, &mut [u8]) -> bool;

const PAGE_SIZE: u32 = 4096;
const HEADER_LEN: usize = 64; // 32-byte page header + 32-byte entry bitmap
const ENTRY_SIZE: usize = 32;
const ENTRY_COUNT: usize = 126;

const PAGE_UNINIT: u32 = 0xFFFF_FFFF; // state of a never-written / erased page

const ST_WRITTEN: u8 = 0b10; // entry-state bitmap code for a live entry

// Entry type codes (nvs_types.hpp).
const T_U8: u8 = 0x01;
const T_STR: u8 = 0x21;
const T_BLOB_DATA: u8 = 0x42; // blob payload chunk
#[allow(dead_code)] // documents the index type gather_value deliberately skips; used in tests
const T_BLOB_IDX: u8 = 0x48; // blob index (metadata only, no inline payload)

/// Reader over an ESP-IDF NVS partition. Holds the caller's read callback
/// and the list of parseable pages ordered by sequence number (newest
/// first) so duplicate keys resolve to the most recent write.
pub struct PbNvs<'a> {
    read: ReadFn<'a>,
    /// (page_index, seqno) for every page that looks like a written NVS
    /// page, sorted by seqno descending.
    pages: Vec<(u32, u32)>,
}

impl<'a> PbNvs<'a> {
    /// Scan the partition's page headers. A page is kept only if its
    /// version byte is a known NVS version (v1/v2) and its state is not the
    /// erased sentinel — this drops both a `preboot_guard`-wiped page (all
    /// 0xFF → state 0xFFFFFFFF) and an "LXBG"-style guard record (foreign
    /// version byte).
    pub fn open(read: ReadFn<'a>, region_len: u32) -> PbNvs<'a> {
        let n_pages = region_len / PAGE_SIZE;
        let mut pages: Vec<(u32, u32)> = Vec::new();
        let mut hdr = [0u8; 12];
        for p in 0..n_pages {
            if !(read)(p * PAGE_SIZE, &mut hdr) {
                continue;
            }
            let state = u32::from_le_bytes(hdr[0..4].try_into().unwrap());
            let seqno = u32::from_le_bytes(hdr[4..8].try_into().unwrap());
            let version = hdr[8];
            if state != PAGE_UNINIT && (version == 0xFE || version == 0xFF) {
                pages.push((p, seqno));
            }
        }
        // Newest first; a key written on a higher-seqno page supersedes any
        // stale copy on an older one.
        pages.sort_by(|a, b| b.1.cmp(&a.1));
        PbNvs { read, pages }
    }

    fn read_page(&mut self, page: u32) -> Option<Vec<u8>> {
        let mut buf = alloc::vec![0u8; PAGE_SIZE as usize];
        (self.read)(page * PAGE_SIZE, &mut buf).then_some(buf)
    }

    /// Resolve a namespace name to its index: the newest written U8 entry
    /// with ns_index 0 whose key equals `name`.
    pub fn namespace_index(&mut self, name: &str) -> Option<u8> {
        let pages = self.pages.clone();
        for (p, _) in pages {
            let Some(buf) = self.read_page(p) else { continue };
            for e in WrittenEntries::new(&buf) {
                let ent = entry_slice(&buf, e);
                if ent[0] == 0 && ent[1] == T_U8 && key_eq(ent, name) {
                    return Some(ent[24]); // value in data[0]
                }
            }
        }
        None
    }

    /// Reassemble the payload bytes for `(ns, key)` from the newest page
    /// that carries a written STR or BLOB value for it. BLOB payloads may
    /// be split across several `0x42` chunks (chunk_index order); STR is a
    /// single run. The `0x48` blob-index entry carries no payload and is
    /// ignored — the chunks are self-describing (each header's size field
    /// bounds its own data), so the index isn't needed to stitch them.
    pub fn read_value(&mut self, ns: u8, key: &str) -> Option<Vec<u8>> {
        let pages = self.pages.clone();
        for (p, _) in pages {
            let Some(buf) = self.read_page(p) else { continue };
            if let Some(v) = gather_value(&buf, ns, key) {
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
        None
    }
}

/// 32-byte entry slice at index `e`.
fn entry_slice(page: &[u8], e: usize) -> &[u8] {
    let at = HEADER_LEN + e * ENTRY_SIZE;
    &page[at..at + ENTRY_SIZE]
}

/// Two-bit entry state from the page bitmap @32.
fn entry_state(page: &[u8], e: usize) -> u8 {
    (page[32 + e / 4] >> ((e % 4) * 2)) & 0b11
}

/// Key match: the entry's 16-byte NUL-padded key vs `key`.
fn key_eq(ent: &[u8], key: &str) -> bool {
    let k = key.as_bytes();
    if k.len() > 16 {
        return false;
    }
    let field = &ent[8..24];
    field.starts_with(k) && field[k.len()..].iter().all(|&b| b == 0)
}

/// Iterator over the indices of written entries on a page, skipping the
/// data slots that belong to a written multi-slot (STR/BLOB) entry so a
/// trailing payload byte can't be misread as an entry header.
struct WrittenEntries<'p> {
    page: &'p [u8],
    e: usize,
}

impl<'p> WrittenEntries<'p> {
    fn new(page: &'p [u8]) -> Self {
        WrittenEntries { page, e: 0 }
    }
}

impl Iterator for WrittenEntries<'_> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        while self.e < ENTRY_COUNT {
            let e = self.e;
            let written = entry_state(self.page, e) == ST_WRITTEN;
            let span = entry_slice(self.page, e)[2] as usize;
            // Advance past this entry's payload slots when it's a valid
            // written multi-slot record; otherwise step one slot.
            self.e += if written && span > 0 && e + span <= ENTRY_COUNT {
                span
            } else {
                1
            };
            if written {
                return Some(e);
            }
        }
        None
    }
}

/// Collect every written STR/BLOB chunk for `(ns, key)` on one page and
/// concatenate them in chunk-index order. Each chunk's `size u16 @24`
/// bounds the payload held in its following `span-1` slots.
fn gather_value(page: &[u8], ns: u8, key: &str) -> Option<Vec<u8>> {
    let mut chunks: Vec<(u8, Vec<u8>)> = Vec::new();
    for e in WrittenEntries::new(page) {
        let ent = entry_slice(page, e);
        let typ = ent[1];
        if ent[0] != ns || !(typ == T_BLOB_DATA || typ == T_STR) || !key_eq(ent, key) {
            continue;
        }
        let span = ent[2] as usize;
        if span < 1 || e + span > ENTRY_COUNT {
            continue;
        }
        let size = u16::from_le_bytes([ent[24], ent[25]]) as usize;
        let data_start = HEADER_LEN + (e + 1) * ENTRY_SIZE;
        let data_end = HEADER_LEN + (e + span) * ENTRY_SIZE;
        let avail = data_end - data_start;
        let take = size.min(avail);
        if let Some(bytes) = page.get(data_start..data_start + take) {
            chunks.push((ent[3], bytes.to_vec()));
        }
    }
    if chunks.is_empty() {
        return None;
    }
    chunks.sort_by_key(|c| c.0);
    let mut out = Vec::new();
    for (_, mut b) in chunks {
        out.append(&mut b);
    }
    Some(out)
}

/// Decode an ESP-IDF WiFi SSID blob. PB stores it as `{ u32 len; u8
/// ssid[32]; }` (a 4-byte little-endian length prefix then the SSID
/// bytes). When the leading u32 isn't a plausible length (≤32 and within
/// the payload) the blob is treated as a raw NUL-terminated string
/// instead, which covers the older STR form. Returns None on non-UTF-8.
fn decode_ssid(payload: &[u8]) -> Option<String> {
    if payload.len() >= 4 {
        let n = u32::from_le_bytes(payload[0..4].try_into().unwrap()) as usize;
        if n >= 1 && n <= 32 && n <= payload.len() - 4 {
            return String::from_utf8(payload[4..4 + n].to_vec()).ok();
        }
    }
    decode_cstr(payload)
}

/// Decode a NUL-terminated C string from a blob payload (PB stores
/// `sta.pswd` as a 65-byte `u8 passwd[]`). Returns None on non-UTF-8; an
/// empty / all-NUL payload decodes to "".
fn decode_cstr(payload: &[u8]) -> Option<String> {
    let end = payload.iter().position(|&b| b == 0).unwrap_or(payload.len());
    String::from_utf8(payload[..end].to_vec()).ok()
}

/// Lift a stock Pixelblaze v3's WiFi station credentials out of its NVS
/// partition: SSID from `nvs.net80211`/`sta.ssid`, password from
/// `nvs.net80211`/`sta.pswd`. An absent/empty SSID (never provisioned) →
/// None; an empty password is legal (open network) and still returns
/// `Some(ssid, "")`. `region_len` is the NVS partition length (0x5000 on
/// a stock 4 MiB PB).
pub fn extract_wifi(read: ReadFn<'_>, region_len: u32) -> Option<(String, String)> {
    let mut nvs = PbNvs::open(read, region_len);
    let ns = nvs.namespace_index("nvs.net80211")?;
    let ssid = decode_ssid(&nvs.read_value(ns, "sta.ssid")?).filter(|s| !s.is_empty())?;
    let pass = nvs
        .read_value(ns, "sta.pswd")
        .and_then(|b| decode_cstr(&b))
        .unwrap_or_default();
    Some((ssid, pass))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- synthetic NVS page builder -------------------------------------

    struct Rec {
        ns: u8,
        typ: u8,
        chunk: u8,
        key: &'static str,
        /// data[8] field for primitives, or the variable payload for
        /// STR/BLOB (then `size` is set and the bytes spill into the
        /// following slots).
        data: Vec<u8>,
        is_var: bool,
        /// entry-state: true = written (0b10), false = erased (0b00).
        written: bool,
    }

    fn prim(ns: u8, typ: u8, key: &'static str, val: u8) -> Rec {
        Rec { ns, typ, chunk: 0xFF, key, data: alloc::vec![val], is_var: false, written: true }
    }

    fn blob(ns: u8, key: &'static str, payload: Vec<u8>, written: bool) -> Rec {
        Rec { ns, typ: T_BLOB_DATA, chunk: 0x80, key, data: payload, is_var: true, written }
    }

    /// Build a 4096-byte NVS page from a list of records.
    fn mk_page(state: u32, seqno: u32, recs: &[Rec]) -> Vec<u8> {
        let mut page = alloc::vec![0xFFu8; PAGE_SIZE as usize];
        page[0..4].copy_from_slice(&state.to_le_bytes());
        page[4..8].copy_from_slice(&seqno.to_le_bytes());
        page[8] = 0xFE; // v2
        // bitmap starts all-ones (0b11 = empty); clear bits per entry state.
        let mut slot = 0usize;
        let set_state = |page: &mut [u8], e: usize, st: u8| {
            let byte = 32 + e / 4;
            let shift = (e % 4) * 2;
            page[byte] = (page[byte] & !(0b11 << shift)) | (st << shift);
        };
        for r in recs {
            let span = if r.is_var {
                1 + (r.data.len() + ENTRY_SIZE - 1) / ENTRY_SIZE
            } else {
                1
            };
            let at = HEADER_LEN + slot * ENTRY_SIZE;
            page[at] = r.ns;
            page[at + 1] = r.typ;
            page[at + 2] = span as u8;
            page[at + 3] = r.chunk;
            // crc32 @4 left as-is (not verified).
            let kb = r.key.as_bytes();
            page[at + 8..at + 8 + kb.len()].copy_from_slice(kb);
            for b in &mut page[at + 8 + kb.len()..at + 24] {
                *b = 0;
            }
            if r.is_var {
                let size = r.data.len() as u16;
                page[at + 24..at + 26].copy_from_slice(&size.to_le_bytes());
                page[at + 26..at + 28].copy_from_slice(&0u16.to_le_bytes());
                page[at + 28..at + 32].copy_from_slice(&0u32.to_le_bytes());
                let ds = HEADER_LEN + (slot + 1) * ENTRY_SIZE;
                page[ds..ds + r.data.len()].copy_from_slice(&r.data);
            } else {
                for (i, &b) in r.data.iter().enumerate().take(8) {
                    page[at + 24 + i] = b;
                }
            }
            let st = if r.written { ST_WRITTEN } else { 0b00 };
            for s in slot..slot + span {
                set_state(&mut page, s, st);
            }
            slot += span;
        }
        page
    }

    /// `{ u32 len; u8 ssid[32]; }` blob, the PB `sta.ssid` layout.
    fn ssid_blob(ssid: &str) -> Vec<u8> {
        let mut v = alloc::vec![0u8; 36];
        v[0..4].copy_from_slice(&(ssid.len() as u32).to_le_bytes());
        v[4..4 + ssid.len()].copy_from_slice(ssid.as_bytes());
        v
    }

    /// 65-byte NUL-terminated password blob, the PB `sta.pswd` layout.
    fn pswd_blob(pass: &str) -> Vec<u8> {
        let mut v = alloc::vec![0u8; 65];
        v[..pass.len()].copy_from_slice(pass.as_bytes());
        v
    }

    /// Glue pages into one region and run a closure with a read callback.
    fn with_region<T>(pages: &[Vec<u8>], f: impl FnOnce(ReadFn, u32) -> T) -> T {
        let mut region = Vec::new();
        for p in pages {
            region.extend_from_slice(p);
        }
        let len = region.len() as u32;
        let mut read = |off: u32, buf: &mut [u8]| {
            let o = off as usize;
            match region.get(o..o + buf.len()) {
                Some(s) => {
                    buf.copy_from_slice(s);
                    true
                }
                None => false,
            }
        };
        f(&mut read, len)
    }

    /// Namespace reg on its own page, creds on another — the stock PB
    /// layout (reg on 0xB000, creds on 0xA000).
    #[test]
    fn extracts_creds_split_across_pages() {
        let ns_page = mk_page(0xFFFF_FFFC, 10, &[prim(0, T_U8, "nvs.net80211", 2)]);
        let data_page = mk_page(
            0xFFFF_FFFC,
            11,
            &[
                blob(2, "ap.ssid", ssid_blob("decoy-ap"), true), // same ns, wrong key
                blob(2, "sta.ssid", ssid_blob("MyNetwork"), true),
                blob(2, "sta.pswd", pswd_blob("hunter2!"), true),
            ],
        );
        with_region(&[ns_page, data_page], |read, len| {
            let (ssid, pass) = extract_wifi(read, len).unwrap();
            assert_eq!(ssid, "MyNetwork");
            assert_eq!(pass, "hunter2!");
        });
    }

    /// The newest page wins: a stale written copy on a lower-seqno page and
    /// an erased copy on a higher one must both lose to the live write.
    #[test]
    fn newest_written_entry_wins() {
        let ns_page = mk_page(0xFFFF_FFFC, 1, &[prim(0, T_U8, "nvs.net80211", 2)]);
        let stale = mk_page(0xFFFF_FFFC, 5, &[blob(2, "sta.ssid", ssid_blob("OldNet"), true)]);
        let newest = mk_page(
            0xFFFF_FFFE,
            9,
            &[
                blob(2, "sta.ssid", ssid_blob("OldNet"), false), // erased on newest
                blob(2, "sta.ssid", ssid_blob("NewNet"), true),
            ],
        );
        with_region(&[ns_page, stale, newest], |read, len| {
            let (ssid, _) = extract_wifi(read, len).unwrap();
            assert_eq!(ssid, "NewNet");
        });
    }

    /// preboot_guard clobbers the creds' sibling page: one page erased (all
    /// 0xFF) and one holding a foreign "LXBG" record. The creds live
    /// elsewhere and must still come back.
    #[test]
    fn survives_wiped_and_lxbg_pages() {
        let wiped = alloc::vec![0xFFu8; PAGE_SIZE as usize]; // erased page
        let mut lxbg = alloc::vec![0xFFu8; PAGE_SIZE as usize];
        lxbg[0..4].copy_from_slice(b"LXBG"); // non-NVS guard record
        lxbg[8] = 0x00; // version byte not 0xFE/0xFF → page rejected
        let ns_page = mk_page(0xFFFF_FFFC, 10, &[prim(0, T_U8, "nvs.net80211", 2)]);
        let data_page = mk_page(
            0xFFFF_FFFC,
            11,
            &[
                blob(2, "sta.ssid", ssid_blob("SurviveNet"), true),
                blob(2, "sta.pswd", pswd_blob("stillhere"), true),
            ],
        );
        with_region(&[wiped, ns_page, lxbg, data_page], |read, len| {
            let (ssid, pass) = extract_wifi(read, len).unwrap();
            assert_eq!(ssid, "SurviveNet");
            assert_eq!(pass, "stillhere");
        });
    }

    /// Open network: SSID present, password empty → Some with empty pass.
    #[test]
    fn empty_password_is_kept() {
        let ns_page = mk_page(0xFFFF_FFFC, 10, &[prim(0, T_U8, "nvs.net80211", 2)]);
        let data_page = mk_page(
            0xFFFF_FFFC,
            11,
            &[
                blob(2, "sta.ssid", ssid_blob("OpenNet"), true),
                blob(2, "sta.pswd", pswd_blob(""), true),
            ],
        );
        with_region(&[ns_page, data_page], |read, len| {
            let (ssid, pass) = extract_wifi(read, len).unwrap();
            assert_eq!(ssid, "OpenNet");
            assert_eq!(pass, "");
        });
    }

    /// No provisioning at all (no sta.ssid) → None.
    #[test]
    fn unprovisioned_is_none() {
        let ns_page = mk_page(0xFFFF_FFFC, 10, &[prim(0, T_U8, "nvs.net80211", 2)]);
        with_region(&[ns_page], |read, len| {
            assert!(extract_wifi(read, len).is_none());
        });
    }

    /// Older-IDF raw STR form of sta.ssid (no u32 length prefix) still
    /// decodes via the C-string fallback.
    #[test]
    fn str_form_ssid_fallback() {
        let mut v = alloc::vec![0u8; 20];
        v[..7].copy_from_slice(b"StrNet7");
        let rec = Rec { ns: 2, typ: T_STR, chunk: 0xFF, key: "sta.ssid", data: v, is_var: true, written: true };
        let ns_page = mk_page(0xFFFF_FFFC, 10, &[prim(0, T_U8, "nvs.net80211", 2)]);
        let data_page = mk_page(0xFFFF_FFFC, 11, &[rec]);
        with_region(&[ns_page, data_page], |read, len| {
            let (ssid, _) = extract_wifi(read, len).unwrap();
            assert_eq!(ssid, "StrNet7");
        });
    }

    /// A blob-index (0x48) entry carries no payload and must be ignored
    /// (its presence alongside the 0x42 chunk must not double-count).
    #[test]
    fn blob_index_entry_ignored() {
        let idx = Rec { ns: 2, typ: T_BLOB_IDX, chunk: 0xFF, key: "sta.ssid", data: ssid_blob("IdxNet"), is_var: true, written: true };
        let ns_page = mk_page(0xFFFF_FFFC, 10, &[prim(0, T_U8, "nvs.net80211", 2)]);
        let data_page = mk_page(
            0xFFFF_FFFC,
            11,
            &[blob(2, "sta.ssid", ssid_blob("IdxNet"), true), idx],
        );
        with_region(&[ns_page, data_page], |read, len| {
            let (ssid, _) = extract_wifi(read, len).unwrap();
            assert_eq!(ssid, "IdxNet");
        });
    }

    /// Junk bytes must not panic the page scan.
    #[test]
    fn junk_region_no_panic() {
        let junk = alloc::vec![0x5Au8; PAGE_SIZE as usize * 2];
        let len = junk.len() as u32;
        let mut read = |off: u32, buf: &mut [u8]| {
            let o = off as usize;
            match junk.get(o..o + buf.len()) {
                Some(s) => {
                    buf.copy_from_slice(s);
                    true
                }
                None => false,
            }
        };
        let _ = extract_wifi(&mut read, len);
    }
}
