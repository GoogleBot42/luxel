//! Minimal read-only SPIFFS reader — just enough to lift a stock
//! Pixelblaze v3's `/config.json` off the data partition it leaves behind
//! during a takeover (pixel count, LED type, colour order, brightness).
//! Sibling of `wledfs.rs` (WLED's littlefs) and built the same way: a pure
//! module with no esp-hal dependencies that reads through a caller-supplied
//! region-relative callback, so the exact same file is compiled and tested
//! on the host against dump images (tools/pbfs-check).
//!
//! Format notes (spiffs_nucleus.h, ESP-IDF geometry: 4 KiB blocks, 256-byte
//! pages, 32-byte names, 4-byte meta — all verified against a real v3
//! dump): every page starts with a 5-byte header `obj_id u16 | span_ix u16
//! | flags u8`. Flag bits are CLEARED when the property holds (NOR flash: a
//! write can only clear bits) — used, final, index, deleted. Object ids
//! with bit 15 set are index pages; the span-0 index page is the object's
//! header (size, type, name, meta, then the page numbers of the first 103
//! data pages) and later spans hold 124 page numbers each. A data page is
//! its header plus 251 payload bytes, so a file's bytes are NEVER
//! contiguous. The first page of every block is the object lookup (a cache
//! of the headers) and is skipped — the headers are the truth.
//!
//! Deleting only clears one flag bit, and Pixelblaze rewrites config.json
//! (bumping `rev`) on every settings change, so a used device carries
//! dozens of stale copies until garbage collection — the dump this was
//! built against had 36 of them, several with a higher `rev` than one grep
//! might stop at. Only the index walk tells the live copy apart, which is
//! why this is a real reader and not a scan; the scan survives only as a
//! last resort for a partition whose live header was torn (see
//! [PbFs::stale_config_head]).
//!
//! Best-effort like its sibling: SPIFFS has no CRCs to check, directories
//! don't exist, and every failure path yields None → board defaults.

use alloc::vec::Vec;

/// Region-relative flash read. Returns false on failure.
pub type ReadFn<'a> = &'a mut dyn FnMut(u32, &mut [u8]) -> bool;

const PAGE: u32 = 256;
const BLOCK: u32 = 4096;
const PAGES_PER_BLOCK: u32 = BLOCK / PAGE;
const LOOKUP_PAGES: u32 = 1;
const NAME_LEN: usize = 32;
const META_LEN: usize = 4;
/// Object-index HEADER page: page header (5) + pad to 4 (3) + size u32 +
/// type u8 + name + meta, then u16 data-page numbers.
const IX_HDR_LEN: usize = 5 + 3 + 4 + 1 + NAME_LEN + META_LEN; // 49
/// Object-index continuation page: page header (5) + pad to 4 (3).
const IX_LEN: usize = 8;
const HDR_IX_ENTRIES: u32 = ((PAGE as usize - IX_HDR_LEN) / 2) as u32; // 103
const IX_ENTRIES: u32 = ((PAGE as usize - IX_LEN) / 2) as u32; // 124
const PAGE_HDR_LEN: usize = 5;
const DATA_PER_PAGE: u32 = PAGE - PAGE_HDR_LEN as u32; // 251

const OBJ_ID_FREE: u16 = 0xFFFF;
const OBJ_ID_DELETED: u16 = 0x0000;
const OBJ_ID_IX: u16 = 0x8000;
const TYPE_FILE: u8 = 1;
const UNDEFINED_LEN: u32 = 0xFFFF_FFFF;
// Page flags — a CLEARED bit means the property holds.
const F_USED: u8 = 1 << 0;
const F_FINAL: u8 = 1 << 1;
const F_INDEX: u8 = 1 << 2;
const F_DELETED: u8 = 1 << 7;

#[derive(Clone, Copy)]
struct PageHdr {
    obj_id: u16,
    span: u16,
    flags: u8,
}

impl PageHdr {
    fn parse(page: &[u8]) -> PageHdr {
        PageHdr {
            obj_id: u16::from_le_bytes([page[0], page[1]]),
            span: u16::from_le_bytes([page[2], page[3]]),
            flags: page[4],
        }
    }

    /// Fully written and not deleted.
    fn live(&self) -> bool {
        self.obj_id != OBJ_ID_FREE
            && self.obj_id != OBJ_ID_DELETED
            && self.flags & (F_USED | F_FINAL) == 0
            && self.flags & F_DELETED != 0
    }

    fn is_index(&self) -> bool {
        self.flags & F_INDEX == 0 && self.obj_id & OBJ_ID_IX != 0
    }
}

pub struct PbFs<'a> {
    read: ReadFn<'a>,
    pages: u32,
}

impl<'a> PbFs<'a> {
    /// SPIFFS carries no magic (ESP-IDF builds it without one), so "mount"
    /// is a geometry check: at least one block, whole pages. Whether the
    /// region is actually SPIFFS shows up as "no file found".
    pub fn open(read: ReadFn<'a>, region_len: u32) -> Option<PbFs<'a>> {
        (region_len >= BLOCK).then_some(PbFs { read, pages: region_len / PAGE })
    }

    fn read_page(&mut self, page: u32, buf: &mut [u8; PAGE as usize]) -> bool {
        page < self.pages && (self.read)(page * PAGE, buf)
    }

    /// Walk every page header in the region, a block at a time (one 4 KiB
    /// read per block, never a per-page read), handing `f` each non-lookup
    /// page's number and bytes. `f` returns false to stop early.
    fn for_each_page(&mut self, mut f: impl FnMut(u32, &[u8]) -> bool) {
        let mut buf = alloc::vec![0u8; BLOCK as usize];
        for b in 0..self.pages / PAGES_PER_BLOCK {
            if !(self.read)(b * BLOCK, &mut buf) {
                return;
            }
            for p in LOOKUP_PAGES..PAGES_PER_BLOCK {
                let pg = &buf[(p * PAGE) as usize..((p + 1) * PAGE) as usize];
                if !f(b * PAGES_PER_BLOCK + p, pg) {
                    return;
                }
            }
        }
    }

    /// Every live object-index header page carrying `name` — normally one;
    /// a rewrite torn between "new header written" and "old header
    /// deleted" leaves two. Returns (header page, obj_id, size).
    fn find_headers(&mut self, name: &[u8]) -> Vec<(u32, u16, u32)> {
        let mut out = Vec::new();
        self.for_each_page(|page, pg| {
            let h = PageHdr::parse(pg);
            if h.live() && h.is_index() && h.span == 0 && pg[12] == TYPE_FILE {
                let n = &pg[13..13 + NAME_LEN];
                let n = &n[..n.iter().position(|&c| c == 0).unwrap_or(NAME_LEN)];
                if n == name {
                    let size = u32::from_le_bytes(pg[8..12].try_into().unwrap());
                    out.push((page, h.obj_id & !OBJ_ID_IX, size));
                }
            }
            true
        });
        out
    }

    /// Page number of object-index span `span` (>= 1) of `obj_id` — only
    /// files past 103 data pages (~25 KB) have any.
    fn find_index_page(&mut self, obj_id: u16, span: u16) -> Option<u32> {
        let mut found = None;
        self.for_each_page(|page, pg| {
            let h = PageHdr::parse(pg);
            if h.live() && h.is_index() && h.obj_id == obj_id | OBJ_ID_IX && h.span == span {
                found = Some(page);
                return false;
            }
            true
        });
        found
    }

    /// Reassemble a file from its header page: every data page is looked
    /// up through the index, then checked to be live, this object's, and the
    /// span the index claims — any mismatch (a torn write, a recycled page)
    /// fails the whole file rather than splicing in a stale page.
    fn assemble(&mut self, hdr_page: u32, obj_id: u16, size: u32) -> Option<Vec<u8>> {
        if size == UNDEFINED_LEN || size > self.pages.saturating_mul(DATA_PER_PAGE) {
            return None;
        }
        let mut ix = [0u8; PAGE as usize];
        if !self.read_page(hdr_page, &mut ix) {
            return None;
        }
        let mut ix_span = 0u32;
        let mut data = [0u8; PAGE as usize];
        let mut out = Vec::with_capacity(size as usize);
        for i in 0..size.div_ceil(DATA_PER_PAGE) {
            let (span, entry) = if i < HDR_IX_ENTRIES {
                (0, i)
            } else {
                (1 + (i - HDR_IX_ENTRIES) / IX_ENTRIES, (i - HDR_IX_ENTRIES) % IX_ENTRIES)
            };
            if span != ix_span {
                let p = self.find_index_page(obj_id, span as u16)?;
                if !self.read_page(p, &mut ix) {
                    return None;
                }
                ix_span = span;
            }
            let at = if span == 0 { IX_HDR_LEN } else { IX_LEN } + 2 * entry as usize;
            let dp = u16::from_le_bytes([ix[at], ix[at + 1]]) as u32;
            if !self.read_page(dp, &mut data) {
                return None;
            }
            let h = PageHdr::parse(&data);
            if !h.live() || h.flags & F_INDEX == 0 || h.obj_id != obj_id || h.span as u32 != i {
                return None;
            }
            let take = (size - out.len() as u32).min(DATA_PER_PAGE) as usize;
            out.extend_from_slice(&data[PAGE_HDR_LEN..PAGE_HDR_LEN + take]);
        }
        Some(out)
    }

    /// Every live copy of `name` (with its leading slash, as Pixelblaze
    /// stores it) that reassembles cleanly. Normally zero or one.
    pub fn read_file_all(&mut self, name: &str) -> Vec<Vec<u8>> {
        let heads = self.find_headers(name.as_bytes());
        heads
            .into_iter()
            .filter_map(|(page, id, size)| self.assemble(page, id, size))
            .collect()
    }

    // Used by the host rig (tools/pbfs-check) for debugging; the firmware
    // path reads config.json through read_config, so it is dead there.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn read_file(&mut self, name: &str) -> Option<Vec<u8>> {
        self.read_file_all(name).into_iter().next()
    }

    /// Last resort when no live config header survives: the highest-`rev`
    /// span-0 data page that looks like a config.json, live or stale —
    /// its 251 payload bytes only. (A deleted copy's continuation pages
    /// can't be told apart from another deleted copy's, so nothing past
    /// the first page is trusted; `parse_wiring` tolerates the truncation
    /// and the keys that matter most come first in Pixelblaze's layout.)
    pub fn stale_config_head(&mut self) -> Option<Vec<u8>> {
        let mut best: Option<(i64, Vec<u8>)> = None;
        self.for_each_page(|_, pg| {
            let h = PageHdr::parse(pg);
            let written = h.obj_id != OBJ_ID_FREE
                && h.obj_id & OBJ_ID_IX == 0
                && h.span == 0
                && h.flags & (F_USED | F_FINAL) == 0
                && h.flags & F_INDEX != 0;
            if written && pg[PAGE_HDR_LEN..].starts_with(b"{\"rev\":") {
                if let Some(rev) = json_int(&pg[PAGE_HDR_LEN..], "rev") {
                    let newer = match &best {
                        None => true,
                        Some((r, _)) => rev > *r,
                    };
                    if newer {
                        best = Some((rev, pg[PAGE_HDR_LEN..].to_vec()));
                    }
                }
            }
            true
        });
        best.map(|(_, v)| v)
    }

    /// The newest settings file: the live `/config.json`, else its twin
    /// `/config2.json` (Pixelblaze writes both on every save), else the
    /// stale-page fallback. Where a torn rewrite left two live copies the
    /// higher `rev` wins.
    pub fn read_config(&mut self) -> Option<Vec<u8>> {
        for name in ["/config.json", "/config2.json"] {
            let newest = self
                .read_file_all(name)
                .into_iter()
                .max_by_key(|cfg| json_int(cfg, "rev").unwrap_or(-1));
            if newest.is_some() {
                return newest;
            }
        }
        self.stale_config_head()
    }
}

// ---------------------------------------------------------------------------
// JSON. config.json is one flat object, so a first-match scan is sound: no
// key repeats, and the quotes in the pattern keep "brightness" from
// matching inside "maxBrightness".

/// Position of the value for the first `"key":` in `json`.
fn json_value(json: &[u8], key: &str) -> Option<usize> {
    let pat_len = key.len() + 2;
    let mut at = 0usize;
    loop {
        let pos = json
            .get(at..)?
            .windows(pat_len)
            .position(|w| w[0] == b'"' && w[pat_len - 1] == b'"' && &w[1..pat_len - 1] == key.as_bytes())?
            + at;
        let mut i = pos + pat_len;
        while i < json.len() && matches!(json[i], b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
        }
        if i < json.len() && json[i] == b':' {
            i += 1;
            while i < json.len() && matches!(json[i], b' ' | b'\t' | b'\n' | b'\r') {
                i += 1;
            }
            return (i < json.len()).then_some(i);
        }
        at = pos + pat_len; // a string VALUE equal to the key: keep looking
    }
}

/// Parse the number at `i` in THOUSANDTHS (×1000, three decimals kept,
/// the rest truncated) — Pixelblaze's brightness is `0.13`, `0.475`, …
fn parse_milli(json: &[u8], mut i: usize) -> Option<i64> {
    let neg = json.get(i) == Some(&b'-');
    if neg {
        i += 1;
    }
    let digits_start = i;
    let mut int: i64 = 0;
    while json.get(i).is_some_and(u8::is_ascii_digit) {
        int = int.saturating_mul(10).saturating_add((json[i] - b'0') as i64);
        i += 1;
    }
    if i == digits_start {
        return None;
    }
    let mut milli = int.saturating_mul(1000);
    if json.get(i) == Some(&b'.') {
        i += 1;
        let mut scale = 100;
        while let Some(d) = json.get(i).filter(|d| d.is_ascii_digit()) {
            milli = milli.saturating_add((d - b'0') as i64 * scale);
            scale /= 10;
            i += 1;
        }
    }
    Some(if neg { -milli } else { milli })
}

/// Numeric value of `key` in thousandths (see [parse_milli]).
pub fn json_milli(json: &[u8], key: &str) -> Option<i64> {
    parse_milli(json, json_value(json, key)?)
}

/// Numeric value of `key`, truncated to an integer.
pub fn json_int(json: &[u8], key: &str) -> Option<i64> {
    json_milli(json, key).map(|m| m / 1000)
}

/// Raw bytes of `key`'s string value. Pixelblaze's own keys never carry
/// escapes; a backslash (user-typed, in `name`) bails to None rather than
/// unescaping — callers only need the length or an enum-like word.
pub fn json_str<'j>(json: &'j [u8], key: &str) -> Option<&'j [u8]> {
    let i = json_value(json, key)?;
    if json[i] != b'"' {
        return None;
    }
    let start = i + 1;
    let mut j = start;
    while j < json.len() {
        match json[j] {
            b'"' => return Some(&json[start..j]),
            b'\\' => return None,
            _ => j += 1,
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Mapping to Luxel.

/// LED wiring lifted from Pixelblaze's config.json, already in Luxel's
/// codes. Field-by-field optional: any missing or unmappable value is None
/// and the board default covers it. There is no data pin — a Pixelblaze v3
/// hardwires DATA to GPIO23 and CLK to GPIO18.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct PbWiring {
    /// `pixelCount`. Always > 0.
    pub pixels: Option<u32>,
    /// `ledType` through [map_led_type]: Luxel protocol code
    /// (`leds::Protocol::as_u8`, 0 = sk9822, 1 = ws2812).
    pub protocol: Option<u8>,
    /// `colorOrder` through [map_color_order]: Luxel `outpipe::ColorOrder`
    /// code, RELATIVE to the mapped protocol's native wire order — None
    /// whenever the protocol is.
    pub order: Option<u8>,
    /// `brightness` — the main-page slider, 0..1 — as 0..255.
    pub bri_255: Option<u8>,
    /// `maxBrightness` — Settings → "Limit brightness", 0..1 — as 0..255.
    /// Pixelblaze scales the slider by this limiter, so the light the strip
    /// actually showed is `bri_255 × max_bri_255 / 255`; a device left at
    /// slider 1.0 / limit 0.5 is a half-brightness device.
    pub max_bri_255: Option<u8>,
}

/// Pixelblaze `ledType` → Luxel protocol code. 1 is the WS2812/SK6812
/// single-wire family, 2 is APA102/SK9822 (clocked). Everything else (0 =
/// none, the output-expander and WS2801 types) has no Luxel encoder →
/// None.
pub fn map_led_type(t: i64) -> Option<u8> {
    match t {
        1 => Some(1),
        2 => Some(0),
        _ => None,
    }
}

/// Luxel's `ColorOrder::PERMS` (outpipe.rs), semantics out[i] = in[perm[i]].
const LUXEL_PERMS: [[u8; 3]; 6] = [
    [0, 1, 2], // 0 rgb
    [0, 2, 1], // 1 rbg
    [1, 0, 2], // 2 grb
    [1, 2, 0], // 3 gbr
    [2, 0, 1], // 4 brg
    [2, 1, 0], // 5 bgr
];

/// The wire order each Luxel encoder emits on its own (leds.rs:
/// `encode_ws2812` writes G,R,B; `encode_sk9822` writes B,G,R), as
/// wire position → logical channel. Both happen to be self-inverse.
fn native_order(luxel_protocol: u8) -> Option<[u8; 3]> {
    match luxel_protocol {
        1 => Some([1, 0, 2]),
        0 => Some([2, 1, 0]),
        _ => None,
    }
}

/// Pixelblaze `colorOrder` ("RGB" … "BGR" — the strip's WIRE order, the
/// same thing WLED's COL_ORDER_* names) → Luxel `ColorOrder` code. Luxel's
/// order is a PRE-encoder remap with identity 0 ("rgb") and the encoders
/// already emit each chip's native order, so the code is the permutation P
/// with native∘P = pb_order, solved here rather than tabulated: P[N[i]] =
/// W[i]. Hence "GRB" on a ws2812 is the identity, and on an sk9822 it is
/// code 4 ("brg"). RGBW orders → None (no Luxel RGBW encoder).
pub fn map_color_order(pb_order: &[u8], luxel_protocol: u8) -> Option<u8> {
    if pb_order.len() != 3 {
        return None;
    }
    let mut want = [0u8; 3];
    for (slot, c) in want.iter_mut().zip(pb_order) {
        *slot = match c.to_ascii_uppercase() {
            b'R' => 0,
            b'G' => 1,
            b'B' => 2,
            _ => return None,
        };
    }
    if want[0] == want[1] || want[1] == want[2] || want[0] == want[2] {
        return None;
    }
    let native = native_order(luxel_protocol)?;
    let mut perm = [0u8; 3];
    for i in 0..3 {
        perm[native[i] as usize] = want[i];
    }
    LUXEL_PERMS.iter().position(|p| *p == perm).map(|c| c as u8)
}

/// 0..1 (thousandths) → 0..255, rounded. Negative is garbage → None;
/// above 1 clamps.
fn unit_to_255(milli: i64) -> Option<u8> {
    (milli >= 0).then(|| ((milli.min(1000) * 255 + 500) / 1000) as u8)
}

/// Parse the wiring out of a config.json byte buffer (whole or, from the
/// stale-page fallback, its first 251 bytes). Pure — tools/pbfs-check
/// tests this against real dumps.
pub fn parse_wiring(cfg: &[u8]) -> PbWiring {
    let mut w = PbWiring::default();
    w.pixels = json_int(cfg, "pixelCount")
        .filter(|&v| v > 0)
        .map(|v| v.min(u32::MAX as i64) as u32);
    // `ledType` is the key a v3 writes; `pixelType` is accepted in case an
    // older/newer firmware named it so (neither appears twice).
    w.protocol = json_int(cfg, "ledType")
        .or_else(|| json_int(cfg, "pixelType"))
        .and_then(map_led_type);
    w.order = w
        .protocol
        .and_then(|p| json_str(cfg, "colorOrder").and_then(|o| map_color_order(o, p)));
    w.bri_255 = json_milli(cfg, "brightness").and_then(unit_to_255);
    w.max_bri_255 = json_milli(cfg, "maxBrightness").and_then(unit_to_255);
    w
}

/// Mount + newest config + [parse_wiring]. None only when nothing that
/// looks like a Pixelblaze config exists in the region.
pub fn extract_wiring(read: ReadFn<'_>, region_len: u32) -> Option<PbWiring> {
    let mut fs = PbFs::open(read, region_len)?;
    let cfg = fs.read_config()?;
    Some(parse_wiring(&cfg))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact key layout of a real v3 (rev 216) config.json, name and
    /// pattern id replaced.
    const CFG: &[u8] = br#"{"rev":216,"name":"Pixelblaze_ABCDEF","brandName":"","pixelCount":180,"brightness":1,"maxBrightness":0.5,"colorOrder":"GRB","dataSpeed":3500000,"lastProgramPath":"/p/xxxxxxxxxxxxxxxxx","startupMode":2,"simpleUiMode":false,"learningUiMode":false,"ledType":2,"sequenceTimer":15000,"sequencerMode":2,"discoveryEnabled":true,"timezone":"America/Denver","autoOffEnable":false,"autoOffStartHour":0,"autoOffStartMinute":0,"autoOffEndHour":0,"autoOffEndMinute":0,"cpuSpeed":240,"networkPowerSave":false,"mapperFit":0}"#;

    #[test]
    fn parses_the_v3_shape() {
        let w = parse_wiring(CFG);
        assert_eq!(w.pixels, Some(180));
        assert_eq!(w.protocol, Some(0)); // ledType 2 = APA102/SK9822
        assert_eq!(w.order, Some(4)); // GRB relative to sk9822's native BGR
        assert_eq!(w.bri_255, Some(255));
        assert_eq!(w.max_bri_255, Some(128));
        assert_eq!(json_int(CFG, "rev"), Some(216));
        assert_eq!(json_str(CFG, "name"), Some(&b"Pixelblaze_ABCDEF"[..]));
    }

    #[test]
    fn ws2812_and_brightness_rounding() {
        let cfg = br#"{"rev":118,"pixelCount":300,"brightness":0.13,"maxBrightness":1,"colorOrder":"GRB","ledType":1}"#;
        let w = parse_wiring(cfg);
        assert_eq!(w.pixels, Some(300));
        assert_eq!(w.protocol, Some(1));
        assert_eq!(w.order, Some(0)); // GRB IS a ws2812's native order
        assert_eq!(w.bri_255, Some(33)); // 0.13 × 255 = 33.15
        assert_eq!(w.max_bri_255, Some(255));
        let cfg = br#"{"brightness":0.475,"maxBrightness":1.5,"colorOrder":"RGB","ledType":1,"pixelCount":0}"#;
        let w = parse_wiring(cfg);
        assert_eq!(w.bri_255, Some(121)); // 121.125
        assert_eq!(w.max_bri_255, Some(255)); // clamped
        assert_eq!(w.order, Some(2)); // RGB wire on a GRB chip = "grb" remap
        assert_eq!(w.pixels, None); // 0 is not a strip
        assert_eq!(parse_wiring(br#"{"brightness":-0.1}"#).bri_255, None);
    }

    #[test]
    fn unmappable_and_truncated_and_junk() {
        // output expander / RGBW: protocol None takes order with it
        let w = parse_wiring(br#"{"pixelCount":8,"colorOrder":"GRBW","ledType":3,"brightness":1}"#);
        assert_eq!((w.pixels, w.protocol, w.order, w.bri_255), (Some(8), None, None, Some(255)));
        let w = parse_wiring(br#"{"pixelCount":8,"colorOrder":"GRBW","ledType":2}"#);
        assert_eq!((w.protocol, w.order), (Some(0), None));
        // stale-page fallback hands over the first 251 bytes: the keys that
        // fit parse, the rest are None, a cut-off number is harmless
        let w = parse_wiring(&CFG[..251]);
        assert_eq!((w.pixels, w.bri_255, w.max_bri_255), (Some(180), Some(255), Some(128)));
        assert_eq!(w.protocol, None); // "ledType" lies past the cut …
        assert_eq!(w.order, None); // … and the order is only meaningful against it
        assert_eq!(parse_wiring(&CFG[..CFG.len() - 1]), parse_wiring(CFG));
        let w = parse_wiring(&[0xFF, 0x22, 0x7B, 0x00, b'"']);
        assert_eq!(w, PbWiring::default());
        assert_eq!(parse_wiring(b"").pixels, None);
        assert_eq!(json_str(br#"{"name":"a\"b"}"#, "name"), None);
        assert_eq!(json_int(br#"{"name":"rev","rev":7}"#, "rev"), Some(7));
    }

    /// Derivation check for map_color_order: for every order and both
    /// protocols, Pixelblaze's wire bytes (logical RGB laid out in
    /// colorOrder) must equal Luxel's (ColorOrder perm applied first, then
    /// the encoder's native order).
    #[test]
    fn wire_order_roundtrip() {
        let c = [10u8, 20, 30]; // (R,G,B)
        for (proto, native) in [(1u8, [1usize, 0, 2]), (0u8, [2usize, 1, 0])] {
            for name in ["RGB", "RBG", "GRB", "GBR", "BRG", "BGR", "grb"] {
                let want: [u8; 3] = core::array::from_fn(|i| match name.as_bytes()[i].to_ascii_uppercase() {
                    b'R' => c[0],
                    b'G' => c[1],
                    _ => c[2],
                });
                let code = map_color_order(name.as_bytes(), proto).unwrap() as usize;
                let permuted: [u8; 3] = core::array::from_fn(|i| c[LUXEL_PERMS[code][i] as usize]);
                let luxel_wire: [u8; 3] = core::array::from_fn(|i| permuted[native[i]]);
                assert_eq!(luxel_wire, want, "{name} on protocol {proto}: luxel code {code}");
            }
        }
        // same answers as wledfs::map_color_order's tables (WLED 0 GRB, 4 BGR)
        assert_eq!(map_color_order(b"GRB", 1), Some(0));
        assert_eq!(map_color_order(b"BGR", 0), Some(0));
        assert_eq!(map_color_order(b"GRB", 0), Some(4));
        assert_eq!(map_color_order(b"GRB", 2), None);
        assert_eq!(map_color_order(b"GRBW", 1), None);
        assert_eq!(map_color_order(b"GGB", 1), None);
        assert_eq!(map_led_type(0), None);
        assert_eq!(map_led_type(1), Some(1));
        assert_eq!(map_led_type(2), Some(0));
        assert_eq!(map_led_type(3), None);
    }

    // -- a synthetic SPIFFS image ---------------------------------------

    fn put_page(img: &mut [u8], page: usize, obj_id: u16, span: u16, flags: u8, body: &[u8]) {
        let at = page * PAGE as usize;
        img[at..at + 2].copy_from_slice(&obj_id.to_le_bytes());
        img[at + 2..at + 4].copy_from_slice(&span.to_le_bytes());
        img[at + 4] = flags;
        img[at + 5..at + 5 + body.len()].copy_from_slice(body);
    }

    /// One file: header page `hdr`, payload spread over `data_pages`.
    /// `live` false = deleted (bit 7 cleared on every page, as SPIFFS does).
    fn put_file(img: &mut [u8], hdr: usize, obj_id: u16, name: &[u8], data_pages: &[usize], payload: &[u8], live: bool) {
        let del = if live { F_DELETED } else { 0 };
        let mut body = alloc::vec![0xFFu8; PAGE as usize - 5];
        body[..3].fill(0xFF); // pad
        body[3..7].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        body[7] = TYPE_FILE;
        body[8..8 + NAME_LEN].fill(0);
        body[8..8 + name.len()].copy_from_slice(name);
        for (i, &dp) in data_pages.iter().enumerate() {
            let at = IX_HDR_LEN - 5 + 2 * i;
            body[at..at + 2].copy_from_slice(&(dp as u16).to_le_bytes());
        }
        // flags: used, final, index cleared; deleted bit per `live`
        let flags = (!(F_USED | F_FINAL | F_INDEX)) & (0x7F | del);
        put_page(img, hdr, obj_id | OBJ_ID_IX, 0, flags, &body);
        for (i, chunk) in payload.chunks(DATA_PER_PAGE as usize).enumerate() {
            let flags = (!(F_USED | F_FINAL)) & (0x7F | del);
            put_page(img, data_pages[i], obj_id, i as u16, flags, chunk);
        }
    }

    fn run(img: &[u8]) -> Option<Vec<u8>> {
        let mut read = |off: u32, buf: &mut [u8]| {
            let o = off as usize;
            img.get(o..o + buf.len()).map(|s| buf.copy_from_slice(s)).is_some()
        };
        PbFs::open(&mut read, img.len() as u32)?.read_config()
    }

    #[test]
    fn index_walk_picks_the_live_copy_not_the_highest_rev() {
        let mut img = alloc::vec![0xFFu8; 2 * BLOCK as usize];
        // a 300-byte config spans two data pages; make the rev obvious
        let mut live = alloc::vec::Vec::from(&br#"{"rev":5,"pixelCount":180,"brandName":""#[..]);
        live.resize(280, b'x');
        live.extend_from_slice(br#"","ledType":2,"colorOrder":"GRB","brightness":0.5}"#);
        let mut stale = alloc::vec::Vec::from(&br#"{"rev":9,"pixelCount":300,"brandName":""#[..]);
        stale.resize(280, b'y');
        stale.extend_from_slice(br#"","ledType":1,"colorOrder":"RGB","brightness":1}"#);
        // stale copy first AND later in flash than the live one, higher rev
        put_file(&mut img, 1, 0x15, b"/config.json", &[2, 3], &stale, false);
        put_file(&mut img, 4, 0x15, b"/config.json", &[5, 6], &live, true);
        put_file(&mut img, 7, 0x15, b"/config.json", &[8, 9], &stale, false);
        // config2.json twin, also live, lower rev — must not win
        let twin = br#"{"rev":4,"pixelCount":1,"ledType":1,"colorOrder":"GRB","brightness":1}"#;
        put_file(&mut img, 17, 0x1d, b"/config2.json", &[18], twin, true);
        let cfg = run(&img).unwrap();
        assert_eq!(cfg, live);
        let w = parse_wiring(&cfg);
        assert_eq!((w.pixels, w.protocol, w.order, w.bri_255), (Some(180), Some(0), Some(4), Some(128)));

        // a torn rewrite: both copies live — the higher rev wins
        put_file(&mut img, 7, 0x15, b"/config.json", &[8, 9], &stale, true);
        assert_eq!(run(&img).unwrap(), stale);

        // the live headers torn (deleted) with the twin gone: stale-page
        // fallback hands back the highest rev's FIRST page only
        put_file(&mut img, 4, 0x15, b"/config.json", &[5, 6], &live, false);
        put_file(&mut img, 7, 0x15, b"/config.json", &[8, 9], &stale, false);
        put_file(&mut img, 17, 0x1d, b"/config2.json", &[18], twin, false);
        let head = run(&img).unwrap();
        assert_eq!(head.len(), DATA_PER_PAGE as usize);
        assert_eq!(&head[..], &stale[..DATA_PER_PAGE as usize]);
        assert_eq!(parse_wiring(&head).pixels, Some(300));

        // a data page the index points at but that belongs to another
        // object (recycled) fails the file instead of splicing
        put_file(&mut img, 4, 0x15, b"/config.json", &[5, 6], &live, true);
        put_page(&mut img, 6, 0x16, 1, !(F_USED | F_FINAL), &live[251..]);
        assert_eq!(run(&img).unwrap().len(), DATA_PER_PAGE as usize); // fell back

        // nothing config-shaped at all
        let blank = alloc::vec![0xFFu8; BLOCK as usize];
        assert!(run(&blank).is_none());
        assert!(PbFs::open(&mut |_, _| false, 100).is_none());
    }
}
