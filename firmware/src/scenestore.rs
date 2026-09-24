//! Pure scene-list algebra — the half of `scenes.rs` that touches no flash,
//! no locks and no device, so a host test can drive it (Gitea #478).
//!
//! Same split as `patlog.rs` (format) vs `patterns.rs` (executor): everything
//! here is a total function over a scene list, a wire body and a byte cap, and
//! `tools/patlog-check` compiles this exact file for the host so
//! `cargo test --workspace` covers it. `scenes.rs` supplies the flash, the
//! critical sections and the id counter.

use alloc::string::String;
use alloc::vec::Vec;

use luxel_core::jsonview::{push_hex, push_piece, push_u32};
use luxel_core::scene::{self, Scene};

/// Scene ids are `seq ^ ID_MASK`. A different constant from the pattern
/// store's (`patterns.rs` `ID_MASK`) so a scene id can never be mistaken for
/// a pattern id by eye, or by a client that lost track of which list it came
/// from.
pub const ID_MASK: u32 = 0x5cef_0a17;

pub fn id_hex(seq: u32) -> String {
    let mut out = String::new();
    push_hex(&mut out, seq ^ ID_MASK, 8);
    out
}

pub fn seq_of(id: &str) -> Option<u32> {
    u32::from_str_radix(id, 16).ok().map(|v| v ^ ID_MASK)
}

/// The counter value a freshly loaded list should continue from: one past
/// the highest sequence any stored id decodes to. Nothing is persisted, so
/// the ids a device hands out are a function of what it already holds.
pub fn next_seq(list: &[Scene]) -> u32 {
    let mut next = 1u32;
    for s in list {
        if let Some(seq) = seq_of(&s.id) {
            next = next.max(seq.wrapping_add(1));
        }
    }
    next
}

/// The flash/wire bytes for a whole scene list: every block back to back, in
/// exactly the format `POST /api/scenes` accepts.
///
/// Grows a `String` the ordinary way, so the device uses [`blob_try`]
/// instead — this one is for the host tests and for callers with a real
/// allocator.
pub fn blob_of(list: &[Scene]) -> String {
    let mut out = String::new();
    for s in list {
        scene::serialize(s, &mut out);
    }
    out
}

/// The most `serialize` ∘ `parse` can ADD to a body.
///
/// Every `L` line already needs all ten fields and every binding line is
/// echoed verbatim or dropped, so the only growth is `S -` (3 B) becoming
/// `S <8 hex>` (10 B). 128 B is margin on top of that; the host test
/// `serialize_never_outgrows_its_body` is what holds it.
pub const SLACK: usize = 128;

/// [`blob_of`] in ONE fallible allocation of `hint` bytes.
///
/// The device builds this blob from an HTTP handler, on a heap a live scene
/// has already eaten — on the Seengreat panel at the two-pattern-layer cap
/// `heap_largest` is ~6.6 KB. A `String` that doubles its way to 4 KiB there
/// is an allocator panic, i.e. a reboot (Gitea #724), so the caller passes
/// the size it knows the result cannot exceed (the stored blob's length plus
/// the body plus [`SLACK`]) and a failure to reserve it becomes an error the
/// console can show.
pub fn blob_try(list: &[Scene], hint: usize, free: usize) -> Result<String, String> {
    let mut out = String::new();
    if out.try_reserve_exact(hint).is_err() {
        return Err(no_memory(free));
    }
    for s in list {
        scene::serialize(s, &mut out);
    }
    Ok(out)
}

/// `scenes: not enough memory to save (N B free)` — the scene write path
/// REPORTING an out-of-memory instead of taking it (Gitea #724). `free` is
/// the device's free heap at the attempt, so the number in the log says how
/// close it was.
pub fn no_memory(free: usize) -> String {
    let mut m = String::from("scenes: not enough memory to save (");
    push_u32(&mut m, free as u32);
    push_piece(&mut m, " B free)");
    m
}

/// What a scene write needs to know about the device beyond the list and
/// the body it was handed.
pub struct Limits {
    /// The whole blob's ceiling — [`crate::patterns::BLOB_MAX`].
    pub max: usize,
    /// Pattern layers this board can hold resident.
    pub layers: usize,
    /// Bytes the stored blob occupies TODAY, so the new one is one exact
    /// reservation rather than a doubling `String`.
    pub cur_len: usize,
    /// Free heap at the attempt, for [`no_memory`].
    pub free: usize,
}

/// `scenes: store full (N of MAX B)` — assembled without `format!`, which
/// costs the firmware image a full `Arguments` plumbing per call site.
pub fn too_big(n: usize, max: usize) -> String {
    let mut m = String::from("scenes: store full (");
    push_u32(&mut m, n as u32);
    push_piece(&mut m, " of ");
    push_u32(&mut m, max as u32);
    push_piece(&mut m, " B)");
    m
}

/// A scene whose `pat` layers cannot all be resident is refused at the door,
/// so the store never holds one the device could not show. `scene: layer N
/// does not fit`, N being the 1-based LAYER index (not the pattern-layer
/// index) — the same message and numbering the mirror and the render task's
/// per-layer heap check use, so the web translates one string.
pub fn fits_layers(s: &Scene, max: usize) -> Result<(), String> {
    let mut pat = 0usize;
    for (i, l) in s.layers.iter().enumerate() {
        if l.kind() == scene::LayerKind::Pattern {
            pat += 1;
            if pat > max {
                let mut m = String::from("scene: layer ");
                push_u32(&mut m, i as u32 + 1);
                push_piece(&mut m, " does not fit");
                return Err(m);
            }
        }
    }
    Ok(())
}

/// The list a POST would produce, plus the id the record ends up with.
///
/// `id` is the route's id (`POST /api/scenes/<id>`, which must already
/// exist) or `None` for a bare `POST /api/scenes`, where the body's own `S`
/// id is honoured and `next` is assigned only for `S -`. Errors, in order: a
/// parse failure from `luxel_core::scene`, a layer the board cannot hold
/// (`max_layers`), an unknown route id, or the blob not fitting `max` — in
/// which case NOTHING changes, unlike the playlist, whose oversized
/// definition is applied live and silently lost at the next reboot.
pub fn upsert(
    mut list: Vec<Scene>,
    body: &str,
    id: Option<&str>,
    next: u32,
    lim: &Limits,
) -> Result<(Vec<Scene>, String, String), String> {
    let mut sc = scene::parse(body)?;
    fits_layers(&sc, lim.layers)?;
    // The route's id outranks the block's, so `S -` replaces in place.
    if let Some(r) = id {
        sc.id = String::from(r);
    }
    let at = list
        .iter()
        .position(|s| s.id == sc.id)
        .filter(|_| !sc.id.is_empty());
    if at.is_none() && id.is_some() {
        return Err(String::from("no such scene"));
    }
    if sc.id.is_empty() {
        sc.id = id_hex(next);
    }
    let target = sc.id.clone();
    // The list is taken BY VALUE and edited in place: the caller already
    // holds a clone of the resident one, and a second `to_vec` here was a
    // whole extra copy of every scene on the write path's peak (#724).
    match at {
        Some(i) => list[i] = sc,
        None => list.push(sc),
    }
    // A replace only ever shrinks what it replaced, so the stored length
    // plus this body plus SLACK bounds the result either way.
    let blob = blob_try(&list, lim.cur_len + body.len() + SLACK, lim.free)?;
    if blob.len() > lim.max {
        return Err(too_big(blob.len(), lim.max));
    }
    Ok((list, target, blob))
}

/// The list a DELETE would produce. Takes `list` by value for the same
/// reason [`upsert`] does.
pub fn remove(mut list: Vec<Scene>, id: &str) -> Result<Vec<Scene>, String> {
    let before = list.len();
    list.retain(|s| s.id != id);
    if list.len() == before {
        return Err(String::from("no such scene"));
    }
    Ok(list)
}

/// Split a playlist `I` line's first token: `S<sceneId>` is a scene item,
/// anything else is a pattern item. The ONE place the prefix is stripped, so
/// everything downstream sees a bare id plus the flag.
pub fn item_token(tok: &str) -> (bool, &str) {
    match tok.strip_prefix('S') {
        Some(sid) if scene::valid_id(sid) => (true, sid),
        _ => (false, tok),
    }
}

/// Filter a persisted playlist body, dropping every item that names scene
/// `id` (its `I S<id>` line and the binding lines under it). `None` when
/// nothing matched, so the caller can skip the flash write.
///
/// Works on the TEXT because the playlist has no serializer: flash holds the
/// client's POST body verbatim and that text is canonical.
pub fn drop_scene_lines(text: &str, id: &str) -> Option<String> {
    let mut out = String::new();
    let mut dropping = false;
    let mut changed = false;
    for line in text.lines() {
        match line.split_whitespace().next().unwrap_or("") {
            "I" => {
                let tok = line.split_whitespace().nth(1).unwrap_or("");
                let (scene, sid) = item_token(tok);
                dropping = scene && sid == id;
                if dropping {
                    changed = true;
                    continue;
                }
            }
            // binding lines belong to the item above them
            "C" | "P" => {
                if dropping {
                    continue;
                }
            }
            _ => dropping = false,
        }
        out.push_str(line);
        out.push('\n');
    }
    changed.then_some(out)
}

/// Local wall clock as `(y, mo, d, h, m, s)` from unix seconds with the
/// timezone offset already applied.
///
/// Howard Hinnant's civil-from-days, integer only. The VM has its own private
/// copy (`vm.rs civil_from_unix`); hoisting one into a shared module crosses
/// into another agent's files this cycle, so it is tracked instead.
pub fn civil_from_unix(secs: i64) -> (u16, u8, u8, u8, u8, u8) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
    let year = (if month <= 2 { y + 1 } else { y }) as u16;
    (
        year,
        month,
        day,
        (rem / 3600) as u8,
        (rem % 3600 / 60) as u8,
        (rem % 60) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = 3840;
    /// Layers the imaginary board affords — only the pattern-layer count is
    /// checked, and the fixtures below are colour layers.
    const LAYERS: usize = 2;

    /// The host has a real allocator, so `cur_len`/`free` only have to be
    /// honest enough that `blob_try`'s reservation is an upper bound.
    fn lim(list: &[Scene], layers: usize) -> Limits {
        Limits {
            max: MAX,
            layers,
            cur_len: blob_of(list).len(),
            free: usize::MAX,
        }
    }

    fn one(name: &str, layers: usize) -> String {
        let mut s = String::from("S - ");
        s.push_str(name);
        s.push('\n');
        for _ in 0..layers {
            s.push_str("L color 0 0 0 0 normal 100 none fill 1\nK ff8800\n");
        }
        s
    }

    #[test]
    fn ids_round_trip_and_are_not_pattern_ids() {
        for seq in [1u32, 2, 7, 4096, u32::MAX] {
            let id = id_hex(seq);
            assert_eq!(id.len(), 8, "{id}");
            assert!(scene::valid_id(&id), "{id}");
            assert_eq!(seq_of(&id), Some(seq));
        }
        // the pattern store's mask, so the two namespaces cannot collide by
        // accident on the same counter value
        assert_ne!(ID_MASK, 0x5eed_1e55);
    }

    #[test]
    fn a_bare_post_assigns_an_id_and_a_second_one_appends() {
        let (list, a, _) = upsert(Vec::new(), &one("first", 1), None, 1, &lim(&[], LAYERS)).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(a, id_hex(1));
        let l = lim(&list, LAYERS);
        let (list, b, _) = upsert(list, &one("second", 1), None, 2, &l).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(b, id_hex(2));
        assert_eq!(next_seq(&list), 3);
    }

    #[test]
    fn posting_to_an_id_replaces_in_place() {
        let (list, a, _) = upsert(Vec::new(), &one("first", 1), None, 1, &lim(&[], LAYERS)).unwrap();
        let l = lim(&list, LAYERS);
        let (list, b, _) = upsert(list, &one("edited", 2), Some(&a), 9, &l).unwrap();
        assert_eq!(a, b);
        assert_eq!(list.len(), 1, "a replace must not append");
        assert_eq!(list[0].name, "edited");
        assert_eq!(list[0].layers.len(), 2);
    }

    #[test]
    fn a_blob_over_the_cap_is_refused_and_changes_nothing() {
        // fill until one more scene would not fit
        let mut list: Vec<Scene> = Vec::new();
        let mut seq = 1u32;
        loop {
            let l = lim(&list, LAYERS);
            match upsert(list.clone(), &one("filler", 8), None, seq, &l) {
                Ok((next, _, _)) => {
                    list = next;
                    seq += 1;
                }
                Err(e) => {
                    assert!(e.starts_with("scenes: store full ("), "{e}");
                    assert!(e.ends_with(" of 3840 B)"), "{e}");
                    break;
                }
            }
            assert!(seq < 500, "the cap was never reached");
        }
        // the list that survived still fits, and the refusal left it alone
        let before = blob_of(&list);
        assert!(before.len() <= MAX);
        let l = lim(&list, LAYERS);
        assert!(upsert(list.clone(), &one("one more", 8), None, seq, &l).is_err());
        assert_eq!(blob_of(&list), before);
    }

    #[test]
    fn a_blob_round_trips_through_parse_all() {
        let (list, _, _) = upsert(Vec::new(), &one("a", 2), None, 1, &lim(&[], LAYERS)).unwrap();
        let l = lim(&list, LAYERS);
        let (list, _, blob) = upsert(list, &one("b", 1), None, 2, &l).unwrap();
        assert_eq!(blob, blob_of(&list), "upsert's blob is the list's blob");
        let back = scene::parse_all(&blob).expect("parse_all");
        assert_eq!(back, list, "flash bytes must reload identically");
        assert_eq!(blob_of(&back), blob, "and re-serialize to a fixed point");
    }

    #[test]
    fn delete_removes_exactly_one() {
        let (list, a, _) = upsert(Vec::new(), &one("a", 1), None, 1, &lim(&[], LAYERS)).unwrap();
        let l = lim(&list, LAYERS);
        let (list, _, _) = upsert(list, &one("b", 1), None, 2, &l).unwrap();
        let after = remove(list, &a).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].name, "b");
        assert!(
            remove(after, &a).is_err(),
            "a second delete is an error"
        );
    }

    #[test]
    fn posting_to_an_unknown_id_is_no_such_scene() {
        // the route's id must already exist — the same rule the mirror has,
        // and the same message, so the web translates one string
        for id in ["nope", "00ZZ0011", "5cef0a16"] {
            assert_eq!(
                upsert(Vec::new(), &one("x", 1), Some(id), 1, &lim(&[], LAYERS)).unwrap_err(),
                "no such scene"
            );
        }
    }

    #[test]
    fn a_scene_with_more_pattern_layers_than_the_board_affords_is_refused() {
        // two colour layers, then three pattern layers: layer 5 (1-based) is
        // the third pattern layer and the first one over a cap of 2
        let body = "S - too many\n\
                    L color 0 0 0 0 normal 100 none fill 1\n\
                    L color 0 0 0 0 normal 100 none fill 1\n\
                    L pat 0 0 0 0 normal 100 none fill 1\nI 5eed1c92\n\
                    L pat 0 0 0 0 normal 100 none fill 1\nI 5eed1c9d\n\
                    L pat 0 0 0 0 normal 100 none fill 1\nI 5eed1c9c\n";
        assert_eq!(
            upsert(Vec::new(), body, None, 1, &lim(&[], 2)).unwrap_err(),
            "scene: layer 5 does not fit"
        );
        // the same scene on a board that affords three is fine
        assert!(upsert(Vec::new(), body, None, 1, &lim(&[], 3)).is_ok());
    }

    #[test]
    fn a_parse_error_names_its_line() {
        let e = upsert(
            Vec::new(),
            "S - x\nL bogus 0 0 0 0 normal 100 none fill 1\n",
            None,
            1,
            &lim(&[], LAYERS),
        )
        .unwrap_err();
        assert!(e.starts_with("scene: line 2: "), "{e}");
    }

    // ---- the fallible blob build (Gitea #724) ----

    /// `blob_try`'s reservation is `cur_len + body.len() + SLACK`, and it is
    /// an UPPER bound only because `serialize` never writes more than the
    /// body it parsed plus a fresh id. If this ever fails, the device's
    /// scene save is one infallible `String` growth away from an allocator
    /// panic again — raise SLACK, do not relax the test.
    #[test]
    fn serialize_never_outgrows_its_body() {
        let bodies = [
            // the smallest legal block
            "S -\n",
            // `S -` becoming a real id is the only growth there is
            "S - probe scene\nL color 0 0 0 0 normal 100 none fill 1\nK 101010\n",
            // every layer kind, every binding line
            "S - everything\n\
             L pat -3 4 16 16 add 50 black contain 3\n\
             N base\n\
             I 5eed1c92\n\
             C speed 32768 100\n\
             P xy\n\
             R 60 0:ff0000 128:00ff00 255:0000ff\n\
             L text 0 0 0 0 normal 100 none fill 1\n\
             T lit HELLO WORLD\n\
             F large ffcc00 c bounce 40\n\
             L text 0 0 0 0 normal 100 none fill 1\n\
             T clock HH:MM\n\
             L sprite 24 24 16 16 normal 100 none fill 1\n\
             I 5eed1e57\n\
             L color 0 0 0 0 multiply 12 luma tile 0\n\
             K ff8800\n",
            // a name at the 64-byte ceiling
            "S - 0123456789012345678901234567890123456789012345678901234567890123\n\
             L color 0 0 0 0 normal 100 none fill 1\n",
        ];
        for body in bodies {
            let sc = scene::parse(body).expect(body);
            let mut out = String::new();
            scene::serialize(&sc, &mut out);
            assert!(
                out.len() <= body.len() + SLACK,
                "{} B in, {} B out (slack {}): {body}",
                body.len(),
                out.len(),
                SLACK
            );
            // and with the id assigned, which is where the growth is
            let mut with_id = sc.clone();
            with_id.id = id_hex(1);
            let mut out = String::new();
            scene::serialize(&with_id, &mut out);
            assert!(out.len() <= body.len() + SLACK, "{body}");
        }
    }

    #[test]
    fn blob_try_is_blob_of_with_a_reservation() {
        let (list, _, _) = upsert(Vec::new(), &one("a", 2), None, 1, &lim(&[], LAYERS)).unwrap();
        let want = blob_of(&list);
        let got = blob_try(&list, want.len(), 0).expect("host always has the memory");
        assert_eq!(got, want);
        // a hint under the true size still produces the right bytes (the
        // host grows it); the device's bound is what keeps that off the
        // Xtensa allocator
        assert_eq!(blob_try(&list, 1, 0).unwrap(), want);
    }

    #[test]
    fn the_out_of_memory_message_carries_the_free_heap() {
        assert_eq!(no_memory(6616), "scenes: not enough memory to save (6616 B free)");
    }

    // ---- the playlist's scene items ----

    #[test]
    fn the_i_token_splits_scene_items_from_pattern_items() {
        assert_eq!(item_token("5eed1c92"), (false, "5eed1c92"));
        assert_eq!(item_token("S5cef0a16"), (true, "5cef0a16"));
        // an `S` that is not followed by a well-formed id is a pattern id
        // that happens to start with one — the ids are hex, so this can only
        // be a malformed line, and it must not be read as a scene
        assert_eq!(item_token("Snot-an-id"), (false, "Snot-an-id"));
        assert_eq!(item_token(""), (false, ""));
    }

    #[test]
    fn deleting_a_scene_drops_its_playlist_item_and_its_bindings() {
        let body = "D 30\nX 500\nI 5eed1c92 -1\nC speed 32768\nI S5cef0a16 10\nC ignored 1\nI 5eed1c9d 5\nP x\n";
        let out = drop_scene_lines(body, "5cef0a16").expect("changed");
        assert_eq!(
            out,
            "D 30\nX 500\nI 5eed1c92 -1\nC speed 32768\nI 5eed1c9d 5\nP x\n"
        );
        // nothing to drop → no flash write
        assert!(drop_scene_lines(body, "5cef0a99").is_none());
    }

    // ---- the clock source ----

    #[test]
    fn civil_matches_known_instants() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        // 2026-09-24T01:02:03Z
        assert_eq!(civil_from_unix(1_790_298_123), (2026, 9, 25, 1, 2, 3));
        // a leap day
        assert_eq!(civil_from_unix(1_709_164_800), (2024, 2, 29, 0, 0, 0));
    }
}
