//! The sprite store's routes (Gitea #740).
//!
//! A sprite is a first-class store record, not a sprite-tagged pattern: one
//! `LXSP` blob (`luxel_core::sprite`) living in a record's SOURCE extent
//! with no bytecode beside it. That record IS the wire body of
//! `GET`/`POST /api/sprites/<id>`, the flash bytes, the playground's stored
//! bytes and the compositor's input — there is no second representation and
//! nothing on this path parses, compiles or executes anything.
//!
//! Same split as `scenestore.rs` (format) vs `scenes.rs` (executor): the
//! record's reader and validator are `luxel_core::sprite`, the log is
//! `patterns.rs`, and this module is only the API shape — list bodies,
//! upsert-by-name, replace-by-id and the error sentences.

use alloc::string::String;
use alloc::vec::Vec;

use luxel_core::jsonview::{push_escaped, push_piece, push_u32, Chunks};
use luxel_core::sprite::{self, SpriteHead, SpriteView, SPRITE_HDR, SPRITE_MAX_BYTES};

use crate::patterns;

/// `{"ok":false,"error":"…"}`. Every message this module produces is a
/// `&'static str` from [`sprite::check`] or one of the two "no such sprite"
/// sentences, so nothing needs escaping.
fn err(msg: &str) -> String {
    let mut out = String::from("{\"ok\":false,\"error\":\"");
    push_piece(&mut out, msg);
    push_piece(&mut out, "\"}");
    out
}

/// `{"ok":false,"error":"no such sprite"}` — the answer to an id the store
/// does not hold, on every verb.
pub fn no_such() -> String {
    err("no such sprite")
}

/// `GET /api/sprites` →
/// `{"sprites":[{"id","name","w","h","frames","fps","colors","bytes"},…],
/// "max_bytes":16384}`.
///
/// Segmented like `/api/scenes` and `/api/patterns` (Gitea #753): a library
/// of sprites is a multi-KB body, and a `String` doubling its way there
/// needs a contiguous block a fragmented heap need not have. Each record
/// costs ONE 12-byte header read — the geometry comes from
/// [`SpriteHead`], never from the payload — so listing 60 sprites reads
/// 720 bytes of flash, not 720 KB.
pub fn list_json() -> Chunks {
    // ~88 B per entry sizes the segment index only; an under-estimate costs
    // one index reallocation and can never refuse the response.
    let mut out = Chunks::with_hint(48 + patterns::sprite_count() * 88);
    push_piece(&mut out, "{\"sprites\":[");
    let mut first = true;
    patterns::for_each_sprite(SPRITE_HDR, |id, name, bytes, head| {
        // A head that did not read (flash busy on a `flashmap-off` board)
        // lists as zeros rather than dropping the record: the console can
        // still open, rename or delete it.
        let h = SpriteHead::parse(head).unwrap_or_default();
        if !first {
            push_piece(&mut out, ",");
        }
        first = false;
        push_piece(&mut out, "{\"id\":\"");
        push_piece(&mut out, id);
        push_piece(&mut out, "\",\"name\":\"");
        push_escaped(&mut out, name);
        push_piece(&mut out, "\",\"w\":");
        push_u32(&mut out, h.w as u32);
        push_piece(&mut out, ",\"h\":");
        push_u32(&mut out, h.h as u32);
        push_piece(&mut out, ",\"frames\":");
        push_u32(&mut out, h.frames as u32);
        push_piece(&mut out, ",\"fps\":");
        push_u32(&mut out, h.fps as u32);
        push_piece(&mut out, ",\"colors\":");
        push_u32(&mut out, h.colors as u32);
        push_piece(&mut out, ",\"bytes\":");
        push_u32(&mut out, bytes);
        push_piece(&mut out, "}");
    });
    push_piece(&mut out, "],\"max_bytes\":");
    push_u32(&mut out, SPRITE_MAX_BYTES as u32);
    push_piece(&mut out, "}");
    out
}

/// A sprite's record as MAPPED memory — `GET /api/sprites/<id>`'s body,
/// served in place. The caller must hold the record's pin (a resident scene
/// layer does) or accept that a concurrent save can move the bytes.
pub fn get(id: &str) -> Option<&'static [u8]> {
    patterns::sprite_slice(id)
}

/// [`get`]'s `flashmap-off` fallback: the record in a transient fallible Vec.
pub fn get_vec(id: &str) -> Option<Vec<u8>> {
    patterns::sprite_vec(id)
}

/// A sprite's record length — the `Content-Length` snapshot
/// `GET /api/sprites/<id>` takes before it streams anything, and the
/// existence check the route needs anyway. `None` = no such sprite.
pub fn record_len(id: &str) -> Option<usize> {
    patterns::sprite_stat(id).map(|(_, len)| len)
}

/// `POST /api/sprites` (`id` None) and `POST /api/sprites/<id>` (replace).
///
/// The body is the raw record — never `from_utf8`'d, it is binary. The
/// NAME comes out of the record, not the route: the record is canonical, so
/// the name the store lists and the name inside the bytes cannot disagree
/// (and a rename is just a save with a different name in it).
///
/// A bare POST whose record names an existing sprite REPLACES it, keeping
/// its id — the pattern store's rule. `POST /<id>` replaces THAT record
/// whatever the name says, which is what makes a rename possible.
pub async fn save(body: &[u8], id: Option<&str>) -> String {
    // The one validation: `sprite::check` is also the diagnosis, and its
    // sentences already start `sprite: `.
    if let Err(why) = sprite::check(body) {
        return err(why);
    }
    let Some(sp) = SpriteView::parse(body) else {
        // unreachable — `check` passed — but never unwrap a wire body
        return err("sprite: record is malformed");
    };
    if let Some(want) = id {
        if !patterns::sprite_exists(want) {
            return no_such();
        }
    }
    patterns::save_sprite(sp.name, body, id).await
}

/// `DELETE /api/sprites/<id>`.
///
/// A resident scene layer pointing at it simply draws nothing from the next
/// frame on — exactly what a deleted pattern does to a `pat` layer. The
/// bytes stay put until a compaction reclaims them, so a layer reading the
/// record in place cannot fault on it.
pub async fn delete(id: &str) -> String {
    patterns::delete_sprite(id).await
}
