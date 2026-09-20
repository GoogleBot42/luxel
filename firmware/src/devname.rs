//! The device's user-settable NAME (Gitea #538): what the title bar, the
//! Settings page and Home Assistant call this board, instead of the bare
//! `luxel-<mac6>` the MAC gives it.
//!
//! Persistence lives here rather than in `config.rs` for the same reason
//! the output palette's does: a name is variable-length and the nvs
//! partition's four sectors are all spoken for (WiFi, device settings,
//! MQTT, the boot guard). So it rides the pattern store's reserved-key
//! blob mechanism, like the device map, the playlist, the resume record
//! and the palette.
//!
//! Blob format (v1): `u8 version=1` then the name's UTF-8 bytes. Integrity
//! is the store's (a CRC per item), so there is no checksum of our own. A
//! zero-length name — which is what a cleared name writes — means "use the
//! board default".
//!
//! **The name is applied at boot.** `main()` builds the DHCP hostname (and,
//! at #536, the setup AP's SSID) from it before the network stack exists
//! and nothing re-reads it, exactly like the data pin. `POST /api/name`
//! therefore persists + updates `/api/status` live and answers
//! `reboot_required`.

use esp_println::println;

use crate::patterns;
use crate::shared;

const VERSION: u8 = 1;

/// 32 bytes. That is the 802.11 SSID limit and the width of the
/// `heapless::String<32>` `main()` builds the hostname in, so one cap
/// covers every consumer.
pub const MAX_NAME: usize = 32;

/// A name this device will accept: 1..=32 bytes of printable UTF-8.
///
/// Control bytes are out because the name reaches a serial log and (at
/// #536) an SSID; `"` and `\` are out because the name is pushed into
/// three JSON bodies, one of them `/api/status`, which the console polls
/// continuously — excluding the two JSON metacharacters keeps every one of
/// those a plain `push_piece` of the stored bytes instead of a
/// `json_escape` allocation per poll.
///
/// Byte-wise on purpose: a multi-byte UTF-8 sequence's continuation bytes
/// are all >= 0x80, so no character this rejects can hide inside one, and
/// walking bytes avoids linking `char`-decoding for a 32-byte string.
pub fn valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.bytes().any(|b| b < 0x20 || b == 0x7f || b == b'"' || b == b'\\')
}

/// Seed the shared name from flash, falling back to `default` (the board's
/// `luxel-<mac6>`). Call after `patterns::init()` and BEFORE the network
/// stack is built — `main()` reads the result back for the DHCP hostname.
/// Cheap: a ≤33-byte blob.
#[inline(never)]
pub fn init(default: &str) {
    shared::set_device_name_default(default);
    shared::set_device_name(default, false);
    let Some(blob) = patterns::read_blob(patterns::NAME_KEY) else {
        return;
    };
    let Some((&ver, rest)) = blob.split_first() else {
        return;
    };
    if ver != VERSION || rest.len() > MAX_NAME {
        return;
    }
    let Ok(name) = core::str::from_utf8(rest) else {
        return;
    };
    if valid(name) {
        println!("name: \"{}\" from flash", name);
        shared::set_device_name(name, true);
    }
}

/// Apply a name live and persist it; an EMPTY name clears the record and
/// goes back to the board default. Returns false if the store refused the
/// write — the name is applied live regardless, matching how the palette
/// and the device map handle an unpersistable value.
///
/// There is no blob delete, so the cleared state is a version byte on its
/// own, which `init` reads as "no name".
pub fn set(name: &str) -> bool {
    if name.len() > MAX_NAME {
        return false;
    }
    let mut blob = [0u8; 1 + MAX_NAME];
    blob[0] = VERSION;
    blob[1..1 + name.len()].copy_from_slice(name.as_bytes());
    let persisted = patterns::store_blob(patterns::NAME_KEY, &blob[..1 + name.len()]);
    if !persisted {
        println!("name: could not persist — applied live only");
    }
    if name.is_empty() {
        let default = shared::device_name_default();
        shared::set_device_name(&default, false);
    } else {
        shared::set_device_name(name, true);
    }
    persisted
}
