//! Host build of `firmware/src/parttab/raw.rs` (GPL-3.0-or-later, like the
//! firmware it comes from) — the partition-table codec.
//!
//! `raw.rs` is deliberately `no_std` + `alloc` and touches no flash and no
//! esp-hal, purely so this crate can compile the very same file for the
//! host. It carries no `extern crate alloc` of its own; that lives here, so
//! the firmware (which already has one) and this crate can both include it
//! unchanged.
//!
//! What the tests below protect is the arithmetic the repartition (Gitea
//! #501) bets a device on. A misread table is not a failed test on metal —
//! it is a migration that erases the wrong region, or a WLED device whose
//! credentials get skipped, or a table written past the end of the die. All
//! three are serial-recovery bricks, and nothing on the bench has a serial
//! path right now. So every table under test is serialized at test time
//! from the REAL `firmware/partitions*.csv` by `esp-idf-part` — the crate
//! espflash and `firmware/build.rs` use — rather than transcribed by hand,
//! which is the only way a test can still be right after somebody edits a
//! CSV.

extern crate alloc;

#[path = "../../../firmware/src/parttab/raw.rs"]
pub mod raw;

/// The app-image checks `/api/ota` runs before it points `otadata` at a
/// freshly written slot (Gitea #655) — pure for the same reason `raw.rs`
/// is, and compiled here for the same reason.
#[path = "../../../firmware/src/appimg.rs"]
pub mod appimg;

#[cfg(test)]
mod tests {
    use super::raw::*;
    use esp_idf_part::PartitionTable;

    /// The store's own geometry, from `firmware/src/patterns.rs`: the key
    /// area is the first `STORE_LEN` of the `storage` partition and the
    /// packed pattern log starts at `LOG_OFF` and runs to the end of it.
    /// Both are layout-independent — only the log's *length* changes — and
    /// the migrator leans on exactly that.
    const KEY_AREA_LEN: u32 = 0x2_0000;
    const LOG_OFF: u32 = 0x4_9000;
    /// `parttab::SECTOR`: the staging header the migrator writes ahead of
    /// the repacked log image.
    const SECTOR: u32 = 0x1000;

    /// The layout a device flashed before #501 is running right now. This
    /// is the "old" side of every migration sum, so it is spelled out here
    /// rather than read from the tree: the point is that it is *frozen*
    /// history, and an edit to `partitions.csv` must not be able to change
    /// what we believe is on a device in the field.
    const PRE_501_CSV: &str = "\
nvs,      data, nvs,     0x9000,   0x4000,
otadata,  data, ota,     0xd000,   0x2000,
phy_init, data, phy,     0xf000,   0x1000,
ota_0,    app,  ota_0,   0x10000,  0x100000,
ota_1,    app,  ota_1,   0x110000, 0x100000,
storage,  data, spiffs,  0x210000, 0x100000,
assets,   data, spiffs,  0x310000, 0x0F0000,
";

    /// WLED's stock 4 MB layout — the other thing a Luxel image can find
    /// itself booting on top of (the takeover path, docs/wled-migration.md).
    const WLED_CSV: &str = "\
nvs,      data, nvs,     0x9000,   0x5000,
otadata,  data, ota,     0xe000,   0x2000,
app0,     app,  ota_0,   0x10000,  0x180000,
app1,     app,  ota_1,   0x190000, 0x180000,
spiffs,   data, spiffs,  0x310000, 0x0F0000,
";

    fn bin(csv: &str) -> Vec<u8> {
        PartitionTable::try_from(csv.to_string())
            .expect("esp-idf-part parses the table")
            .to_bin()
            .expect("esp-idf-part serializes the table")
    }

    fn four_mb() -> Vec<u8> {
        bin(include_str!("../../../firmware/partitions.csv"))
    }

    fn sixteen_mb() -> Vec<u8> {
        bin(include_str!("../../../firmware/partitions-16mb.csv"))
    }

    fn pre_501() -> Vec<u8> {
        bin(PRE_501_CSV)
    }

    fn wled() -> Vec<u8> {
        bin(WLED_CSV)
    }

    /// Every real table this repo ships, by the name the failure message
    /// should use.
    fn real_tables() -> Vec<(&'static str, Vec<u8>)> {
        vec![("partitions.csv", four_mb()), ("partitions-16mb.csv", sixteen_mb())]
    }

    fn label(p: &Part) -> String {
        let n = p.label.iter().position(|&c| c == 0).unwrap_or(16);
        String::from_utf8_lossy(&p.label[..n]).into_owned()
    }

    fn overlap(a: &Part, b: &Part) -> bool {
        a.offset < b.end() && b.offset < a.end()
    }

    fn named(table: &[u8], name: &str) -> Part {
        entries(table)
            .into_iter()
            .find(|p| p.labelled(name))
            .unwrap_or_else(|| panic!("no partition labelled {name:?}"))
    }

    // ---- the CSV side, parsed independently of esp-idf-part -------------

    struct Row {
        name: String,
        ptype: u8,
        subtype: u8,
        offset: u32,
        size: u32,
    }

    fn hex(s: &str) -> u32 {
        let s = s.trim();
        u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
            .unwrap_or_else(|e| panic!("bad number {s:?}: {e}"))
    }

    /// The CSV's own rows, read with a dumb split — deliberately NOT via
    /// `esp-idf-part`, so the round-trip test compares two independent
    /// readings of the file rather than one library against itself.
    fn rows(csv: &str) -> Vec<Row> {
        csv.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| {
                let f: Vec<&str> = l.split(',').map(str::trim).collect();
                assert!(f.len() >= 5, "malformed csv row {l:?}");
                let ptype = match f[1] {
                    "app" => TYPE_APP,
                    "data" => TYPE_DATA,
                    other => panic!("unknown partition type {other:?}"),
                };
                // ESP-IDF's subtype numbering, the half of it these tables use.
                let subtype = match (ptype, f[2]) {
                    (TYPE_APP, "factory") => 0x00,
                    (TYPE_APP, "ota_0") => SUBTYPE_OTA0,
                    (TYPE_APP, "ota_1") => SUBTYPE_OTA1,
                    (TYPE_DATA, "ota") => 0x00,
                    (TYPE_DATA, "phy") => 0x01,
                    (TYPE_DATA, "nvs") => 0x02,
                    (TYPE_DATA, "fat") => 0x81,
                    (TYPE_DATA, "spiffs") => 0x82,
                    (_, other) => panic!("unknown subtype {other:?}"),
                };
                Row { name: f[0].to_string(), ptype, subtype, offset: hex(f[3]), size: hex(f[4]) }
            })
            .collect()
    }

    // ------------------------------------------------------------------ 1

    /// Decoding a real table must give back exactly the CSV that produced
    /// it — every row, in order, with the right type, subtype, offset, size
    /// and label — and the walk must STOP at the MD5 row.
    ///
    /// Catches: a wrong field offset or endianness in `entries()` (which
    /// would hand the migrator plausible-looking garbage), and a terminator
    /// check that misses `0xEBEB` — that one would let the MD5 row and then
    /// erased flash decode as extra "partitions", so `flash_needed()` could
    /// claim the table runs past the die and `data_labelled` could match a
    /// label out of an MD5 digest.
    #[test]
    fn entries_round_trip_the_real_csvs() {
        for (name, table) in real_tables() {
            let want = rows(match name {
                "partitions.csv" => include_str!("../../../firmware/partitions.csv"),
                _ => include_str!("../../../firmware/partitions-16mb.csv"),
            });
            let got = entries(&table);
            assert_eq!(got.len(), want.len(), "{name}: entry count (did the MD5 row stop the walk?)");
            for (g, w) in got.iter().zip(&want) {
                assert_eq!(label(g), w.name, "{name}: label");
                assert_eq!(g.ptype, w.ptype, "{name}: {} type", w.name);
                assert_eq!(g.subtype, w.subtype, "{name}: {} subtype", w.name);
                assert_eq!(g.offset, w.offset, "{name}: {} offset", w.name);
                assert_eq!(g.len, w.size, "{name}: {} size", w.name);
                assert_eq!(g.end(), w.offset + w.size, "{name}: {} end()", w.name);
            }
            // The bytes really do carry the terminator we are relying on.
            assert!(
                table.windows(2).any(|w| w == [0xEB, 0xEB]),
                "{name}: serialized table has no MD5 row — this test proves nothing"
            );
        }
    }

    // ------------------------------------------------------------------ 2

    /// The lookups the firmware actually performs must find the right
    /// entries in both real tables, and `labelled()` must be an EXACT
    /// compare, not a prefix one.
    ///
    /// Catches: a prefix match. `"stor"` matching `storage` would be a
    /// harmless-looking bug today and a data-loss one the day somebody adds
    /// a `storage2` partition — the store, and worse the migrator's erase,
    /// would silently bind to whichever entry came first in the table.
    #[test]
    fn labels_are_looked_up_exactly() {
        for (name, table) in real_tables() {
            for want in ["storage", "assets", "otadata"] {
                let p = data_labelled(&table, want)
                    .unwrap_or_else(|| panic!("{name}: no data partition labelled {want:?}"));
                assert_eq!(label(&p), want, "{name}: data_labelled returned the wrong entry");
                assert_eq!(p.ptype, TYPE_DATA, "{name}: {want} is not a data partition");
            }
            let a = app_slot(&table, SUBTYPE_OTA0).expect("ota_0");
            let b = app_slot(&table, SUBTYPE_OTA1).expect("ota_1");
            assert_eq!(label(&a), "ota_0", "{name}: ota_0 slot");
            assert_eq!(label(&b), "ota_1", "{name}: ota_1 slot");
            assert_eq!(a.ptype, TYPE_APP, "{name}: ota_0 is not an app partition");
            assert_eq!(b.ptype, TYPE_APP, "{name}: ota_1 is not an app partition");
            assert_eq!(app_entries(&table).len(), 2, "{name}: exactly two app slots, no factory");

            // A prefix of a real label, and a real label with a suffix.
            assert!(data_labelled(&table, "stor").is_none(), "{name}: \"stor\" must not match storage");
            assert!(
                data_labelled(&table, "storage2").is_none(),
                "{name}: a future storage2 partition must not be found as storage"
            );
            assert!(data_labelled(&table, "asset").is_none(), "{name}: \"asset\" must not match assets");
            assert!(data_labelled(&table, "").is_none(), "{name}: the empty label matches nothing");
            // An app partition is not reachable through the data lookup.
            assert!(data_labelled(&table, "ota_0").is_none(), "{name}: ota_0 is an app partition");
        }
    }

    // ------------------------------------------------------------------ 3

    /// `is_luxel` is the discriminator that routes a boot between "migrate
    /// my own older layout" and "take over somebody else's flash". It must
    /// be true for every layout this project has ever shipped and false for
    /// WLED's.
    ///
    /// Catches: the expensive direction. A WLED table misread as ours skips
    /// the takeover entirely — no credential inheritance, so the device
    /// comes up with no WiFi on a board whose only recovery is serial. The
    /// other direction (ours misread as foreign) re-runs a takeover over a
    /// Luxel store.
    #[test]
    fn is_luxel_separates_our_layouts_from_wleds() {
        assert!(is_luxel(&four_mb()), "the 4 MB table is ours");
        assert!(is_luxel(&sixteen_mb()), "the 16 MB table is ours");
        assert!(is_luxel(&pre_501()), "the pre-#501 table is ours — that is the whole migration");
        assert!(!is_luxel(&wled()), "WLED's table must route to the takeover, not the migrator");

        // And for the right reason: WLED has an app pair and a data
        // partition of the same size at the same offset as our `assets`.
        // Only the labels tell them apart.
        let w = wled();
        assert_eq!(app_entries(&w).len(), 2, "WLED has two app slots too");
        assert!(data_labelled(&w, "spiffs").is_some(), "WLED's data partition is labelled spiffs");
        assert!(data_labelled(&w, "storage").is_none());
        assert!(data_labelled(&w, "assets").is_none());
    }

    // ------------------------------------------------------------------ 4

    /// Alignment, overlap and flash-size invariants, asserted over the
    /// PARSED table rather than the CSV text — so they hold for the bytes
    /// the bootloader reads, not for a comment somebody kept up to date.
    ///
    /// Catches: a CSV edit that breaks a hardware requirement. App slots
    /// must be 64 KiB-aligned or the second-stage bootloader will not map
    /// them; `storage`/`assets` must be 64 KiB-aligned in BOTH offset and
    /// length because `flashmap.rs` maps them through the cache MMU, which
    /// only takes aligned offsets and whole pages; two partitions that
    /// overlap mean one region silently eats another's writes.
    #[test]
    fn the_real_tables_obey_the_hardware_alignment_rules() {
        const MMU: u32 = 0x1_0000;
        for (name, table) in real_tables() {
            let ps = entries(&table);
            for p in &ps {
                assert_eq!(p.offset % 0x1000, 0, "{name}: {} offset is not sector aligned", label(p));
                assert!(p.len > 0, "{name}: {} is empty", label(p));
                if p.ptype == TYPE_APP {
                    assert_eq!(p.offset % MMU, 0, "{name}: app slot {} must be 64 KiB aligned", label(p));
                }
                if p.labelled("storage") || p.labelled("assets") {
                    assert_eq!(p.offset % MMU, 0, "{name}: {} offset must be MMU-page aligned", label(p));
                    assert_eq!(p.len % MMU, 0, "{name}: {} length must be whole MMU pages", label(p));
                }
            }
            for (i, a) in ps.iter().enumerate() {
                for b in &ps[i + 1..] {
                    assert!(
                        !overlap(a, b),
                        "{name}: {} [{:#x}..{:#x}) overlaps {} [{:#x}..{:#x})",
                        label(a),
                        a.offset,
                        a.end(),
                        label(b),
                        b.offset,
                        b.end()
                    );
                }
            }
        }

        // The 4 MB table fills its part exactly. The 16 MB one deliberately
        // stops at 0xE00000 and leaves the top 2 MiB unallocated (see the
        // CSV header and the `expect_end` constant in firmware/build.rs) —
        // so what must hold is "fits on a 16 MB die", not "ends at one".
        assert_eq!(flash_needed(&four_mb()), 0x40_0000, "the 4 MB table must fill a 4 MB part exactly");
        assert_eq!(
            flash_needed(&sixteen_mb()),
            0xE0_0000,
            "the 16 MB table's declared end — keep this in step with firmware/build.rs"
        );
        assert!(flash_needed(&sixteen_mb()) <= 0x100_0000, "the 16 MB table must fit a 16 MB part");
        // And the pre-#501 layout, which the migrator sizes its staging
        // against, really did fill a 4 MB part.
        assert_eq!(flash_needed(&pre_501()), 0x40_0000);
    }

    // ------------------------------------------------------------------ 5

    /// The preconditions `firmware/src/migrate.rs` depends on but cannot
    /// check on a device without already being mid-migration. Each one is a
    /// thing a future CSV edit could break in complete silence.
    #[test]
    fn the_4mb_layout_is_migratable_from_the_pre_501_one() {
        let old = pre_501();
        let new = four_mb();

        // (a) `assets` does not move. This is what lets a 4 MB migration
        // leave the web bundle exactly where src/assets.rs already maps it,
        // so the playground survives the repartition instead of needing a
        // re-upload over a network the device may not have.
        let oa = named(&old, "assets");
        let na = named(&new, "assets");
        assert_eq!(na.offset, oa.offset, "the 4 MB assets partition moved — the bundle would be lost");
        assert_eq!(na.len, oa.len, "the 4 MB assets partition resized — the bundle would be truncated");

        // (b) The new `storage` must not overlap the OLD key area. The
        // migrator erases the new region and only afterwards reads the old
        // reserved blobs (playlist, pixel map, palette, device name, …) out
        // of the old key area, so an overlap there erases them first.
        let os = named(&old, "storage");
        let ns = named(&new, "storage");
        let old_keys = Part { len: KEY_AREA_LEN, ..os };
        assert!(
            !overlap(&ns, &old_keys),
            "new storage [{:#x}..{:#x}) overlaps the old key area [{:#x}..{:#x}) — \
             the migrator would erase the blobs it has not read yet",
            ns.offset,
            ns.end(),
            old_keys.offset,
            old_keys.end()
        );

        // (c) The new `storage` has to hold the store's fixed head (key
        // area, current-pattern scratch) and still leave a log worth having.
        assert!(ns.len > LOG_OFF, "new storage {:#x} is smaller than LOG_OFF {LOG_OFF:#x}", ns.len);
        let new_log = ns.len - LOG_OFF;
        assert!(
            new_log >= 0x3_0000,
            "the new 4 MB log is only {new_log:#x} B — too small to carry a device's library across"
        );
        assert_eq!(new_log, 0x3_7000, "the migration tests in tools/patlog-check size against this");
        let old_log = os.len - LOG_OFF;
        assert!(old_log > new_log, "the 4 MB log is supposed to SHRINK; this test is stale otherwise");

        // (d) Staging. The repacked image is built into the OLD ota_1 (free
        // scratch — the device is single-image from here to its next OTA),
        // behind one sector of staging header. If that does not fit, the
        // migration can never run on any device, blocked or not.
        let staging = app_slot(&old, SUBTYPE_OTA1).expect("the old ota_1 is the staging area");
        assert!(
            staging.len >= SECTOR + new_log,
            "old ota_1 is {:#x} B, staging needs {:#x} B",
            staging.len,
            SECTOR + new_log
        );
    }

    /// The 16 MB layout moves everything, so what it needs is the opposite
    /// property: no old region may sit under a new one at all. The migrator
    /// reads old `storage` and old `assets` after writing the new ones, so
    /// any overlap is a read of bytes it has already destroyed.
    #[test]
    fn the_16mb_layout_relocates_every_region_clear_of_the_old_one() {
        let old = pre_501();
        let new = sixteen_mb();

        let (os, ns) = (named(&old, "storage"), named(&new, "storage"));
        assert!(
            !overlap(&os, &ns),
            "16 MB: new storage [{:#x}..{:#x}) overlaps the old one [{:#x}..{:#x})",
            ns.offset,
            ns.end(),
            os.offset,
            os.end()
        );
        let (oa, na) = (named(&old, "assets"), named(&new, "assets"));
        assert!(
            !overlap(&oa, &na),
            "16 MB: new assets [{:#x}..{:#x}) overlaps the old one [{:#x}..{:#x})",
            na.offset,
            na.end(),
            oa.offset,
            oa.end()
        );
        assert!(na.len >= oa.len, "16 MB: the new assets partition must hold the old bundle");
        assert!(ns.len > LOG_OFF, "16 MB: storage must hold the store's head");
        assert!(ns.len - LOG_OFF > os.len - LOG_OFF, "16 MB: the log is supposed to GROW");

        // Staging is the old ota_1 on this board too, and the log it has to
        // hold is the one the NEW table offers — much bigger here. The
        // migrator's own `SECTOR + bytes > staging.len` guard is what
        // catches a device whose library exceeds it; what must hold
        // unconditionally is that the *4 MB* worth this test cares about
        // fits, since a 16 MB device stages only what it actually stores.
        let staging = app_slot(&old, SUBTYPE_OTA1).expect("old ota_1");
        assert!(staging.len >= SECTOR + (named(&four_mb(), "storage").len - LOG_OFF));
    }
}

/// `/api/ota`'s two pure decisions (Gitea #655): which slot an update goes
/// into, and whether what landed there is an image the bootloader will
/// boot. Both were the Seengreat brick of 2026-09-21 — the update picked
/// the running slot, and the bootloader asserted on the torn image it left.
#[cfg(test)]
mod ota_tests {
    use super::appimg;
    use super::raw::*;
    use esp_idf_part::PartitionTable;

    const PRE_501_CSV: &str = "\
nvs,      data, nvs,     0x9000,   0x4000,
otadata,  data, ota,     0xd000,   0x2000,
phy_init, data, phy,     0xf000,   0x1000,
ota_0,    app,  ota_0,   0x10000,  0x100000,
ota_1,    app,  ota_1,   0x110000, 0x100000,
storage,  data, spiffs,  0x210000, 0x100000,
assets,   data, spiffs,  0x310000, 0x0F0000,
";
    /// A single-slot table: the only image IS the running one.
    const ONE_SLOT_CSV: &str = "\
nvs,      data, nvs,     0x9000,   0x4000,
otadata,  data, ota,     0xd000,   0x2000,
ota_0,    app,  ota_0,   0x10000,  0x100000,
storage,  data, spiffs,  0x110000, 0x100000,
";
    /// No otadata at all — a factory-only layout the bootloader would boot
    /// but nothing could ever activate an update on.
    const NO_OTADATA_CSV: &str = "\
nvs,      data, nvs,     0x9000,   0x4000,
ota_0,    app,  ota_0,   0x10000,  0x100000,
ota_1,    app,  ota_1,   0x110000, 0x100000,
";

    fn bin(csv: &str) -> Vec<u8> {
        PartitionTable::try_from(csv.to_string()).unwrap().to_bin().unwrap()
    }

    /// The pre-#501, 4 MB and 16 MB tables — every layout a device can be
    /// running when it takes an update.
    fn every_table() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("pre-501", bin(PRE_501_CSV)),
            ("partitions.csv", bin(include_str!("../../../firmware/partitions.csv"))),
            ("partitions-16mb.csv", bin(include_str!("../../../firmware/partitions-16mb.csv"))),
        ]
    }

    // ---- slot selection --------------------------------------------------

    /// From either slot, on every layout, the target is the OTHER slot —
    /// and never the running one. otadata is not an input at all, which is
    /// the whole point: this is the erased-otadata state the panel was in.
    #[test]
    fn target_is_always_the_other_slot() {
        for (name, t) in every_table() {
            let ota0 = app_slot(&t, SUBTYPE_OTA0).unwrap();
            let ota1 = app_slot(&t, SUBTYPE_OTA1).unwrap();
            // a running image that fills its slot entirely
            let from0 = ota_target(&t, ota0.offset, ota0.len).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(from0.offset, ota1.offset, "{name}: running ota_0 must update ota_1");
            assert_eq!(from0.subtype, SUBTYPE_OTA1);
            let from1 = ota_target(&t, ota1.offset, ota1.len).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(from1.offset, ota0.offset, "{name}: running ota_1 must update ota_0");
            assert_eq!(from1.subtype, SUBTYPE_OTA0);
        }
    }

    /// The exact Seengreat state of 2026-09-21: pre-#501 table, executing
    /// from ota_0, a 985,856 B image. The library answered ota_0.
    #[test]
    fn seengreat_state_picks_ota1() {
        let t = bin(PRE_501_CSV);
        let target = ota_target(&t, 0x10000, 985_856).unwrap();
        assert_eq!((target.offset, target.len), (0x110000, 0x100000));
    }

    /// A device that cannot say where it runs from does not update.
    #[test]
    fn unknown_running_slot_refuses() {
        for (name, t) in every_table() {
            let e = ota_target(&t, u32::MAX, 0).err().unwrap();
            assert!(e.contains("cannot tell which slot is running"), "{name}: {e}");
        }
    }

    /// The running slot is the only app slot: refuse, do not eat it.
    #[test]
    fn single_slot_refuses() {
        let t = bin(ONE_SLOT_CSV);
        let e = ota_target(&t, 0x10000, 0x80000).err().unwrap();
        assert!(e.contains("no second OTA slot"), "{e}");
    }

    #[test]
    fn missing_otadata_refuses() {
        let t = bin(NO_OTADATA_CSV);
        let e = ota_target(&t, 0x10000, 0x80000).err().unwrap();
        assert!(e.contains("no otadata"), "{e}");
    }

    /// A running image that spills into the free slot (a table whose slots
    /// overlap, or a lying image length) must not be written over.
    #[test]
    fn overlap_with_running_image_refuses() {
        let t = bin(PRE_501_CSV);
        // 0x10000 + 0x100001 reaches one byte into ota_1
        let e = ota_target(&t, 0x10000, 0x100001).err().unwrap();
        assert!(e.contains("overlaps the running image"), "{e}");
        // and exactly filling the slot is fine
        assert!(ota_target(&t, 0x10000, 0x100000).is_ok());
    }

    /// The 4 MB → 16 MB two-hop board runs the 4 MB table with ota_1 at
    /// 0x150000; the 16 MB table's ota_1 is at 0x310000. Whatever is live,
    /// the answer comes from THAT table.
    #[test]
    fn target_comes_from_the_live_table() {
        let four = bin(include_str!("../../../firmware/partitions.csv"));
        let sixteen = bin(include_str!("../../../firmware/partitions-16mb.csv"));
        let a = ota_target(&four, 0x10000, 1_000_000).unwrap();
        let b = ota_target(&sixteen, 0x10000, 1_000_000).unwrap();
        assert_ne!(a.offset, b.offset);
        assert_eq!(a.offset, app_slot(&four, SUBTYPE_OTA1).unwrap().offset);
        assert_eq!(b.offset, app_slot(&sixteen, SUBTYPE_OTA1).unwrap().offset);
    }

    // ---- image verification ---------------------------------------------

    /// Build an ESP app image the way espflash does: header, segments,
    /// checksum byte padded to 16, optional 32-byte hash. Data is
    /// pseudo-random so a stale-tail test cannot pass by coincidence.
    fn image(segments: &[(u32, usize)], hash: bool, seed: u8) -> Vec<u8> {
        let mut v = vec![0u8; 24];
        v[0] = 0xE9;
        v[1] = segments.len() as u8;
        v[23] = hash as u8;
        let mut x = seed;
        for (i, (load, len)) in segments.iter().enumerate() {
            v.extend_from_slice(&load.to_le_bytes());
            v.extend_from_slice(&(*len as u32).to_le_bytes());
            let start = v.len();
            for _ in 0..*len {
                x = x.wrapping_mul(31).wrapping_add(7);
                v.push(x);
            }
            if i == 0 {
                // esp_app_desc magic at image offset 0x20
                v[start..start + 4].copy_from_slice(&[0x32, 0x54, 0xCD, 0xAB]);
            }
        }
        // the checksum, from what is actually in the buffer
        let mut sum2 = 0xEFu8;
        let mut pos = 24;
        for _ in 0..segments.len() {
            let len = u32::from_le_bytes(v[pos + 4..pos + 8].try_into().unwrap()) as usize;
            for b in &v[pos + 8..pos + 8 + len] {
                sum2 ^= *b;
            }
            pos += 8 + len;
        }
        let padded = (v.len() + 1 + 15) & !15;
        v.resize(padded, 0);
        v[padded - 1] = sum2;
        if hash {
            v.extend_from_slice(&[0x5A; 32]);
        }
        v
    }

    fn verify(img: &[u8]) -> Result<(), &'static str> {
        verify_len(img, img.len())
    }

    fn verify_len(img: &[u8], len: usize) -> Result<(), &'static str> {
        appimg::verify(len as u32, &mut |off, buf| {
            let off = off as usize;
            if off + buf.len() > img.len() {
                return false;
            }
            buf.copy_from_slice(&img[off..off + buf.len()]);
            true
        })
    }

    /// The shape of a real image: drom map, data load, iram load, irom
    /// map, more iram — five segments, hash appended, ~1 MB.
    fn realistic() -> Vec<u8> {
        image(
            &[
                (0x3F40_0020, 132_948),
                (0x3FFB_0000, 19_400),
                (0x4008_0000, 44_236),
                (0x400D_0020, 791_768),
                (0x4008_ACCC, 51_336),
            ],
            true,
            1,
        )
    }

    #[test]
    fn whole_image_verifies() {
        assert_eq!(verify(&realistic()), Ok(()));
        assert_eq!(verify(&image(&[(0x3F40_0020, 4_000), (0x4008_0000, 16)], false, 9)), Ok(()));
    }

    /// espflash's own output, when one is around (`nix build
    /// .#luxel-fw-athom-music` → `result/luxel-fw-ota.bin`): point
    /// `LUXEL_OTA_IMAGE` at it. Skipped, not failed, otherwise — CI has no
    /// firmware image on the host test path.
    #[test]
    fn real_espflash_image_verifies() {
        let Ok(path) = std::env::var("LUXEL_OTA_IMAGE") else { return };
        let img = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(appimg::header_ok(&img));
        assert_eq!(verify(&img), Ok(()), "{path}");
        // and one flipped byte in the last segment is caught
        let mut bad = img.clone();
        let i = bad.len() - 64;
        bad[i] ^= 0x01;
        assert_eq!(verify(&bad), Err("image checksum mismatch"));
    }

    /// Cut anywhere — mid-segment, mid-header, before the checksum — and it
    /// is not an image. This is the wedged upload.
    #[test]
    fn truncated_image_is_rejected() {
        let img = realistic();
        for cut in [0x23, 0x24, 100, 24 + 8 + 132_948 - 1, 24 + 8 + 132_948 + 3, img.len() / 2, img.len() - 33, img.len() - 1] {
            assert!(verify_len(&img, cut).is_err(), "cut at {cut} of {} verified", img.len());
        }
    }

    /// A stale tail: the panel's slot held the new image's head over the
    /// previous image's bytes, so a segment header was read out of
    /// somebody's `.rodata` — vaddr 0x33697053 ("Spi3"), size 0x63616d48
    /// ("Hmac"). The bootloader asserted on it and looped. Here the same
    /// bytes are a refusal.
    #[test]
    fn stale_tail_is_rejected() {
        let new = realistic();
        let old = image(
            &[(0x3F40_0020, 132_948), (0x3FFB_0000, 19_400), (0x4008_0000, 44_236), (0x400D_0020, 790_000), (0x4008_ACCC, 53_104)],
            true,
            2,
        );
        assert_eq!(new.len(), old.len(), "fixture: same total length");
        // the upload died 300 KiB in; the rest of the slot is the old image
        let mut slot = old.clone();
        slot[..300 * 1024].copy_from_slice(&new[..300 * 1024]);
        let r = verify(&slot);
        assert!(
            r == Err("corrupt segment table") || r == Err("image checksum mismatch"),
            "stale tail verified: {r:?}"
        );
        // and the panel's literal bytes as a segment header
        let mut torn = realistic();
        let seg3 = 24 + (8 + 132_948) + (8 + 19_400) + (8 + 44_236);
        torn[seg3..seg3 + 8].copy_from_slice(&[0x53, 0x70, 0x69, 0x33, 0x48, 0x6D, 0x61, 0x63]);
        assert_eq!(verify(&torn), Err("corrupt segment table"));
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(verify(&[0xFF; 4096]).is_err(), "erased flash");
        assert!(verify(&[0u8; 4096]).is_err(), "zeros");
        let mut img = realistic();
        img[0] = 0xE8;
        assert_eq!(verify(&img), Err(appimg::NOT_AN_IMAGE));
        let mut img = realistic();
        img[1] = 17;
        assert_eq!(verify(&img), Err("corrupt segment table"));
        // a segment whose load range wraps the address space — the
        // bootloader's `load_end > load_addr` assert
        let mut img = realistic();
        img[24..28].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
        assert_eq!(verify(&img), Err("corrupt segment table"));
    }

    /// Trailing bytes after the hash, or a hash flag with no hash: the
    /// length has to be exactly what the segment table implies.
    #[test]
    fn length_must_match_segment_table() {
        let mut img = realistic();
        img.extend_from_slice(&[0; 16]);
        assert_eq!(verify(&img), Err("image length does not match its segment table"));
        let img = image(&[(0x3F40_0020, 4_000)], false, 3);
        let mut flagged = img.clone();
        flagged[23] = 1;
        assert_eq!(verify(&flagged), Err("image length does not match its segment table"));
    }

    #[test]
    fn checksum_byte_is_checked() {
        let mut img = realistic();
        let n = img.len();
        img[n - 33] ^= 0x10; // the checksum byte sits just before the 32-byte hash
        assert_eq!(verify(&img), Err("image checksum mismatch"));
    }

    /// A read failure anywhere is a refusal, never a pass.
    #[test]
    fn unreadable_flash_is_rejected() {
        let img = realistic();
        let mut calls = 0;
        let r = appimg::verify(img.len() as u32, &mut |off, buf| {
            calls += 1;
            if calls == 5 {
                return false;
            }
            let off = off as usize;
            buf.copy_from_slice(&img[off..off + buf.len()]);
            true
        });
        assert_eq!(r, Err("image unreadable after write"));
    }

    #[test]
    fn header_ok_matches_the_write_gate() {
        let img = realistic();
        assert!(appimg::header_ok(&img[..0x24]));
        assert!(!appimg::header_ok(&img[..0x23]));
        let mut no_desc = img[..0x24].to_vec();
        no_desc[0x20] = 0;
        assert!(!appimg::header_ok(&no_desc));
    }
}
