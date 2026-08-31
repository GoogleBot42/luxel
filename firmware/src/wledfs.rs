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

// ---------------------------------------------------------------------------
// LED wiring + default-state import (cfg.json `hw.led`, `def`, `light.gc`).
// Same best-effort posture as the WiFi inheritance: every field is
// individually optional, any parse failure leaves it None, and the board
// defaults cover whatever is missing. The scanners below are scope-aware
// (balanced-bracket ranges) because the interesting keys — "len", "type",
// "order", "pin" — collide all over cfg.json (`hw.btn.ins[].type`,
// `ir.type`, `relay.pin`, …) where the WiFi lift's first-match-anywhere
// trick would grab the wrong one.

/// Find the value for `"key"` inside `range`, starting at `from`. Returns
/// (value_start, resume_pos); resume_pos continues the search when the value
/// isn't the shape the caller wanted (json_string_value's keep-looking
/// posture, bounded to the range).
fn next_value(
    json: &[u8],
    key: &str,
    range: core::ops::Range<usize>,
    from: usize,
) -> Option<(usize, usize)> {
    let pat_len = key.len() + 2;
    let end = range.end.min(json.len());
    let mut at = from.max(range.start);
    loop {
        if at + pat_len > end {
            return None;
        }
        let pos = json[at..end].windows(pat_len).position(|w| {
            w[0] == b'"' && w[pat_len - 1] == b'"' && &w[1..pat_len - 1] == key.as_bytes()
        })? + at;
        let mut i = pos + pat_len;
        while i < end && matches!(json[i], b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
        }
        if i < end && json[i] == b':' {
            i += 1;
            while i < end && matches!(json[i], b' ' | b'\t' | b'\n' | b'\r') {
                i += 1;
            }
            if i < end {
                return Some((i, pos + pat_len));
            }
            return None;
        }
        at = pos + pat_len;
    }
}

/// Given `json[start]` == `open`, the range INSIDE the matching close
/// bracket. Tracks strings/escapes; a torn container yields None. Counting
/// one bracket kind is sound — JSON nests properly, so `{}` and `[]` each
/// balance independently.
fn balanced_inner(
    json: &[u8],
    start: usize,
    end: usize,
    open: u8,
    close: u8,
) -> Option<core::ops::Range<usize>> {
    let mut depth = 0usize;
    let mut in_str = false;
    let mut esc = false;
    for (i, &c) in json.iter().enumerate().take(end.min(json.len())).skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == open {
            depth += 1;
        } else if c == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(start + 1..i);
            }
        }
    }
    None
}

/// Range inside the brackets of `key`'s object (`{}`) or array (`[]`)
/// value, scoped to `range`.
fn container_inner(
    json: &[u8],
    key: &str,
    range: core::ops::Range<usize>,
    open: u8,
    close: u8,
) -> Option<core::ops::Range<usize>> {
    let mut from = range.start;
    loop {
        let (v, resume) = next_value(json, key, range.clone(), from)?;
        if json[v] == open {
            return balanced_inner(json, v, range.end, open, close);
        }
        from = resume;
    }
}

/// Range inside the first `{…}` element of `key`'s array value.
fn first_object_in_array(
    json: &[u8],
    key: &str,
    range: core::ops::Range<usize>,
) -> Option<core::ops::Range<usize>> {
    let arr = container_inner(json, key, range, b'[', b']')?;
    let mut i = arr.start;
    while i < arr.end && matches!(json[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    (i < arr.end && json[i] == b'{')
        .then(|| balanced_inner(json, i, arr.end, b'{', b'}'))
        .flatten()
}

/// Parse a number at `i` in TENTHS (×10, keeping exactly one decimal digit
/// — WLED's gamma is `"col":2.8`; further digits truncate).
fn parse_tenths(json: &[u8], mut i: usize, end: usize) -> Option<i64> {
    let neg = json.get(i) == Some(&b'-');
    if neg {
        i += 1;
    }
    let digits_start = i;
    let mut int: i64 = 0;
    while i < end && json.get(i).is_some_and(u8::is_ascii_digit) {
        int = int.saturating_mul(10).saturating_add((json[i] - b'0') as i64);
        i += 1;
    }
    if i == digits_start {
        return None;
    }
    let mut tenths = int.saturating_mul(10);
    if i < end && json.get(i) == Some(&b'.') {
        if let Some(d) = json.get(i + 1).filter(|d| d.is_ascii_digit()) {
            tenths = tenths.saturating_add((d - b'0') as i64);
        }
    }
    Some(if neg { -tenths } else { tenths })
}

/// Numeric value for `key` within `range`, in tenths (see [parse_tenths]).
fn json_tenths(json: &[u8], key: &str, range: core::ops::Range<usize>) -> Option<i64> {
    let mut from = range.start;
    loop {
        let (v, resume) = next_value(json, key, range.clone(), from)?;
        if let Some(t) = parse_tenths(json, v, range.end) {
            return Some(t);
        }
        from = resume;
    }
}

/// Numeric value (truncated to integer) for `key` within `range`.
fn json_int(json: &[u8], key: &str, range: core::ops::Range<usize>) -> Option<i64> {
    json_tenths(json, key, range).map(|t| t / 10)
}

/// First number of `key`'s array value (WLED pins are `"pin":[18]`).
fn json_array0_int(json: &[u8], key: &str, range: core::ops::Range<usize>) -> Option<i64> {
    let arr = container_inner(json, key, range, b'[', b']')?;
    let mut i = arr.start;
    while i < arr.end && matches!(json[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    parse_tenths(json, i, arr.end).map(|t| t / 10)
}

/// LED wiring + defaults lifted from WLED's cfg.json. Field-by-field
/// optional; consumers map WLED's codes with [map_strip_type] /
/// [map_color_order] and fall back per field.
#[derive(Clone, Copy, Default)]
pub struct WledWiring {
    /// `hw.led.ins[0].len`, falling back to `hw.led.total`. Always > 0.
    pub pixels: Option<u32>,
    /// `hw.led.ins[0].pin[0]` — informational only (Luxel pins are
    /// compile-time per board); logged so a mismatch is diagnosable.
    pub pin: Option<i32>,
    /// `hw.led.ins[0].type` — WLED bus type code (TYPE_*).
    pub strip_type: Option<u8>,
    /// `hw.led.ins[0].order` — WLED color order code (COL_ORDER_*).
    pub order: Option<u8>,
    /// `def.bri` — WLED's boot brightness, 0–255.
    pub bri: Option<u8>,
    /// `hw.led.maxpwr` in mA, only when the user enabled the limiter
    /// (WLED 0 = off), clamped to Luxel's 20 A ceiling.
    pub cap_ma: Option<u16>,
    /// `light.gc.col` × 10, only when it's a real gamma (WLED 1.0 = off;
    /// anything outside 1.1–5.0 is treated as off/garbage).
    pub gamma_tenths: Option<u8>,
}

/// Parse the wiring fields out of a cfg.json byte buffer. Pure — the
/// host-side rig (tools/wledfs-check) tests this against real dumps.
pub fn parse_wiring(cfg: &[u8]) -> WledWiring {
    let all = 0..cfg.len();
    let mut w = WledWiring::default();
    if let Some(led) = container_inner(cfg, "hw", all.clone(), b'{', b'}')
        .and_then(|hw| container_inner(cfg, "led", hw, b'{', b'}'))
    {
        w.cap_ma = json_int(cfg, "maxpwr", led.clone())
            .filter(|&v| v > 0)
            .map(|v| v.min(20_000) as u16);
        if let Some(ins0) = first_object_in_array(cfg, "ins", led.clone()) {
            w.pixels = json_int(cfg, "len", ins0.clone())
                .filter(|&v| v > 0)
                .map(|v| v.min(u32::MAX as i64) as u32);
            w.pin = json_array0_int(cfg, "pin", ins0.clone())
                .and_then(|v| i32::try_from(v).ok());
            w.strip_type = json_int(cfg, "type", ins0.clone())
                .and_then(|v| u8::try_from(v).ok());
            w.order = json_int(cfg, "order", ins0).and_then(|v| u8::try_from(v).ok());
        }
        if w.pixels.is_none() {
            w.pixels = json_int(cfg, "total", led)
                .filter(|&v| v > 0)
                .map(|v| v.min(u32::MAX as i64) as u32);
        }
    }
    if let Some(def) = container_inner(cfg, "def", all.clone(), b'{', b'}') {
        w.bri = json_int(cfg, "bri", def).and_then(|v| u8::try_from(v).ok());
    }
    if let Some(gc) = container_inner(cfg, "light", all, b'{', b'}')
        .and_then(|l| container_inner(cfg, "gc", l, b'{', b'}'))
    {
        w.gamma_tenths = json_tenths(cfg, "col", gc)
            .filter(|&v| (11..=50).contains(&v))
            .map(|v| v as u8);
    }
    w
}

/// Mount + read cfg.json + [parse_wiring]. None only when the filesystem
/// or file is unreadable — an empty/unconfigured cfg still returns a
/// (mostly-None) wiring.
pub fn extract_wiring(read: ReadFn<'_>, region_len: u32) -> Option<WledWiring> {
    let mut fs = WledFs::open(read, region_len)?;
    let cfg = fs.read_file("cfg.json")?;
    Some(parse_wiring(&cfg))
}

/// WLED bus type (const.h TYPE_*) → Luxel protocol code
/// (`leds::Protocol::as_u8`: 0 = sk9822, 1 = ws2812). Deliberately
/// conservative: only chips Luxel's two encoders genuinely drive. RGBW
/// (SK6812/TM1814), 400 kHz WS2811, WS2801 (SPI but not APA-framed),
/// analog/PWM, matrix and virtual buses have no Luxel equivalent → None
/// (board default + a log line).
pub fn map_strip_type(t: u8) -> Option<u8> {
    match t {
        22 => Some(1), // TYPE_WS2812_RGB — the WS2812/WS2815/WS281x RGB family
        51 => Some(0), // TYPE_APA102 — SK9822-compatible framing
        _ => None,
    }
}

/// WLED color order (const.h COL_ORDER_*: 0 GRB, 1 RGB, 2 BRG, 3 RBG,
/// 4 BGR, 5 GBR — the strip's WIRE order) → Luxel outpipe ColorOrder code.
/// Luxel's ColorOrder is a PRE-encoder remap with identity 0 ("rgb"), and
/// the encoders already emit each chip's native wire order (ws2812 GRB,
/// sk9822 BGR) — so the mapping is relative to that native order: the perm
/// P solving native∘P = wled_order. E.g. WLED GRB on a ws2812 is the
/// native order → identity, NOT Luxel's "grb". Verified by construction in
/// the wire_order_roundtrip test.
pub fn map_color_order(wled_order: u8, luxel_protocol: u8) -> Option<u8> {
    let table: &[u8; 6] = match luxel_protocol {
        1 => &[0, 2, 1, 4, 3, 5], // ws2812 (native GRB)
        0 => &[4, 5, 2, 3, 0, 1], // sk9822 (native BGR)
        _ => return None,
    };
    table.get(wled_order as usize).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic cfg.json with the exact structure of a real Athom
    /// WLED-SoundReactive dump (secrets replaced), including the decoy
    /// "type"/"pin"/"ins" keys in hw.btn / ir / relay / digitalmic that a
    /// scope-blind first-match scan would trip over.
    const CFG: &[u8] = br#"{"rev":[1,0],"vid":22102011,"id":{"mdns":"wled-000000","name":"WLED-SoundReactive","inv":"Light"},"nw":{"ins":[{"ssid":"test-net","pskl":8,"ip":[0,0,0,0],"gw":[0,0,0,0],"sn":[255,255,255,0]}]},"ap":{"ssid":"WLED_000000","pskl":0,"chan":1,"hide":0,"behav":0,"ip":[4,3,2,1]},"wifi":{"sleep":false},"hw":{"led":{"total":30,"maxpwr":850,"ledma":55,"cct":false,"fps":42,"rgbwm":3,"ins":[{"start":0,"len":30,"pin":[18],"order":0,"rev":false,"skip":0,"type":22,"ref":false}]},"com":[],"btn":{"max":4,"ins":[{"type":2,"pin":[0],"macros":[0,0,0]},{"type":0,"pin":[-1],"macros":[0,0,0]}],"tt":32,"mqtt":false},"ir":{"pin":25,"type":9,"sel":true},"relay":{"pin":2,"rev":false},"baud":1152,"analogmic":{"pin":36},"digitalmic":{"en":5,"pins":{"i2ssd":32,"i2sws":15,"i2sck":-1}}},"light":{"scale-bri":100,"pal-mode":0,"aseg":false,"gc":{"bri":1,"col":2.8},"tr":{"mode":true,"dur":7,"pal":0}},"def":{"ps":0,"on":true,"bri":128},"if":{"live":{"en":true,"port":5568,"timeout":25,"offset":0}},"um":{}}"#;

    #[test]
    fn parses_the_athom_shape() {
        let w = parse_wiring(CFG);
        assert_eq!(w.pixels, Some(30));
        assert_eq!(w.pin, Some(18));
        assert_eq!(w.strip_type, Some(22));
        assert_eq!(w.order, Some(0));
        assert_eq!(w.bri, Some(128));
        assert_eq!(w.cap_ma, Some(850));
        assert_eq!(w.gamma_tenths, Some(28));
    }

    #[test]
    fn empty_ins_falls_back_to_total_and_zero_total_is_none() {
        let cfg = br#"{"hw":{"led":{"total":45,"maxpwr":0,"ins":[]},"btn":{"ins":[{"type":2,"pin":[0]}]}}}"#;
        let w = parse_wiring(cfg);
        assert_eq!(w.pixels, Some(45));
        assert_eq!(w.strip_type, None); // btn decoy must not leak in
        assert_eq!(w.pin, None);
        assert_eq!(w.cap_ma, None); // maxpwr 0 = limiter off
        let stale = br#"{"hw":{"led":{"total":0,"ins":[]}}}"#;
        assert_eq!(parse_wiring(stale).pixels, None);
    }

    #[test]
    fn missing_blocks_yield_all_none() {
        let w = parse_wiring(br#"{"nw":{"ins":[{"ssid":"x"}]}}"#);
        assert!(w.pixels.is_none() && w.strip_type.is_none() && w.bri.is_none());
        // junk bytes must not panic either
        let _ = parse_wiring(&[0xFF, 0x22, 0x7B, 0x00]);
    }

    #[test]
    fn gamma_off_and_garbage_filtered() {
        let g = |col: &str| {
            let mut cfg = alloc::string::String::new();
            cfg.push_str(r#"{"light":{"gc":{"bri":1,"col":"#);
            cfg.push_str(col);
            cfg.push_str("}}}");
            parse_wiring(cfg.as_bytes()).gamma_tenths
        };
        assert_eq!(g("2.8"), Some(28));
        assert_eq!(g("2"), Some(20));
        assert_eq!(g("1.0"), None); // WLED's "off"
        assert_eq!(g("1"), None);
        assert_eq!(g("9.9"), None); // implausible
        assert_eq!(g("-2.8"), None);
    }

    #[test]
    fn negative_and_decimal_numbers() {
        let cfg = br#"{"hw":{"led":{"ins":[{"len":30,"pin":[-1],"order":3,"type":22}]}}}"#;
        let w = parse_wiring(cfg);
        assert_eq!(w.pin, Some(-1));
        assert_eq!(w.order, Some(3));
    }

    #[test]
    fn strip_type_map_is_conservative() {
        assert_eq!(map_strip_type(22), Some(1)); // WS2812 RGB
        assert_eq!(map_strip_type(51), Some(0)); // APA102
        for t in [18, 21, 24, 30, 31, 40, 41, 50, 52, 65, 80] {
            assert_eq!(map_strip_type(t), None, "type {t} must not map");
        }
    }

    /// Derivation check for map_color_order: for every WLED order and both
    /// protocols, WLED's wire bytes (logical RGB reordered by COL_ORDER_*)
    /// must equal Luxel's (ColorOrder perm applied first — outpipe.rs
    /// semantics out[i] = in[perm[i]] — then the encoder's native order).
    #[test]
    fn wire_order_roundtrip() {
        // WLED COL_ORDER_*: wire position → logical channel (R=0,G=1,B=2)
        const WLED: [[usize; 3]; 6] = [
            [1, 0, 2], // GRB
            [0, 1, 2], // RGB
            [2, 0, 1], // BRG
            [0, 2, 1], // RBG
            [2, 1, 0], // BGR
            [1, 2, 0], // GBR
        ];
        // luxel-core outpipe.rs PERMS, same codes
        const LUXEL: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let c = [10u8, 20, 30]; // (R,G,B)
        for (proto, native) in [(1u8, [1usize, 0, 2]), (0u8, [2usize, 1, 0])] {
            for wled_order in 0..6u8 {
                let wled_wire: [u8; 3] =
                    core::array::from_fn(|i| c[WLED[wled_order as usize][i]]);
                let luxel_code = map_color_order(wled_order, proto).unwrap() as usize;
                let permuted: [u8; 3] = core::array::from_fn(|i| c[LUXEL[luxel_code][i]]);
                let luxel_wire: [u8; 3] = core::array::from_fn(|i| permuted[native[i]]);
                assert_eq!(
                    luxel_wire, wled_wire,
                    "wled order {wled_order} on protocol {proto}: luxel code {luxel_code}"
                );
            }
        }
        assert_eq!(map_color_order(6, 1), None);
        assert_eq!(map_color_order(0, 2), None);
    }
}
