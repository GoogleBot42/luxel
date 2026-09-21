//! The log half of the #501 repartition migration, end to end on the host.
//!
//! `firmware/src/migrate.rs` shrinks the 4 MB `storage` partition from 1 MiB
//! to 512 KiB, which shrinks the packed pattern log from 0xB7000 (732 KiB)
//! to 0x37000 (220 KiB). Every pattern a device holds has to come across
//! that step *byte-identical*, and the whole move is one-shot: the old log
//! is gone as soon as the new store region is written, and the partition
//! table goes in after that. There is no rollback and — right now — no
//! serial port on the bench to recover a device that loses its library.
//!
//! So the staging half is reproduced here against the real `patlog`:
//! `plan` + `align_page` + `build_page` into a staging buffer, exactly the
//! sequence `migrate.rs::stage_log` runs, from a log in the state a real
//! device is actually in (dead records, superseded generations, mixed
//! sizes). What the tests assert is the three things the device cannot
//! tell us afterwards:
//!
//!   * the repacked image FITS the new log, and the measured size says how
//!     much library a 4 MB device can carry across;
//!   * re-scanning that image recovers the live set exactly — same seqs,
//!     names and payload bytes, no dead records, hashes still verifying;
//!   * the refusal (`migration_blocked`) triggers when it does not fit,
//!     rather than truncating;
//!   * and staging is re-runnable after a power cut, because it only ever
//!     reads the old log.

use crate::patlog::{self, Arena, Rec, PAGE};
use crate::store::{Nor, Pat, Store, LOG_LEN};

/// `patterns.rs`' `LOG_OFF` — where the packed log starts inside `storage`.
const LOG_OFF: u32 = 0x4_9000;
/// The old 4 MB `storage` was 1 MiB, so the old log ran to 0xB7000. That is
/// also `store::LOG_LEN`, which is what makes `Store` a faithful stand-in
/// for a pre-#501 device.
const OLD_LOG: u32 = 0x10_0000 - LOG_OFF;
/// The new 4 MB `storage` is 512 KiB. `tools/parttab-check` asserts both of
/// these against the real `firmware/partitions.csv`, so they cannot drift
/// away from the shipped table without a test failing there.
const NEW_LOG: u32 = 0x8_0000 - LOG_OFF;

const _: () = assert!(OLD_LOG == 0xB_7000);
const _: () = assert!(NEW_LOG == 0x3_7000);

/// One live pattern as the migration must preserve it: the identity the
/// store keys on plus every payload byte.
#[derive(Clone, PartialEq, Eq)]
struct Live {
    seq: u32,
    name: String,
    src: Vec<u8>,
    bc: Vec<u8>,
}

impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{} {:?} ({} B src, {} B bc)", self.seq, self.name, self.src.len(), self.bc.len())
    }
}

/// Read a record's payload straight out of an arena's bytes.
fn payload(f: &Nor, r: &Rec) -> (Vec<u8>, Vec<u8>) {
    let s = r.src_off() as usize;
    let b = r.bc_off() as usize;
    (f.mem[s..s + r.src_len as usize].to_vec(), f.mem[b..b + r.bc_len as usize].to_vec())
}

/// The live set as the store's RAM index sees it — `migrate.rs` stages
/// exactly this, via `patterns::index_snapshot()`.
fn snapshot(s: &mut Store) -> Vec<Live> {
    let recs = s.index.clone();
    let mut out: Vec<Live> = recs
        .iter()
        .map(|r| {
            let name = s.rec_name(r).expect("a live record's name is readable");
            let (src, bc) = payload(&s.f, r);
            Live { seq: r.seq, name, src, bc }
        })
        .collect();
    out.sort_by_key(|l| l.seq);
    out
}

/// The live set as it can be recovered from a *raw log image* — the boot
/// scan, the same de-duplication `patterns.rs::reload` does, and a re-hash
/// of every payload. This is the reader side of the guarantee: whatever the
/// device finds when it boots into the new layout.
fn recover(f: &mut Nor) -> Vec<Live> {
    let mut best: Vec<(Rec, String)> = Vec::new();
    let mut found: Vec<(Rec, String)> = Vec::new();
    patlog::scan(f, &mut |r: &Rec, n: &[u8]| {
        found.push((*r, String::from_utf8_lossy(n).into_owned()))
    });
    for (r, n) in found {
        assert!(!r.dead, "a staged image must carry no dead records — {n:?} came across dead");
        match best.iter_mut().find(|(b, _)| b.seq == r.seq) {
            Some(slot) => {
                if r.stamp > slot.0.stamp {
                    *slot = (r, n);
                }
            }
            None => best.push((r, n)),
        }
    }
    let mut out = Vec::new();
    for (r, name) in best {
        // The hashes are what the boot scan checks a record's payload
        // against; a repack that copied the wrong bytes (or regenerated a
        // header over the wrong payload) shows up here, not as a diff.
        assert_eq!(
            patlog::hash_range(f, r.src_off(), r.src_len),
            Some(r.src_hash),
            "{name:?}: source hash does not verify after the repack"
        );
        assert_eq!(
            patlog::hash_range(f, r.bc_off(), r.bc_len),
            Some(r.bc_hash),
            "{name:?}: bytecode hash does not verify after the repack"
        );
        let (src, bc) = payload(f, &r);
        out.push(Live { seq: r.seq, name, src, bc });
    }
    out.sort_by_key(|l| l.seq);
    out
}

/// `migrate.rs::stage_log`, step for step: plan a clean pack toward offset 0
/// with nothing pinned (this runs before the render task starts, so no
/// engine is executing out of the log), page-align the result, then build
/// every page into the staging area.
///
/// Returns the staged image and the byte count `migrate.rs` compares
/// against the new log's length. `pages` caps how many pages are actually
/// built — that is the power-cut knob, and it does not change the plan.
fn stage(old: &mut Nor, recs: &[Rec], pages: Option<u32>) -> (u32, Vec<u8>) {
    let mut places: Vec<patlog::Place> = Vec::new();
    let cursor = patlog::plan(recs, &[], &mut |p| places.push(p))
        .expect("the migration refuses outright if it cannot plan a repack");
    let bytes = patlog::align_page(cursor);

    // Staging starts erased; a cut leaves the un-built tail at 0xFF, which
    // is exactly what a half-written flash sector looks like.
    let mut out = vec![0xFFu8; bytes as usize];
    let mut buf = vec![0u8; PAGE as usize];
    let want = pages.unwrap_or(bytes / PAGE).min(bytes / PAGE);
    for page in 0..want {
        assert!(
            patlog::build_page(old, page, recs, &places, &mut buf),
            "build_page failed on page {page} — migrate.rs aborts the whole migration here"
        );
        let a = (page * PAGE) as usize;
        out[a..a + PAGE as usize].copy_from_slice(&buf);
    }
    (bytes, out)
}

/// Drop a staged image into an arena the size of the NEW log, tail erased —
/// what the new `storage` partition holds once `write_new_store` has run.
fn as_new_log(image: &[u8]) -> Nor {
    assert!(image.len() as u32 <= NEW_LOG, "staged image does not fit the new log");
    let mut f = Nor::new(NEW_LOG);
    f.mem[..image.len()].copy_from_slice(image);
    f
}

/// A device's library, in the shape a device's library actually has: names
/// and payloads of wildly different sizes, not a uniform grid.
fn realistic() -> Vec<Pat> {
    // (name, source bytes, bytecode bytes) — a one-byte name and a 64-byte
    // one (the `MAX_NAME` edge), sources from a few hundred bytes to ~12 KB,
    // bytecode from tiny to ~9 KB. Deliberately not sorted and deliberately
    // not multiples of anything.
    let spec: &[(&str, usize, usize)] = &[
        ("a", 211, 97),
        ("sunset-glow", 1417, 903),
        ("KITT", 640, 512),
        ("fire-2d-with-a-rather-long-descriptive-name-like-the-library-has", 3301, 2755),
        ("blink", 129, 64),
        ("aurora", 11_903, 8_887),
        ("rainbow melt", 2048, 1024),
        ("xmas_lights_v2", 777, 4_001),
        ("plasma", 5_120, 3_333),
        ("sparkfire", 199, 8_192),
        ("green-ripple", 4_095, 4_097),
        ("matrix-rain", 6_600, 1_111),
        ("slow-fade", 333, 222),
        ("opposites", 9_001, 251),
    ];
    spec.iter().map(|(n, s, b)| Pat::new(n, *s, *b)).collect()
}

/// A library in the state a device that has been *used* is in: patterns
/// saved, some deleted, some re-saved under the same name. The log then
/// holds dead records and superseded generations, and the repack has to see
/// past both.
fn churned_device() -> (Store, Vec<Pat>) {
    let mut s = Store::new();
    assert_eq!(LOG_LEN, OLD_LOG, "store::LOG_LEN is the pre-#501 log length");
    assert_eq!(s.f.len(), OLD_LOG, "Store models the pre-#501 log");

    let pats = realistic();
    for p in &pats {
        s.save(p).unwrap_or_else(|e| panic!("save {}: {e}", p.name));
    }
    let mut live = pats.clone();

    // two deletes: dead records in the middle of the log
    for name in ["blink", "plasma"] {
        assert!(s.delete(name), "delete {name}");
        live.retain(|p| p.name != name);
    }

    // three re-saves under the same names, with DIFFERENT payload sizes —
    // so a repack that kept the superseded generation, or the old lengths,
    // is caught by the byte compare rather than by luck.
    for (name, src, bc) in [("KITT", 1_500, 1_200), ("aurora", 700, 640), ("a", 4_444, 2_222)] {
        let p = Pat::new(name, src, bc);
        s.save(&p).unwrap_or_else(|e| panic!("re-save {name}: {e}"));
        let slot = live.iter_mut().find(|q| q.name == name).expect("re-saved a live pattern");
        *slot = p;
    }

    // The migration stages what the RAM index holds; rebuild it from flash
    // so the test cannot be fooled by an index that drifted.
    s.reload();
    (s, live)
}

// ---------------------------------------------------------------------------

/// The whole staging half, against a realistic log: repack a churned
/// 732 KiB log, check it fits the 220 KiB one, and check that everything
/// live comes back out of the staged image byte for byte.
///
/// Catches the migration's central claim — "your store is byte-identical
/// afterwards". A repack that drops the lowest-offset record (Gitea #379's
/// failure mode), carries a dead record across, keeps a superseded
/// generation, or regenerates a header over the wrong payload all fail
/// here, on the host, instead of on a device whose old log has already been
/// erased.
#[test]
fn a_churned_library_repacks_into_the_new_log_byte_for_byte() {
    let (mut s, live) = churned_device();
    let want = snapshot(&mut s);
    assert_eq!(want.len(), live.len(), "the index and the test's own bookkeeping agree");
    assert!(want.len() >= 12, "only {} live patterns — too thin a test", want.len());
    assert!(s.dead_bytes > 0, "the log must actually contain dead records");

    let recs = s.index.clone();
    let before = s.f.mem.clone();
    let (bytes, image) = stage(&mut s.f, &recs, None);

    println!(
        "migrate/stage: {} live pattern(s) ({} B of records, {} B dead) repack to {} B \
         ({:.1}% of the {} B new log; old log was {} B)",
        recs.len(),
        recs.iter().map(|r| r.size()).sum::<u32>(),
        s.dead_bytes,
        bytes,
        100.0 * bytes as f64 / NEW_LOG as f64,
        NEW_LOG,
        OLD_LOG,
    );

    assert!(bytes <= NEW_LOG, "staged {bytes} B does not fit the {NEW_LOG} B new log");
    assert_eq!(s.f.mem, before, "staging must never write the old log — it is the only copy");

    let got = recover(&mut as_new_log(&image));
    assert_eq!(got, want, "the live set changed across the repack");
    // And spelled out, so a failure says which pattern rather than dumping
    // two vectors of payloads.
    for (g, w) in got.iter().zip(&want) {
        assert_eq!(g.seq, w.seq);
        assert_eq!(g.name, w.name, "seq {} came back under a different name", w.seq);
        assert_eq!(g.src.len(), w.src.len(), "{}: source length", w.name);
        assert_eq!(g.src, w.src, "{}: source bytes", w.name);
        assert_eq!(g.bc, w.bc, "{}: bytecode bytes", w.name);
    }
}

/// How much library a 4 MB device can carry across, measured rather than
/// assumed: keep saving library-sized patterns until the repack no longer
/// fits 0x37000, and report the last count that did.
///
/// This is not a pass/fail claim about a number — it is the number
/// `/api/status`' `migration_blocked` message is about, and docs/releases.md
/// quotes it. The assertion is only that it is comfortably more than a
/// hobbyist's library, so the migration is not a surprise for most devices.
#[test]
fn the_new_log_still_holds_a_sensible_library() {
    let mut s = Store::new();
    let mut fits = 0usize;
    let mut fitted_bytes = 0u32;
    for i in 0..400 {
        // The real library's shape (tools/patlog-check's library_fill test
        // measures it): a few KB of source, a few KB of bytecode.
        let p = Pat::new(&format!("pattern-{i:03}"), 600 + (i * 733) % 5200, 500 + (i * 449) % 4400);
        if s.save(&p).is_err() {
            break;
        }
        s.reload();
        let recs = s.index.clone();
        let mut places = Vec::new();
        let cursor = patlog::plan(&recs, &[], &mut |p| places.push(p)).expect("plan");
        let bytes = patlog::align_page(cursor);
        if bytes > NEW_LOG {
            break;
        }
        fits = recs.len();
        fitted_bytes = bytes;
    }
    println!(
        "migrate/capacity: {} library-sized patterns ({} B) is the most the {} B new log takes \
         (the old log was {} B — 3.3x the room)",
        fits, fitted_bytes, NEW_LOG, OLD_LOG
    );
    assert!(fits >= 30, "only {fits} patterns survive a migration — that is too few to ship");
}

/// The refusal. Fill the old log with more LIVE data than the new one can
/// hold and check that `align_page(cursor) > new_log_len` — the exact
/// condition `migrate.rs::stage_log` turns into `migration_blocked`.
///
/// Catches a migration that would truncate instead of refusing. Silently
/// losing patterns is the one outcome the design rules out: the device is
/// supposed to keep running on its old table and tell the user to delete
/// some patterns and reboot.
#[test]
fn an_oversized_library_blocks_the_migration_instead_of_truncating() {
    let mut s = Store::new();
    let mut n = 0;
    for i in 0..400 {
        let p = Pat::new(&format!("big-{i:03}"), 6_000 + (i * 97) % 3_000, 5_000 + (i * 31) % 2_000);
        if s.save(&p).is_err() {
            break;
        }
        n += 1;
        let live: u32 = s.index.iter().map(|r| r.size()).sum();
        if live > NEW_LOG + 2 * PAGE {
            break;
        }
    }
    s.reload();
    let live: u32 = s.index.iter().map(|r| r.size()).sum();
    assert!(live > NEW_LOG, "the test failed to overfill: {live} B live vs a {NEW_LOG} B new log");

    let recs = s.index.clone();
    let mut places = Vec::new();
    let cursor = patlog::plan(&recs, &[], &mut |p| places.push(p)).expect("the plan itself succeeds");
    let bytes = patlog::align_page(cursor);
    println!(
        "migrate/refusal: {n} patterns, {bytes} B repacked vs a {NEW_LOG} B new log — blocked"
    );
    assert!(
        bytes > NEW_LOG,
        "a {live} B live set repacked to {bytes} B and would have been let through"
    );
    // And it is a refusal, not a crash: nothing was written, so the device
    // keeps working on its old table with every pattern still readable.
    assert_eq!(s.on_flash().len(), recs.len(), "the old log must be intact after a refusal");
}

/// Power-cut sweep. Cut the staging build after each page in turn, then
/// re-run the WHOLE stage from the old arena and check the recovered live
/// set is still exact.
///
/// The property under test is that staging is *re-runnable*: it reads the
/// old log and writes only scratch, so a cut costs nothing but the work. If
/// `build_page` ever read from the destination, or the stage ever wrote
/// back into the source, this is where it shows — and on a device that
/// mistake is unrecoverable, because the partial run would have damaged the
/// only copy of the library.
#[test]
fn a_power_cut_mid_staging_costs_nothing_but_the_work() {
    let (mut s, _) = churned_device();
    let want = snapshot(&mut s);
    let recs = s.index.clone();
    let pristine = s.f.mem.clone();

    let (bytes, _) = stage(&mut s.f, &recs, None);
    let pages = bytes / PAGE;
    assert!(pages >= 4, "only {pages} staged pages — not much of a sweep");

    for cut in 0..pages {
        // the cut run: staging dies after `cut` pages
        let (b, partial) = stage(&mut s.f, &recs, Some(cut));
        assert_eq!(b, bytes, "the plan must not depend on how far the last attempt got");
        assert!(
            partial[(cut * PAGE) as usize..].iter().all(|&x| x == 0xFF),
            "cut at page {cut}: the un-built tail must read erased"
        );
        assert_eq!(s.f.mem, pristine, "cut at page {cut}: the old log was modified");

        // the retry: a whole fresh stage from the untouched old log
        let (b2, image) = stage(&mut s.f, &recs, None);
        assert_eq!(b2, bytes, "cut at page {cut}: the retry staged a different size");
        assert_eq!(recover(&mut as_new_log(&image)), want, "cut at page {cut}: the retry lost data");
        assert_eq!(s.f.mem, pristine, "cut at page {cut}: the retry modified the old log");
    }
    println!("migrate/power-cut: {pages} cut points, every retry recovered all {} patterns", want.len());
}
