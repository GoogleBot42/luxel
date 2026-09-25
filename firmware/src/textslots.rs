//! Text-slot persistence (Gitea #745).
//!
//! A scene's `T slot n` layer reads `luxel_core::text`'s slot table, which
//! used to be empty on every boot: the slots only ever arrived from a live
//! `POST /api/text` or an HA text entity, so a device that came up playing a
//! playlist of scenes drew a BLANK text layer until a host happened to
//! re-send the value — "the text overlay takes a while to show up at first,
//! then is reliable" (Jeremy, #729 item 35).
//!
//! The slots now ride the same reserved-key blob mechanism the playlist, the
//! device map, the Layout, the output palette and the scene list use — one
//! item under [patterns::TEXT_KEY] in the storage partition.
//!
//! Blob format v1 is `luxel_core::text`'s own ([`text::encode_slots`] /
//! [`text::decode_slots`]): a version byte then `(slot, len, bytes)` per
//! NON-EMPTY slot, so the record is 1 B on an unused device and at most
//! [`text::SLOTS_BLOB_MAX`] (529 B) with all eight slots full. Integrity is
//! the store's (a CRC per item); everything else the decoder can be handed
//! — a newer version, a truncated record, a slot or length out of range,
//! invalid UTF-8 — is rejected whole rather than half-applied.
//!
//! ## Where the two ends live
//!
//! * **Load** — [`init`], from `main()`'s store-init block, next to
//!   `outpal::init` and `scenes::init`. That is before ANY task spawns,
//!   which is what makes it legal to call `text::set_slot` (the table is
//!   lock-free single-writer and otherwise belongs to the render task) and
//!   what guarantees the first frame a resumed playlist renders already has
//!   the text in it. It is deliberately NOT deferred behind
//!   `wait_config_up()` the way `resume::resume_task` is: that deferral
//!   exists because a multi-KB pattern load during WiFi bring-up OOM-panicked
//!   a boot (WiFi's own mallocs don't null-check), and this is ~1.3 KB worst
//!   case — the table (8 × 72 B) plus the mirror — in the same class as the
//!   palette and Layout blobs that already load there, and two orders below
//!   the pattern resume.
//! * **Store** — [`persist`], from `resume`'s debounced persist loop. Text
//!   slots arrive in bursts (someone typing into the console's text field,
//!   an HA automation pushing a sensor readout), which is the same
//!   flash-wear problem the single-pattern record already solved; a second
//!   embassy task's storage is not free on a board with under 100 bytes of
//!   `.stack` margin (`tools/stack-check.sh`), so they share that one.

use esp_println::println;
use luxel_core::text;

use crate::patterns;
use crate::shared;

/// Arm the debounced write. Called from [`shared::set_text_slot`], i.e. by
/// every path that sets a slot.
///
/// Cheap — it signals `resume`'s already-existing dirty flag and returns;
/// the write happens once things have been quiet for a few seconds. The
/// boot-time [`init`] restore goes through the same funnel and so arms it
/// too: that costs one read-and-compare of a ≤529 B record after DHCP,
/// which the equality check below then discards, and it buys not having to
/// carry a "loaded yet?" static on a board that has no `.bss` to spare.
pub fn mark_dirty() {
    crate::resume::mark_dirty();
}

/// Load the persisted slots into the slot table and the read-back mirror.
///
/// Call from `main()` BEFORE any task spawns — see the module docs for why
/// calling `text::set_slot` is only sound there.
pub fn init() {
    let Some(b) = patterns::read_blob(patterns::TEXT_KEY) else {
        return; // first boot, or nothing was ever set
    };
    let mut n = 0u32;
    let ok = text::decode_slots(&b, |slot, s| {
        text::set_slot(slot, s);
        shared::set_text_slot(slot, s);
        n += 1;
    });
    if !ok {
        println!("text: stored slots unreadable — starting empty");
    } else if n > 0 {
        println!("text: {} slot(s) from flash", n);
    }
}

/// Write the slot table to flash, skipping identical rewrites.
///
/// Called from `resume`'s debounce, never straight from a request handler.
/// A device too tight to build the 529-byte blob logs and gives up — the
/// slots stay live, exactly the way `outpal::store` keeps an unpersistable
/// palette (Gitea #727/#728: this path reports an out-of-memory instead of
/// taking one).
pub fn persist() {
    let Some(blob) = shared::text_slots_blob() else {
        println!("text: not enough memory to save slots");
        return;
    };
    let stored = patterns::read_blob(patterns::TEXT_KEY);
    if stored.as_deref() == Some(&blob[..]) {
        return; // wear discipline: unchanged records are not rewritten
    }
    if stored.is_none() && blob.len() == 1 {
        return; // nothing has ever been set; don't create an empty record
    }
    if !patterns::store_blob(patterns::TEXT_KEY, &blob) {
        println!("text: slot flash write failed (update in progress?)");
    }
}
