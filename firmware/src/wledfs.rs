//! Minimal read-only littlefs v2 reader — just enough to lift WLED's
//! config off the filesystem it leaves behind during a takeover
//! (`cfg.json` holds the WiFi SSID + board wiring, `wsec.json` the WiFi
//! password; validated against real Athom dumps, see UPDATES.md
//! 2026-07-26).
//!
//! Deliberately best-effort: commits are NOT CRC-verified (a torn commit
//! can only yield a missing file or garbage JSON, and every failure path
//! lands in the provisioning-AP fallback), directories are not descended
//! (WLED keeps everything in root), and unknown tags are skipped. The
//! whole parser reads through a caller-supplied region-relative read
//! callback and has no esp-hal dependencies, so the exact same file is
//! compiled and tested on the host against dump images
//! (tools/wledfs-check).
//!
//! Format notes (littlefs SPEC.md): a directory is a pair of alternating
//! metadata blocks (higher revision wins); each block is a log of 32-bit
//! big-endian tags XOR-chained from 0xFFFFFFFF, each optionally followed
//! by inline data. Files are either inline (data lives in the metadata
//! block) or CTZ skip-lists (head block + size; block i>0 starts with
//! ctz(i)+1 backward pointers).

use alloc::string::String;
use alloc::vec::Vec;

/// Region-relative flash read. Returns false on failure.
pub type ReadFn<'a> = &'a mut dyn FnMut(u32, &mut [u8]) -> bool;

const TAG_ERASED: u32 = 0xFFFF_FFFF;
// 11-bit tag types (lfs.h)
const T_REG: u32 = 0x001;
const T_SUPERBLOCK: u32 = 0x0FF;
const T_INLINE: u32 = 0x201;
const T_CTZ: u32 = 0x202;
const T_CREATE: u32 = 0x401;
const T_DELETE: u32 = 0x4FF;
const T_HARDTAIL: u32 = 0x601;

#[derive(Clone, Copy, Default)]
enum FileData {
    #[default]
    None,
    /// data lives in a metadata block: (block, offset, len)
    Inline(u32, u32, u32),
    /// CTZ skip-list: (head block, file size)
    Ctz(u32, u32),
}

#[derive(Clone, Default)]
struct Entry {
    name: Vec<u8>,
    is_file: bool,
    data: FileData,
}

struct ParsedBlock {
    rev: u32,
    entries: Vec<Entry>,
    hardtail: Option<(u32, u32)>,
    /// saw at least one decodable tag — distinguishes "empty log" from junk
    any_tags: bool,
}

pub struct WledFs<'a> {
    read: ReadFn<'a>,
    block_size: u32,
    block_count: u32,
}

impl<'a> WledFs<'a> {
    /// Mount check: block 0 or 1 of the region must carry the littlefs
    /// superblock magic. `block_size` is taken as 4096 (the only geometry
    /// ESP32 WLED uses; the superblock's own copy is cross-checked).
    pub fn open(read: ReadFn<'a>, region_len: u32) -> Option<WledFs<'a>> {
        let mut fs = WledFs { read, block_size: 4096, block_count: region_len / 4096 };
        let mut head = [0u8; 64];
        for b in 0..2u32 {
            if !(fs.read)(b * fs.block_size, &mut head) {
                return None;
            }
            if let Some(at) = head.windows(8).position(|w| w == b"littlefs") {
                // geometry struct follows the magic: version, block_size,
                // block_count (all u32 LE, behind the 4-byte inline tag)
                let g = at + 8 + 4;
                if g + 12 <= head.len() {
                    let bs = u32::from_le_bytes(head[g + 4..g + 8].try_into().unwrap());
                    let bc = u32::from_le_bytes(head[g + 8..g + 12].try_into().unwrap());
                    if bs.is_power_of_two() && bs >= 128 && bs <= 32768 {
                        fs.block_size = bs;
                        fs.block_count = bc.min(region_len / bs);
                    }
                }
                return Some(fs);
            }
        }
        None
    }

    fn read_block(&mut self, block: u32) -> Option<Vec<u8>> {
        if block >= self.block_count {
            return None;
        }
        let mut buf = alloc::vec![0u8; self.block_size as usize];
        (self.read)(block * self.block_size, &mut buf).then_some(buf)
    }

    /// Walk one metadata block's tag log, replaying creates/deletes so
    /// entry ids resolve the way littlefs meant them.
    fn parse_block(&mut self, block: u32) -> Option<ParsedBlock> {
        let buf = self.read_block(block)?;
        let mut out = ParsedBlock {
            rev: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            entries: Vec::new(),
            hardtail: None,
            any_tags: false,
        };
        let mut pos = 4usize;
        let mut ptag = TAG_ERASED;
        loop {
            if pos + 4 > buf.len() {
                break;
            }
            let raw = u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap());
            if raw == TAG_ERASED {
                break; // end of log (erased flash)
            }
            let tag = raw ^ ptag;
            ptag = tag;
            pos += 4;
            let ttype = (tag >> 20) & 0x7FF;
            let id = ((tag >> 10) & 0x3FF) as usize;
            let size = (tag & 0x3FF) as usize;
            let deleted = size == 0x3FF; // "size -1": tag carries no data
            let data_at = pos;
            if !deleted {
                pos += size;
                if pos > buf.len() {
                    break; // torn write — stop at the last sane point
                }
            }
            out.any_tags = true;
            let ensure = |v: &mut Vec<Entry>, id: usize| {
                while v.len() <= id {
                    v.push(Entry::default());
                }
            };
            match ttype {
                T_CREATE => {
                    let at = id.min(out.entries.len());
                    out.entries.insert(at, Entry::default());
                }
                T_DELETE => {
                    if id < out.entries.len() {
                        out.entries.remove(id);
                    }
                }
                T_REG | T_SUPERBLOCK => {
                    ensure(&mut out.entries, id);
                    if !deleted {
                        out.entries[id].name = buf[data_at..data_at + size].to_vec();
                        out.entries[id].is_file = ttype == T_REG;
                    }
                }
                T_INLINE => {
                    ensure(&mut out.entries, id);
                    out.entries[id].data = if deleted {
                        FileData::None
                    } else {
                        FileData::Inline(block, data_at as u32, size as u32)
                    };
                }
                T_CTZ => {
                    ensure(&mut out.entries, id);
                    if !deleted && size >= 8 {
                        let head =
                            u32::from_le_bytes(buf[data_at..data_at + 4].try_into().unwrap());
                        let fsize =
                            u32::from_le_bytes(buf[data_at + 4..data_at + 8].try_into().unwrap());
                        out.entries[id].data = FileData::Ctz(head, fsize);
                    }
                }
                T_HARDTAIL => {
                    if !deleted && size >= 8 {
                        out.hardtail = Some((
                            u32::from_le_bytes(buf[data_at..data_at + 4].try_into().unwrap()),
                            u32::from_le_bytes(buf[data_at + 4..data_at + 8].try_into().unwrap()),
                        ));
                    }
                }
                _ => {} // CRC commits, gstate, attrs, dir structs: skipped
            }
        }
        Some(out)
    }

    /// Parse a metadata pair: both blocks, prefer the newer revision, fall
    /// back to the sibling when the newer one yields nothing.
    fn parse_pair(&mut self, pair: (u32, u32)) -> Option<ParsedBlock> {
        let a = self.parse_block(pair.0);
        let b = self.parse_block(pair.1);
        match (a, b) {
            (Some(a), Some(b)) => {
                let a_newer = a.rev.wrapping_sub(b.rev) as i32 > 0;
                let (first, second) = if a_newer { (a, b) } else { (b, a) };
                if first.any_tags {
                    Some(first)
                } else {
                    Some(second)
                }
            }
            (x, y) => x.or(y),
        }
    }

    fn file_bytes(&mut self, data: FileData) -> Option<Vec<u8>> {
        match data {
            FileData::None => None,
            FileData::Inline(block, off, len) => {
                let buf = self.read_block(block)?;
                buf.get(off as usize..(off + len) as usize).map(<[u8]>::to_vec)
            }
            FileData::Ctz(head, size) => self.read_ctz(head, size),
        }
    }

    /// Read a CTZ skip-list file: figure out how many blocks it spans,
    /// walk the backward pointers from `head` (the LAST block) to index
    /// them, then assemble in order.
    fn read_ctz(&mut self, head: u32, size: u32) -> Option<Vec<u8>> {
        let bs = self.block_size;
        let ptrs = |i: u32| if i == 0 { 0 } else { i.trailing_zeros() + 1 };
        let cap = |i: u32| bs - 4 * ptrs(i);
        // block count for `size` bytes
        let mut n = 0u32;
        let mut total = 0u64;
        while total < size as u64 {
            total += cap(n) as u64;
            n += 1;
            if n > self.block_count {
                return None;
            }
        }
        n = n.max(1);
        // walk head (index n-1) back to index 0 via pointer[0]
        let mut blocks = alloc::vec![0u32; n as usize];
        let mut cur = head;
        for i in (0..n).rev() {
            blocks[i as usize] = cur;
            if i > 0 {
                let mut p = [0u8; 4];
                if !(self.read)(cur.checked_mul(bs)? , &mut p) {
                    return None;
                }
                cur = u32::from_le_bytes(p);
                if cur >= self.block_count {
                    return None;
                }
            }
        }
        let mut out = Vec::with_capacity(size as usize);
        for (i, &b) in blocks.iter().enumerate() {
            let skip = 4 * ptrs(i as u32);
            let take = (cap(i as u32) as usize).min(size as usize - out.len());
            let buf = self.read_block(b)?;
            out.extend_from_slice(buf.get(skip as usize..skip as usize + take)?);
            if out.len() >= size as usize {
                break;
            }
        }
        (out.len() == size as usize).then_some(out)
    }

    /// Look `name` up in the root directory (following same-directory
    /// hardtail continuations) and return its content.
    pub fn read_file(&mut self, name: &str) -> Option<Vec<u8>> {
        let mut pair = (0u32, 1u32);
        for _ in 0..8 {
            let parsed = self.parse_pair(pair)?;
            for e in &parsed.entries {
                if e.is_file && e.name == name.as_bytes() {
                    return self.file_bytes(e.data);
                }
            }
            pair = parsed.hardtail?;
        }
        None
    }
}

/// First `"key":"value"` string in `json`, unescaped. Handles the escapes
/// WLED's ArduinoJson actually emits (\" \\ \/ plus control shorthands);
/// anything fancier aborts — the caller treats that as "no value" and the
/// provisioning AP takes over.
pub fn json_string_value(json: &[u8], key: &str) -> Option<String> {
    let pat_len = key.len() + 2;
    let mut at = 0usize;
    loop {
        let start = json[at..]
            .windows(pat_len)
            .position(|w| w[0] == b'"' && w[pat_len - 1] == b'"' && &w[1..pat_len - 1] == key.as_bytes())?
            + at;
        let mut i = start + pat_len;
        while i < json.len() && (json[i] == b' ' || json[i] == b'\t') {
            i += 1;
        }
        if i < json.len() && json[i] == b':' {
            i += 1;
            while i < json.len() && (json[i] == b' ' || json[i] == b'\t') {
                i += 1;
            }
            if i < json.len() && json[i] == b'"' {
                i += 1;
                let mut out = Vec::new();
                while i < json.len() {
                    match json[i] {
                        b'"' => return String::from_utf8(out).ok(),
                        b'\\' => {
                            i += 1;
                            let e = *json.get(i)?;
                            match e {
                                b'"' | b'\\' | b'/' => out.push(e),
                                b'n' => out.push(b'\n'),
                                b't' => out.push(b'\t'),
                                b'r' => out.push(b'\r'),
                                _ => return None, // \uXXXX etc: bail safely
                            }
                        }
                        c => out.push(c),
                    }
                    i += 1;
                }
                return None;
            }
        }
        at = start + pat_len; // key matched something non-string; keep looking
    }
}

/// Lift WLED's WiFi credentials off its filesystem: SSID from cfg.json
/// (`nw.ins[0].ssid` — first "ssid" key in the file), password from
/// wsec.json (first "psk"). Empty SSID (factory-fresh WLED) → None.
pub fn extract_wifi(read: ReadFn<'_>, region_len: u32) -> Option<(String, String)> {
    let mut fs = WledFs::open(read, region_len)?;
    let cfg = fs.read_file("cfg.json")?;
    let ssid = json_string_value(&cfg, "ssid").filter(|s| !s.is_empty())?;
    let pass = fs
        .read_file("wsec.json")
        .and_then(|w| json_string_value(&w, "psk"))
        .unwrap_or_default();
    Some((ssid, pass))
}
