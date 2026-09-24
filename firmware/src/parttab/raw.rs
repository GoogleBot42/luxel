//! The partition-table *codec*: entry decoding and the queries the rest of
//! the firmware asks of a table. Pure — no flash, no esp-hal, `alloc` and
//! nothing else — so `tools/parttab-check` compiles this exact file for the
//! host and tests it against tables that `esp-idf-part` (the crate espflash
//! uses) serializes from the real `firmware/partitions*.csv`.
//!
//! The on-flash format is ESP-IDF's: 32-byte entries, each
//! `AA 50 | type | subtype | offset u32 | size u32 | label[16] | flags u32`,
//! terminated by an `EBEB…` row carrying the MD5 the bootloader verifies.

use alloc::vec::Vec;

const ENTRY_MAGIC: [u8; 2] = [0xAA, 0x50];
pub const TYPE_APP: u8 = 0x00;
pub const TYPE_DATA: u8 = 0x01;
pub const SUBTYPE_OTA0: u8 = 0x10;
pub const SUBTYPE_OTA1: u8 = 0x11;
/// `data, ota` — the otadata partition's subtype.
pub const SUBTYPE_OTADATA: u8 = 0x00;

/// One partition-table entry.
#[derive(Clone, Copy)]
pub struct Part {
    pub ptype: u8,
    pub subtype: u8,
    pub offset: u32,
    pub len: u32,
    /// The entry's label, NUL-padded exactly as it sits on flash.
    pub label: [u8; 16],
}

impl Part {
    /// One past this partition's last byte.
    pub fn end(&self) -> u32 {
        self.offset.saturating_add(self.len)
    }

    /// Does the entry's label equal `name`? Labels are the only stable
    /// identity a data partition has — subtypes are all `spiffs` — so this
    /// is how both the store and the migrator find their region.
    pub fn labelled(&self, name: &str) -> bool {
        let b = name.as_bytes();
        b.len() <= 16
            && self.label[..b.len()] == *b
            && self.label[b.len()..].iter().all(|c| *c == 0)
    }
}

/// Every entry of a raw partition table, in table order.
pub fn entries(table: &[u8]) -> Vec<Part> {
    let mut out = Vec::new();
    for e in table.chunks_exact(32) {
        if e[0..2] != ENTRY_MAGIC {
            break; // MD5 row (0xEBEB) or erased flash
        }
        let mut label = [0u8; 16];
        label.copy_from_slice(&e[12..28]);
        out.push(Part {
            ptype: e[2],
            subtype: e[3],
            offset: u32::from_le_bytes(e[4..8].try_into().unwrap()),
            len: u32::from_le_bytes(e[8..12].try_into().unwrap()),
            label,
        });
    }
    out
}

/// The app partitions of a table, in table order.
pub fn app_entries(table: &[u8]) -> Vec<Part> {
    let mut v = entries(table);
    v.retain(|p| p.ptype == TYPE_APP);
    v
}

/// The app partition with this OTA subtype.
pub fn app_slot(table: &[u8], subtype: u8) -> Option<Part> {
    app_entries(table).into_iter().find(|p| p.subtype == subtype)
}

/// The data partition with this label.
pub fn data_labelled(table: &[u8], name: &str) -> Option<Part> {
    entries(table)
        .into_iter()
        .find(|p| p.ptype == TYPE_DATA && p.labelled(name))
}

/// Is this table one of ours? A Luxel table always carries the two data
/// partitions this firmware owns by name. WLED's (and every other foreign
/// table) does not — which is what tells [crate::migrate] "an older layout
/// of mine, carry the data across" apart from "somebody else's flash,
/// that is the takeover's job".
pub fn is_luxel(table: &[u8]) -> bool {
    data_labelled(table, "storage").is_some() && data_labelled(table, "assets").is_some()
}

/// Highest byte any entry of `table` reaches — what the flash part must
/// actually have. Writing a table that points past the die is a brick.
pub fn flash_needed(table: &[u8]) -> u32 {
    entries(table).iter().map(|p| p.end()).max().unwrap_or(0)
}

/// The slot `/api/ota` writes into: the OTA app partition of `table` that
/// is NOT the one executing. `booted` is where the running image was
/// MMU-mapped from (`u32::MAX` when that could not be established) and
/// `running_len` is that image's byte length.
///
/// Deliberately not "whatever `otadata` does not point at". With `otadata`
/// erased — which the layout migration leaves behind on every boot in
/// which it runs and then fails after `settle_into_ota0`, and which the
/// bootloader only repairs on the NEXT boot — the esp-bootloader-esp-idf
/// arithmetic answers `ota_0` for a device executing from `ota_0`, and the
/// update erases the code it is running. That is the Seengreat brick of
/// 2026-09-21 (Gitea #655). The running slot is a fact about the MMU, so
/// that is the only input this trusts; a device that cannot say where it
/// is running from does not update.
pub fn ota_target(table: &[u8], booted: u32, running_len: u32) -> Result<Part, &'static str> {
    if booted == u32::MAX {
        return Err("cannot tell which slot is running — not updating");
    }
    if !entries(table)
        .iter()
        .any(|p| p.ptype == TYPE_DATA && p.subtype == SUBTYPE_OTADATA)
    {
        return Err("no otadata partition (old partition table? reflash serially)");
    }
    let Some(t) = app_entries(table)
        .into_iter()
        .find(|p| (p.subtype == SUBTYPE_OTA0 || p.subtype == SUBTYPE_OTA1) && p.offset != booted)
    else {
        return Err("no second OTA slot to update into");
    };
    if t.offset < booted.saturating_add(running_len) && booted < t.end() {
        return Err("the free OTA slot overlaps the running image");
    }
    Ok(t)
}
