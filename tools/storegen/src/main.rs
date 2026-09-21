//! Generate — and afterwards verify — a realistic Luxel `storage` partition
//! image for the partition-migration test (Gitea #501,
//! `tools/qemu/migrate-test.py`).
//!
//! # Why this exists
//!
//! The migration's whole promise is "your patterns and your playlist survive
//! a repartition". Testing that needs two things a Python test cannot
//! honestly produce on its own:
//!
//! 1. a PRE-migration `storage` image that a real device would have written
//!    — the key area laid out by the actual `sequential-storage` crate, the
//!    pattern log laid out by the actual `firmware/src/patlog.rs` record
//!    format, with dead records where a real library has them; and
//! 2. a reader for the POST-migration one, which is NOT a byte copy: the
//!    migrator re-*stores* every reserved blob into a fresh key area and
//!    re-*packs* every live record into a fresh log, so only a real parse
//!    can say whether the data came across.
//!
//! Both halves are the same code path here, which is the point — a second
//! Python implementation of two on-flash formats would be a test of the
//! test. `gen` writes the image plus a JSON sidecar naming everything it
//! put in; `verify` takes a post-run flash dump plus that sidecar and
//! checks the store against it, byte for byte and hash for hash.
//!
//! Both formats come from the firmware sources themselves:
//!
//! * `firmware/src/patlog.rs` is compiled in directly (`#[path]`), the way
//!   `tools/patlog-check` already does — the record layout under test is
//!   literally the shipping one.
//! * `tools/patlog-check/src/store.rs` — the host replica of
//!   `patterns.rs`' save/delete/compact state machine — drives the appends,
//!   so the log this writes was built by the same sequence of operations a
//!   device performs, deletes and re-saves included.
//! * `sequential-storage` at `firmware/Cargo.toml`'s pin writes the key
//!   area (see `keyarea.rs`).
//!
//! # Usage
//!
//! ```text
//!   storegen gen     --out store.bin --sidecar store.json [--overfill]
//!   storegen migrate --pre store.bin --sidecar store.json \
//!                    --new-len 0x400000 --out store-16mb.bin
//!   storegen verify  --flash flash.bin --sidecar store.json \
//!                    --at 0x290000 --len 0x80000 [--allow-dead] [--label new]
//! ```
//!
//! Output is deterministic: every byte comes from a fixed table of names and
//! sizes and a name-seeded generator (`store::Pat::new`). No clock, no RNG,
//! no network.

#[path = "../../../firmware/src/patlog.rs"]
pub mod patlog;

/// The host replica of `patterns.rs`' store state machine, borrowed whole
/// from `tools/patlog-check`. It models the pre-#501 log exactly: a
/// `Store`'s arena is `LOG_PAGES` (183) × 4 KiB = 0xB7000, which IS the
/// 1 MiB partition's log (0x100000 - 0x49000).
#[path = "../../patlog-check/src/store.rs"]
#[allow(dead_code)]
mod store;

mod json;
mod keyarea;

use json::J;
use patlog::{Rec, PAGE};
use sequential_storage::cache::PageStateCache;
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------- geometry

/// `patterns::LOG_AT` — where the packed file log starts inside the
/// partition, on every layout.
const LOG_AT: u32 = 0x49000;
/// The pre-#501 `storage` partition: 1 MiB.
const OLD_STORE_LEN: u32 = 0x10_0000;
/// `patterns::FORMAT_VERSION`. Read out of the firmware at build time
/// rather than typed here — a bump there must not silently make this
/// generator write a store the device wipes on sight.
const FORMAT_VERSION: u32 = firmware_format_version();

/// `patterns::ID_MASK` — the API id of a pattern is `seq ^ ID_MASK`.
const ID_MASK: u32 = 0x5eed_1e55;

// The reserved key range (`patterns::RESERVED_LO ..= RESERVED_HI`) and the
// keys inside it this generator populates.
const RESERVED_LO: u32 = 0x7FFF_FFF0;
const RESERVED_HI: u32 = 0x7FFF_FFFF;
const FORMAT_KEY: u32 = 0x7FFF_FFFF;
const PLAYLIST_KEY: u32 = 0x7FFF_FFFE;
const PLAYSTATE_KEY: u32 = 0x7FFF_FFFD;
const MAP_KEY: u32 = 0x7FFF_FFFC;
const NAME_KEY: u32 = 0x7FFF_FFF8;
/// A key no shipping firmware knows. `migrate::write_new_store` sweeps the
/// WHOLE reserved range rather than a list of known keys, precisely so a
/// device carrying a blob from a NEWER release keeps it across a migration
/// applied by an older one. Nothing else in the suite covers that promise.
const FUTURE_KEY: u32 = 0x7FFF_FFF1;

/// Parse `const FORMAT_VERSION: u32 = N;` out of `firmware/src/patterns.rs`
/// at compile time. A `const fn` over `include_str!` keeps it a compile
/// error rather than a runtime surprise if the line ever changes shape.
const fn firmware_format_version() -> u32 {
    let src = include_bytes!("../../../firmware/src/patterns.rs");
    let needle = b"const FORMAT_VERSION: u32 = ";
    let mut i = 0;
    while i + needle.len() < src.len() {
        let mut k = 0;
        while k < needle.len() && src[i + k] == needle[k] {
            k += 1;
        }
        if k == needle.len() {
            let mut at = i + needle.len();
            let mut v: u32 = 0;
            let mut digits = 0;
            while at < src.len() && src[at] >= b'0' && src[at] <= b'9' {
                v = v * 10 + (src[at] - b'0') as u32;
                at += 1;
                digits += 1;
            }
            assert!(digits > 0, "FORMAT_VERSION in patterns.rs is not a decimal literal");
            return v;
        }
        i += 1;
    }
    panic!("no `const FORMAT_VERSION: u32 = ` in firmware/src/patterns.rs")
}

// ------------------------------------------------------------- the library

/// One pattern the generated store holds: name, source length, bytecode
/// length. Sizes vary on purpose — a log of equal-sized records would hide
/// every placement bug the repack could have.
struct Spec(&'static str, usize, usize);

/// The ordinary library: twelve saves, one delete, one re-save. Eleven
/// patterns survive, and the log carries the dead bytes of the two retired
/// records — which is what a bench device's store actually looks like, and
/// what makes "the migration repacked it" a visible fact rather than an
/// assumption.
const LIBRARY: &[Spec] = &[
    Spec("Rainbow", 412, 968),
    Spec("Sparkle Storm", 1_907, 3_244),
    Spec("KITT", 604, 1_120),
    Spec("Fire 2D", 5_311, 9_688),
    Spec("Breathe", 233, 540),
    Spec("Matrix Rain", 2_744, 4_096),
    Spec("Plasma", 1_118, 2_205),
    Spec("Color Wipe", 377, 812),
    Spec("Twinkle", 866, 1_733),
    Spec("Marble Cave", 12_004, 18_311),
    Spec("Aurora", 3_002, 7_451),
    Spec("Xmas Tree", 991, 1_402),
];
/// Deleted after the initial fill: its record stays in the log as dead
/// bytes and must NOT come across the migration.
const DELETED: &str = "KITT";
/// Re-saved at a different size after the fill: the first generation
/// becomes dead bytes, the second is the one that must survive.
const RESAVED: Spec = Spec("Plasma", 2_560, 4_733);

/// The `--overfill` library: the same machinery, sized so the LIVE log
/// content cannot fit the 4 MB layout's 0x37000-byte log. Twelve near-maximum
/// patterns (`patlog::MAX_SOURCE` is 32 KiB, `MAX_BC` 40 KiB) pack to roughly
/// 600 KB — comfortably over the new budget and comfortably inside the old
/// 0xB7000 log, which is the shape the refusal exists for.
const OVERFILL: &[Spec] = &[
    Spec("Huge 01", 20_011, 30_007),
    Spec("Huge 02", 20_113, 30_101),
    Spec("Huge 03", 20_227, 30_203),
    Spec("Huge 04", 20_341, 30_307),
    Spec("Huge 05", 20_453, 30_401),
    Spec("Huge 06", 20_567, 30_509),
    Spec("Huge 07", 20_681, 30_601),
    Spec("Huge 08", 20_793, 30_707),
    Spec("Huge 09", 20_907, 30_803),
    Spec("Huge 10", 21_019, 30_911),
    Spec("Huge 11", 21_131, 31_013),
    Spec("Huge 12", 21_247, 31_109),
];

// ------------------------------------------------------------------ helpers

fn parse_int(s: &str) -> Result<u32, String> {
    let t = s.trim();
    let r = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(h, 16)
    } else {
        t.parse::<u32>()
    };
    r.map_err(|e| format!("{:?}: {}", s, e))
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

fn need<'a>(args: &'a [String], name: &str) -> Result<&'a str, String> {
    arg(args, name).ok_or_else(|| format!("missing required {} <value>", name))
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err(format!("odd-length hex string ({} chars)", s.len()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

/// `seq ^ ID_MASK`, lower-case hex — the id the HTTP API and the playlist
/// blob use (`patterns::id_hex`).
fn id_hex(seq: u32) -> String {
    format!("{:08x}", seq ^ ID_MASK)
}

// ------------------------------------------------------------- gen: the log

/// Everything `gen` learned about one stored pattern, for the sidecar.
struct Stored {
    seq: u32,
    name: String,
    src: Vec<u8>,
    bc: Vec<u8>,
}

/// Fill a `Store` (the host replica of `patterns.rs`) the way a user would:
/// save everything, delete one, re-save one. Returns the live set.
fn fill(specs: &[Spec], with_churn: bool) -> (store::Store, Vec<Stored>) {
    let mut s = store::Store::new();
    let mut want: BTreeMap<String, store::Pat> = BTreeMap::new();

    for Spec(name, src_len, bc_len) in specs {
        let p = store::Pat::new(name, *src_len, *bc_len);
        s.save(&p).unwrap_or_else(|e| panic!("save {:?}: {}", name, e));
        want.insert(name.to_string(), p);
    }
    if with_churn {
        assert!(s.delete(DELETED), "delete {:?} found nothing", DELETED);
        want.remove(DELETED);
        let Spec(name, src_len, bc_len) = RESAVED;
        let p = store::Pat::new(name, src_len, bc_len);
        s.save(&p).unwrap_or_else(|e| panic!("re-save {:?}: {}", name, e));
        want.insert(name.to_string(), p);
    }

    // Take identity (seq) from the index the store itself keeps, so the
    // sidecar's seqs are the device's, not ours.
    let index = s.index.clone();
    let mut live = Vec::new();
    for r in &index {
        let name = s.rec_name(r).expect("live record has a readable name");
        let p = want.get(&name).unwrap_or_else(|| panic!("index holds an unknown name {:?}", name));
        live.push(Stored { seq: r.seq, name, src: p.src.clone(), bc: p.bc.clone() });
    }
    assert_eq!(live.len(), want.len(), "the index and the expected set disagree");
    (s, live)
}

/// `migrate::stage_log`'s arithmetic, run on the host. Returns
/// `(repacked cursor, page-rounded staged length)` — the first is where the
/// post-migration scan's cursor must land, the second is the number the
/// migrator compares against the new layout's log length.
fn staged_bytes(index: &[Rec]) -> (u32, u32) {
    let mut places = Vec::new();
    let cursor = patlog::plan(index, &[], &mut |p| places.push(p))
        .expect("the generated log must have a valid repack plan");
    (cursor, patlog::align_page(cursor))
}

fn cmd_gen(args: &[String]) -> Result<(), String> {
    let out = need(args, "--out")?;
    let sidecar = need(args, "--sidecar")?;
    let overfill = flag(args, "--overfill");
    let store_len = match arg(args, "--store-len") {
        Some(v) => parse_int(v)?,
        None => OLD_STORE_LEN,
    };
    let log_len = store_len - LOG_AT;
    if log_len != store::LOG_LEN {
        return Err(format!(
            "--store-len {:#x} gives a {:#x}-byte log, but the host store replica \
             (tools/patlog-check/src/store.rs) is fixed at {:#x} — the generator only \
             ever writes the PRE-#501 1 MiB partition",
            store_len, log_len, store::LOG_LEN
        ));
    }

    let specs = if overfill { OVERFILL } else { LIBRARY };
    let (mut st, live) = fill(specs, !overfill);
    let scan = st.reload();
    let (repacked_cursor, packed) = staged_bytes(&st.index);

    // ---- the key area: reserved blobs, through the real crate ----
    let mut key = keyarea::MemFlash::erased(keyarea::KEY_AREA_LEN);
    let mut cache = PageStateCache::<{ keyarea::PAGES }>::new();

    // A real playlist over three of the patterns that are actually stored,
    // in `playlist.rs`' line format. This is the blob Jeremy's bench devices
    // carry and the one that must come through intact.
    let ids: Vec<String> = live.iter().take(3).map(|p| id_hex(p.seq)).collect();
    let mut playlist = String::from("D 45\nX 1200\n");
    for (i, id) in ids.iter().enumerate() {
        playlist.push_str(&format!("I {} {}\n", id, if i == 1 { 90 } else { -1 }));
        if i == 0 {
            playlist.push_str("C speed 65536\nC hue 32768\n");
        }
        if i == 2 {
            playlist.push_str("P 2\n");
        }
    }

    let blobs: Vec<(u32, Vec<u8>)> = vec![
        (FORMAT_KEY, FORMAT_VERSION.to_le_bytes().to_vec()),
        (PLAYLIST_KEY, playlist.into_bytes()),
        // playlist.rs::persist_state — one byte, 1 = "was playing".
        (PLAYSTATE_KEY, vec![1]),
        // devicemap.rs::serialize, MapData::Grid — GRID_TAG then w,h u16 LE.
        (MAP_KEY, vec![0x80, 16, 0, 8, 0]),
        // devname.rs::set — version byte then the name.
        (NAME_KEY, {
            let mut v = vec![1u8];
            v.extend_from_slice(b"Bench Unit 7");
            v
        }),
        (FUTURE_KEY, b"storegen: a key from a firmware that does not exist yet".to_vec()),
    ];
    for (k, v) in &blobs {
        keyarea::store_blob(&mut key, &mut cache, *k, v);
    }
    // Prove the region reads back before anything downstream trusts it.
    for (k, v) in &blobs {
        let got = keyarea::read_blob(&mut key, *k)
            .ok_or_else(|| format!("self-check: blob {:#x} did not read back", k))?;
        if &got != v {
            return Err(format!("self-check: blob {:#x} read back wrong", k));
        }
    }

    // ---- assemble the partition image ----
    let mut img = vec![0xFFu8; store_len as usize];
    img[..keyarea::KEY_AREA_LEN as usize].copy_from_slice(&key.mem);
    // 0x20000..0x49000 is the ad-hoc live-coding slot; a device that has
    // never taken a live push leaves it erased, which is the honest state
    // for a store that was filled through the HTTP API.
    img[LOG_AT as usize..(LOG_AT + log_len) as usize].copy_from_slice(&st.f.mem);
    std::fs::write(out, &img).map_err(|e| format!("{}: {}", out, e))?;

    // ---- the sidecar ----
    let mut pats: Vec<J> = Vec::new();
    for p in &live {
        pats.push(J::Obj(vec![
            ("seq".into(), J::Num(p.seq as u64)),
            ("id".into(), J::s(&id_hex(p.seq))),
            ("name".into(), J::s(&p.name)),
            ("src_len".into(), J::Num(p.src.len() as u64)),
            ("bc_len".into(), J::Num(p.bc.len() as u64)),
            ("src_fnv".into(), J::Num(patlog::fnv1a(&p.src) as u64)),
            ("bc_fnv".into(), J::Num(patlog::fnv1a(&p.bc) as u64)),
        ]));
    }
    let blob_j: Vec<J> = blobs
        .iter()
        .map(|(k, v)| {
            J::Obj(vec![
                ("key".into(), J::Num(*k as u64)),
                ("len".into(), J::Num(v.len() as u64)),
                ("hex".into(), J::s(&hex(v))),
            ])
        })
        .collect();
    let doc = J::Obj(vec![
        ("kind".into(), J::s("luxel-storegen-sidecar")),
        ("version".into(), J::Num(1)),
        ("overfill".into(), J::Num(overfill as u64)),
        ("store_len".into(), J::Num(store_len as u64)),
        ("key_area_len".into(), J::Num(keyarea::KEY_AREA_LEN as u64)),
        ("log_at".into(), J::Num(LOG_AT as u64)),
        ("log_len".into(), J::Num(log_len as u64)),
        ("format_version".into(), J::Num(FORMAT_VERSION as u64)),
        ("reserved_lo".into(), J::Num(RESERVED_LO as u64)),
        ("reserved_hi".into(), J::Num(RESERVED_HI as u64)),
        // What the scan of the GENERATED log finds, before any migration.
        ("pre_records".into(), J::Num(scan.recs as u64)),
        ("pre_live_bytes".into(), J::Num(scan.live as u64)),
        ("pre_dead_bytes".into(), J::Num(scan.dead as u64)),
        ("pre_cursor".into(), J::Num(scan.cursor as u64)),
        // What `migrate::stage_log` will stage, page-rounded: the number it
        // compares against the new layout's log length.
        ("staged_bytes".into(), J::Num(packed as u64)),
        // Where the post-migration boot scan's cursor must land: the repack
        // is contiguous, so this is also the live-byte total.
        ("repacked_cursor".into(), J::Num(repacked_cursor as u64)),
        ("patterns".into(), J::Arr(pats)),
        ("blobs".into(), J::Arr(blob_j)),
    ]);
    std::fs::write(sidecar, doc.to_pretty()).map_err(|e| format!("{}: {}", sidecar, e))?;

    println!("storegen gen{}", if overfill { " --overfill" } else { "" });
    println!("  image      : {} ({} B)", out, store_len);
    println!("  sidecar    : {}", sidecar);
    println!("  key area   : {} reserved blob(s), format v{}", blobs.len(), FORMAT_VERSION);
    println!(
        "  log        : {} record(s) on flash, {} live, {} B live, {} B dead, cursor {}",
        scan.recs,
        live.len(),
        scan.live,
        scan.dead,
        scan.cursor
    );
    println!(
        "  repacks to : cursor {} B, {} B page-rounded ({} pages)",
        repacked_cursor,
        packed,
        packed / PAGE
    );
    Ok(())
}

// ------------------------------------------------------------------ migrate

/// Run the migrator's store move ON THE HOST: take a pre-migration partition
/// image and produce the partition image the device WOULD end up with for a
/// target partition length.
///
/// This is `migrate::stage_log` + `migrate::write_new_store` with the flash
/// ops replaced by memory ops, and nothing else:
///
///   * `patlog::scan` the old log, keep the newest live record per seq
///     (`patterns::reload`'s rule);
///   * `patlog::plan` + `patlog::build_page` repack them toward offset 0 with
///     nothing pinned — the same two functions the firmware calls, so the
///     placement under test is the shipping one;
///   * re-STORE every reserved blob into a FRESH `sequential-storage` key
///     area, key by key over the whole reserved range, which is what makes
///     the new key area a re-store rather than a copy on the device too.
///
/// It exists for the 16 MB layout, which the emulator cannot run (see
/// `tools/qemu/migrate-test.py --plan-16mb`): the resulting image goes through
/// the same `verify` the emulated 4 MB one does, so "a 4 MB-sized library
/// lands intact in a 4 MiB partition" is a checked fact rather than an
/// extrapolation. It is NOT a substitute for the emulation — it cannot see
/// power cuts, flash errors, or the partition table — and the 4 MB path is
/// deliberately covered by the real thing.
fn cmd_migrate(args: &[String]) -> Result<(), String> {
    let pre_path = need(args, "--pre")?;
    let out = need(args, "--out")?;
    let sidecar_path = need(args, "--sidecar")?;
    let new_len = parse_int(need(args, "--new-len")?)?;

    let pre = std::fs::read(pre_path).map_err(|e| format!("{}: {}", pre_path, e))?;
    let text = std::fs::read_to_string(sidecar_path).map_err(|e| format!("{}: {}", sidecar_path, e))?;
    let side = json::parse(&text)?;
    // `--pre-len` overrides the sidecar's `store_len` for the SECOND hop of a
    // chained migration (Gitea #634): the source is then the partition the
    // first hop produced — a 4 MB layout's 512 KiB `storage` — while the
    // sidecar still describes the pre-#501 1 MiB one it came from. The
    // pattern ground truth in the sidecar is length-independent, so only this
    // one number has to be told.
    let old_len = match arg(args, "--pre-len") {
        Some(v) => parse_int(v)?,
        None => side.u32("store_len")?,
    };
    if pre.len() as u32 != old_len {
        return Err(format!("{} is {} B, expected {}", pre_path, pre.len(), old_len));
    }
    if old_len <= LOG_AT {
        return Err(format!("--pre-len {:#x} leaves no log at all", old_len));
    }
    if new_len <= LOG_AT {
        return Err(format!("--new-len {:#x} leaves no log at all", new_len));
    }
    let new_log_len = new_len - LOG_AT;

    // --- the log ---
    let old_log = &pre[LOG_AT as usize..old_len as usize];
    let mut arena = Slice(old_log);
    let mut seen: Vec<(Rec, String)> = Vec::new();
    patlog::scan(&mut arena, &mut |r: &Rec, n: &[u8]| {
        seen.push((*r, String::from_utf8_lossy(n).into_owned()))
    });
    let mut best: BTreeMap<u32, Rec> = BTreeMap::new();
    for (r, _) in &seen {
        if r.dead {
            continue;
        }
        match best.get(&r.seq) {
            Some(b) if b.stamp >= r.stamp => {}
            _ => {
                best.insert(r.seq, *r);
            }
        }
    }
    // `patterns::index_snapshot` hands the migrator the index, which is sorted
    // by ascending offset — `plan` requires that order.
    let mut keep: Vec<Rec> = best.into_values().collect();
    keep.sort_by_key(|r| r.off);

    let mut places = Vec::new();
    let cursor = patlog::plan(&keep, &[], &mut |p| places.push(p))
        .ok_or("migrate: no repack plan places every record")?;
    let bytes = patlog::align_page(cursor);
    if bytes > new_log_len {
        return Err(format!(
            "migrate: BLOCKED — pattern library too large for the new layout \
             (need {} B, have {} B)",
            bytes, new_log_len
        ));
    }

    let mut new_log = vec![0xFFu8; new_log_len as usize];
    let mut page_buf = vec![0u8; PAGE as usize];
    for page in 0..bytes / PAGE {
        if !patlog::build_page(&mut arena, page, &keep, &places, &mut page_buf) {
            return Err(format!("migrate: could not build log page {}", page));
        }
        let at = (page * PAGE) as usize;
        new_log[at..at + PAGE as usize].copy_from_slice(&page_buf);
    }

    // --- the key area: a re-store, not a copy ---
    let mut old_key = keyarea::MemFlash::from_bytes(&pre[..keyarea::KEY_AREA_LEN as usize]);
    let mut new_key = keyarea::MemFlash::erased(keyarea::KEY_AREA_LEN);
    let mut cache = PageStateCache::<{ keyarea::PAGES }>::new();
    let mut moved = 0u32;
    for k in RESERVED_LO..=RESERVED_HI {
        let Some(v) = keyarea::read_blob(&mut old_key, k) else { continue };
        keyarea::store_blob(&mut new_key, &mut cache, k, &v);
        moved += 1;
    }

    let mut img = vec![0xFFu8; new_len as usize];
    img[..keyarea::KEY_AREA_LEN as usize].copy_from_slice(&new_key.mem);
    img[LOG_AT as usize..].copy_from_slice(&new_log);
    std::fs::write(out, &img).map_err(|e| format!("{}: {}", out, e))?;

    println!("storegen migrate {} -> {}", pre_path, out);
    println!("  partition  : {} B -> {} B (log {} B -> {} B)",
             old_len, new_len, old_len - LOG_AT, new_log_len);
    println!("  repacked   : {} live record(s), cursor {} B, {} B staged",
             keep.len(), cursor, bytes);
    println!("  key area   : {} reserved blob(s) re-stored", moved);
    Ok(())
}

// ------------------------------------------------------------------- verify

/// Read-only [`patlog::Arena`] over a slice of a flash dump.
struct Slice<'a>(&'a [u8]);

impl patlog::Arena for Slice<'_> {
    fn len(&self) -> u32 {
        self.0.len() as u32
    }
    fn view(&mut self, off: u32, want: usize) -> Option<&[u8]> {
        let at = off as usize;
        if at >= self.0.len() {
            return None;
        }
        Some(&self.0[at..(at + want).min(self.0.len())])
    }
}

struct Report {
    ok: Vec<String>,
    bad: Vec<String>,
}

impl Report {
    fn require(&mut self, cond: bool, what: String, detail: impl FnOnce() -> String) {
        if cond {
            self.ok.push(what);
        } else {
            self.bad.push(format!("{}\n      {}", what, detail()));
        }
    }
}

fn cmd_verify(args: &[String]) -> Result<(), String> {
    let flash_path = need(args, "--flash")?;
    let sidecar_path = need(args, "--sidecar")?;
    let at = parse_int(need(args, "--at")?)?;
    let len = parse_int(need(args, "--len")?)?;
    let allow_dead = flag(args, "--allow-dead");
    let label = arg(args, "--label").unwrap_or("store");

    let flash = std::fs::read(flash_path).map_err(|e| format!("{}: {}", flash_path, e))?;
    let text = std::fs::read_to_string(sidecar_path).map_err(|e| format!("{}: {}", sidecar_path, e))?;
    let side = json::parse(&text)?;

    let end = (at + len) as usize;
    if end > flash.len() {
        return Err(format!(
            "partition {:#x}+{:#x} runs past the {} B flash image",
            at, len, flash.len()
        ));
    }
    let part = &flash[at as usize..end];

    let key_len = side.u32("key_area_len")?;
    let log_at = side.u32("log_at")?;
    if len < log_at {
        return Err(format!("partition is {:#x} B, shorter than log_at {:#x}", len, log_at));
    }
    let log_len = len - log_at;

    let mut r = Report { ok: Vec::new(), bad: Vec::new() };

    // ---- the key area ----
    if key_len != keyarea::KEY_AREA_LEN {
        return Err(format!(
            "sidecar key_area_len {:#x} != this build's {:#x}",
            key_len,
            keyarea::KEY_AREA_LEN
        ));
    }
    let mut key = keyarea::MemFlash::from_bytes(&part[..key_len as usize]);
    let lo = side.u32("reserved_lo")?;
    let hi = side.u32("reserved_hi")?;
    let mut want_blobs: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    for b in side.arr("blobs")? {
        want_blobs.insert(b.u32("key")?, unhex(b.str("hex")?)?);
    }
    let mut found: BTreeSet<u32> = BTreeSet::new();
    for k in lo..=hi {
        let Some(got) = keyarea::read_blob(&mut key, k) else { continue };
        found.insert(k);
        match want_blobs.get(&k) {
            Some(want) => r.require(
                &got == want,
                format!("{}: reserved blob {:#010x} ({} B) byte-identical", label, k, want.len()),
                || format!("want {}\n      got  {}", hex(want), hex(&got)),
            ),
            None => r.bad.push(format!(
                "{}: UNEXPECTED reserved blob {:#010x} ({} B) — not in the sidecar\n      {}",
                label,
                k,
                got.len(),
                hex(&got)
            )),
        }
    }
    let missing: Vec<String> = want_blobs
        .keys()
        .filter(|k| !found.contains(k))
        .map(|k| format!("{:#010x}", k))
        .collect();
    r.require(
        missing.is_empty(),
        format!("{}: all {} reserved blob(s) present", label, want_blobs.len()),
        || format!("missing: {}", missing.join(", ")),
    );

    // ---- the pattern log ----
    let mut arena = Slice(&part[log_at as usize..(log_at + log_len) as usize]);
    let mut seen: Vec<(Rec, String)> = Vec::new();
    let scan = patlog::scan(&mut arena, &mut |rec: &Rec, name: &[u8]| {
        seen.push((*rec, String::from_utf8_lossy(name).into_owned()))
    });

    r.require(
        scan.torn == 0 && scan.resync == 0,
        format!("{}: log scans clean (0 torn, 0 resyncs)", label),
        || format!("torn={} resync={}", scan.torn, scan.resync),
    );
    if !allow_dead {
        r.require(
            scan.dead == 0,
            format!("{}: no dead records — the log was repacked, not copied", label),
            || {
                let dead: Vec<String> = seen
                    .iter()
                    .filter(|(rec, _)| rec.dead)
                    .map(|(rec, n)| format!("{:?}@{:#x}", n, rec.off))
                    .collect();
                format!("{} B dead in {}", scan.dead, dead.join(", "))
            },
        );
    }

    // Latest generation per seq, exactly as `patterns::reload` picks it.
    let mut best: BTreeMap<u32, (Rec, String)> = BTreeMap::new();
    for (rec, name) in &seen {
        if rec.dead {
            continue;
        }
        match best.get(&rec.seq) {
            Some((b, _)) if b.stamp >= rec.stamp => {}
            _ => {
                best.insert(rec.seq, (*rec, name.clone()));
            }
        }
    }

    let want_pats = side.arr("patterns")?;
    r.require(
        best.len() == want_pats.len(),
        format!("{}: {} live pattern(s) in the log", label, want_pats.len()),
        || {
            let names: Vec<&str> = best.values().map(|(_, n)| n.as_str()).collect();
            format!("found {}: {}", best.len(), names.join(", "))
        },
    );

    let log_bytes = &part[log_at as usize..(log_at + log_len) as usize];
    for w in want_pats {
        let seq = w.u32("seq")?;
        let name = w.str("name")?;
        let src_len = w.u32("src_len")?;
        let bc_len = w.u32("bc_len")?;
        let src_fnv = w.u32("src_fnv")?;
        let bc_fnv = w.u32("bc_fnv")?;
        let Some((rec, got_name)) = best.get(&seq) else {
            r.bad.push(format!("{}: pattern {:?} (seq {}) is GONE", label, name, seq));
            continue;
        };
        // Names live in flash, never in the header.
        r.require(
            got_name == name,
            format!("{}: seq {} is named {:?}", label, seq, name),
            || format!("got {:?}", got_name),
        );
        r.require(
            rec.src_len == src_len && rec.bc_len == bc_len,
            format!("{}: {:?} lengths {} B source + {} B bytecode", label, name, src_len, bc_len),
            || format!("got {} + {}", rec.src_len, rec.bc_len),
        );
        // Re-hash the bytes as they sit in the post-run flash image: the
        // record's own hash fields prove self-consistency, the sidecar's
        // prove the bytes are the ones that went in.
        let read = |off: u32, n: u32| -> Option<&[u8]> {
            let (a, b) = (off as usize, (off + n) as usize);
            log_bytes.get(a..b)
        };
        match (read(rec.src_off(), rec.src_len), read(rec.bc_off(), rec.bc_len)) {
            (Some(src), Some(bc)) => {
                let (hs, hb) = (patlog::fnv1a(src), patlog::fnv1a(bc));
                r.require(
                    hs == src_fnv && hb == bc_fnv,
                    format!("{}: {:?} source + bytecode hash to the pre-migration bytes", label, name),
                    || {
                        format!(
                            "src {:#010x} want {:#010x}; bc {:#010x} want {:#010x}",
                            hs, src_fnv, hb, bc_fnv
                        )
                    },
                );
                r.require(
                    hs == rec.src_hash && hb == rec.bc_hash,
                    format!("{}: {:?} payload matches its own record header", label, name),
                    || {
                        format!(
                            "src {:#010x} hdr {:#010x}; bc {:#010x} hdr {:#010x}",
                            hs, rec.src_hash, hb, rec.bc_hash
                        )
                    },
                );
            }
            _ => r.bad.push(format!(
                "{}: {:?} payload at {:#x}/{:#x} runs past the {:#x}-byte log",
                label,
                name,
                rec.src_off(),
                rec.bc_off(),
                log_len
            )),
        }
    }

    println!("storegen verify [{}] {:#x}+{:#x} of {}", label, at, len, flash_path);
    println!(
        "  log: {} record(s), {} B live, {} B dead, cursor {} of {} B",
        scan.recs, scan.live, scan.dead, scan.cursor, log_len
    );
    for o in &r.ok {
        println!("  ok  {}", o);
    }
    if !r.bad.is_empty() {
        for b in &r.bad {
            println!("  FAIL {}", b);
        }
        return Err(format!("{} of {} store checks failed", r.bad.len(), r.ok.len() + r.bad.len()));
    }
    println!("  {} store checks passed", r.ok.len());
    Ok(())
}

// --------------------------------------------------------------------- main

const USAGE: &str = "\
storegen — build and check a Luxel `storage` partition image (Gitea #501)

  storegen gen     --out <image> --sidecar <json> [--overfill] [--store-len N]
  storegen migrate --pre <image> --sidecar <json> --new-len <len> --out <image>
                   [--pre-len <len>]
  storegen verify  --flash <image> --sidecar <json> --at <off> --len <len>
                   [--allow-dead] [--label <name>]

`gen` writes a pre-#501 1 MiB storage partition: the real sequential-storage
key area with the reserved blobs, and a real patlog file log with a dozen
patterns, a delete and a re-save.  `migrate` runs the store move on the host
and writes the partition image a device WOULD end up with, for a layout the
emulator cannot boot.  `verify` parses one back out of a flash dump and checks
it against the sidecar.  Offsets accept 0x hex.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(|s| s.as_str()) {
        Some("gen") => cmd_gen(&args),
        Some("migrate") => cmd_migrate(&args),
        Some("verify") => cmd_verify(&args),
        Some("-h") | Some("--help") | None => {
            print!("{}", USAGE);
            return;
        }
        Some(other) => Err(format!("unknown subcommand {:?}\n\n{}", other, USAGE)),
    };
    if let Err(e) = r {
        eprintln!("storegen: {}", e);
        std::process::exit(1);
    }
}
