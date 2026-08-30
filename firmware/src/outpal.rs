//! Device output palette: the fourth device-level post-process stage
//! (Gitea #139). A stop list stored on the device recolors every finished
//! frame by luma, the same transform a pattern's `setOutputPalette` does —
//! and it *composes* with that stage rather than replacing it, exactly the
//! way the device blur/glow settings stack on top of a pattern's own.
//!
//! Persistence lives here rather than in `config.rs` because a palette is
//! variable-length: the nvs partition's four sectors are already spoken for
//! (WiFi, device settings, MQTT, the boot guard), so this rides the
//! pattern store's reserved-key blob mechanism — the same one the device
//! map, playlist and resume records use.
//!
//! Blob format (v1): `u8 version=1  u8 amount_pct  u8 count` then `count`
//! stops of `(pos, r, g, b)` bytes. Integrity is the store's (CRC per
//! item), so there is no checksum of our own. A `count` of 0 — which is
//! also what a cleared palette writes — means "no palette".

use alloc::vec::Vec;

use esp_println::println;
use luxel_core::outpipe::MAX_OUTPUT_PALETTE_STOPS;

use crate::patterns;
use crate::shared;

const VERSION: u8 = 1;

fn serialize(stops: &[(u8, [u8; 3])], amount_pct: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + stops.len() * 4);
    out.push(VERSION);
    out.push(amount_pct);
    out.push(stops.len() as u8);
    for (pos, c) in stops {
        out.push(*pos);
        out.extend_from_slice(c);
    }
    out
}

/// Parse a stored blob. Returns None for "no palette" — including a blob
/// written by a newer firmware, a truncated one, or a count past the cap
/// (validated BEFORE reserving, per .claude/rules/firmware.md: never size
/// an allocation from a length field read out of flash).
fn deserialize(b: &[u8]) -> Option<(Vec<(u8, [u8; 3])>, u8)> {
    if b.len() < 3 || b[0] != VERSION {
        return None;
    }
    let amount_pct = b[1];
    let count = b[2] as usize;
    if count == 0 || count > MAX_OUTPUT_PALETTE_STOPS || amount_pct > 100 {
        return None;
    }
    if b.len() < 3 + count * 4 {
        return None;
    }
    let mut stops: Vec<(u8, [u8; 3])> = Vec::with_capacity(count);
    for s in b[3..3 + count * 4].chunks_exact(4) {
        stops.push((s[0], [s[1], s[2], s[3]]));
    }
    // the writer sorts; an unsorted list is corrupt, and sample_palette
    // would quietly sample nonsense from it rather than fail
    if stops.windows(2).any(|w| w[0].0 > w[1].0) {
        return None;
    }
    Some((stops, amount_pct))
}

/// Load the persisted palette into the shared state. Call after
/// `patterns::init()` and before the render task spawns. Cheap — a
/// ≤131-byte blob — so it does not need to wait for `wait_config_up()` the
/// way the multi-KB pattern/playlist resume does. Cooking the 256-entry
/// lookup is the render task's job, behind the palette epoch.
pub fn init() {
    if let Some(b) = patterns::read_blob(patterns::PALETTE_KEY) {
        if let Some((stops, amount)) = deserialize(&b) {
            println!("palette: {} stops @ {}% from flash", stops.len(), amount);
            shared::set_post_palette(stops, amount);
        }
    }
}

/// Install a palette live and persist it. `stops` must already be sorted
/// and within the cap (`outpipe::parse_palette_stops` enforces both).
/// Returns false if the store refused the write — the palette is still
/// applied live, matching how the device map handles an unpersistable map.
pub fn store(stops: Vec<(u8, [u8; 3])>, amount_pct: u8) -> bool {
    let blob = serialize(&stops, amount_pct);
    let persisted = patterns::store_blob(patterns::PALETTE_KEY, &blob);
    if !persisted {
        println!("palette: could not persist — applied live only");
    }
    shared::set_post_palette(stops, amount_pct);
    persisted
}

/// Clear the palette live and in flash. There is no blob delete, so the
/// cleared state is a zero-count record — which `deserialize` reads as
/// "no palette" (the device map clears itself the same way).
pub fn clear() -> bool {
    let persisted = patterns::store_blob(patterns::PALETTE_KEY, &[VERSION, 0, 0]);
    shared::set_post_palette(Vec::new(), 0);
    persisted
}
