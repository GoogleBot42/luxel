//! Gitea #379: a compaction must be a **total function over the live set**.
//!
//! Every test here drives the replica of `patterns.rs` in `super`, not
//! `patlog` directly — the bug lived in what the caller hands `plan()` and
//! what it does afterwards, which is exactly what `patlog`'s own suite
//! cannot see.

use super::*;

/// Library-ish patterns: a few hundred bytes of name and a few KB each, so
/// a full log holds ~100 of them, like the real `library/`.
fn lib(n: usize) -> Vec<Pat> {
    (0..n)
        .map(|i| {
            let src = 600 + (i * 733) % 5200;
            let bc = 500 + (i * 449) % 4400;
            Pat::new(&format!("pattern-{:03}", i), src, bc)
        })
        .collect()
}

/// Assert that every pattern in `want` is on flash, byte for byte, and that
/// nothing else is.
#[track_caller]
fn assert_exactly(s: &mut Store, want: &[Pat], what: &str) {
    let got = s.on_flash();
    let mut missing: Vec<&str> = Vec::new();
    for p in want {
        match got.iter().find(|(n, _, _)| n == &p.name) {
            Some((_, src, bc)) => {
                assert_eq!(src, &p.src, "{}: {} has foreign source", what, p.name);
                assert_eq!(bc, &p.bc, "{}: {} has foreign bytecode", what, p.name);
            }
            None => missing.push(&p.name),
        }
    }
    assert!(missing.is_empty(), "{}: {} of {} live files lost: {:?}", what, missing.len(), want.len(), missing);
    assert_eq!(got.len(), want.len(), "{}: extra files on flash", what);
}

/// Fill the store, delete every third file, then keep saving until the log
/// refuses one. Every accepted save must still be readable at the end.
///
/// This is Gitea #379 reproduction 1 (fill from `library/`) in miniature,
/// with `pinned` empty — the device's discriminating run, which lost a file
/// with nothing pinned at all.
fn fill_churn_and_check(pin: Option<usize>) -> (usize, usize) {
    let mut s = Store::new();
    let pats = lib(400);
    let mut live: Vec<Pat> = Vec::new();

    // phase 1: fill
    for p in &pats {
        if s.save(p).is_err() {
            break;
        }
        live.push(p.clone());
    }
    assert!(live.len() > 60, "only {} patterns fit — the log is too small to churn", live.len());
    assert_exactly(&mut s, &live, "after the first fill");

    // pin one of the files an engine could be executing
    if let Some(k) = pin {
        let i = k.min(live.len() - 1);
        let seq = s.rec_by_name(&live[i].name).expect("pinned file is indexed").seq;
        s.pinned = vec![seq];
    }

    // phase 2: delete every third, then keep saving — the deletes make room
    // only a compaction can hand back.
    let doomed: Vec<String> = live.iter().step_by(3).map(|p| p.name.clone()).collect();
    for n in &doomed {
        if s.pinned.is_empty() || s.rec_by_name(n).map(|r| !s.pinned.contains(&r.seq)) == Some(true) {
            assert!(s.delete(n), "delete {}", n);
            live.retain(|p| &p.name != n);
        }
    }
    assert_exactly(&mut s, &live, "after the deletes");

    // Fresh names, so every save is an append the compaction has to make
    // room for rather than an upsert.
    let mut compacted = 0;
    let mut refused = 0;
    for i in 0..80 {
        let p = Pat::new(&format!("extra-{:03}", i), 700 + (i * 617) % 5000, 600 + (i * 331) % 4000);
        let before = s.compactions;
        if s.save(&p).is_err() {
            refused += 1;
            break;
        }
        live.push(p.clone());
        if s.compactions > before {
            compacted += 1;
        }
        assert_exactly(&mut s, &live, &format!("after saving {}", p.name));
    }
    println!(
        "pin {:?}: {} patterns live, {} compactions, {} refused saves, dead {} B",
        pin,
        live.len(),
        s.compactions,
        refused,
        s.dead_bytes
    );
    (compacted, refused)
}

#[test]
fn a_compaction_with_nothing_pinned_keeps_every_live_file() {
    let (compacted, _) = fill_churn_and_check(None);
    assert!(compacted > 0, "the run never compacted — the test proves nothing");
}

#[test]
fn a_compaction_with_the_lowest_file_pinned_keeps_every_live_file() {
    let (compacted, _) = fill_churn_and_check(Some(0));
    assert!(compacted > 0, "the run never compacted — the test proves nothing");
}

#[test]
fn a_compaction_with_a_middle_file_pinned_keeps_every_live_file() {
    let (compacted, _) = fill_churn_and_check(Some(30));
    assert!(compacted > 0, "the run never compacted — the test proves nothing");
}

/// Pinning the HIGHEST file makes the tail unreclaimable: a pinned record
/// never moves, so the packed length can never fall below its end. The
/// store must then refuse the save — with every file still intact — rather
/// than repack around it and drop something.
#[test]
fn a_compaction_blocked_by_a_pinned_tail_refuses_instead_of_losing_files() {
    let (compacted, refused) = fill_churn_and_check(Some(usize::MAX));
    assert_eq!(compacted, 0, "a pinned tail leaves nothing to reclaim");
    assert!(refused > 0, "the save should have failed loudly");
}

/// Gitea #379 reproduction 3, exactly: fill, delete **everything**, then
/// seed a fresh set. The first save compacts an all-dead log; every later
/// one appends. All of them must be there.
#[test]
fn re_seeding_a_wholly_dead_log_keeps_every_file() {
    let mut s = Store::new();
    let pats = lib(400);
    let mut placed: Vec<Pat> = Vec::new();
    for p in &pats {
        if s.save(p).is_err() {
            break;
        }
        placed.push(p.clone());
    }
    for p in &placed {
        assert!(s.delete(&p.name));
    }
    let (used, n, dead) = s.stats();
    assert_eq!((used, n), (0, 0));
    assert!(dead > 0);

    let seed: Vec<Pat> = lib(20);
    for p in &seed {
        s.save(p).unwrap_or_else(|e| panic!("save {}: {}", p.name, e));
    }
    assert_exactly(&mut s, &seed, "after re-seeding a wholly dead log");
    assert_eq!(s.dead_bytes, 0, "a wiped log has no dead bytes left");
}

/// A compaction reclaims *everything* when nothing is pinned, and exactly
/// what the frozen pages force when something is.
#[test]
fn a_compaction_reclaims_every_dead_byte_it_can() {
    for pin in [None, Some(0usize), Some(4)] {
        let mut s = Store::new();
        let pats = lib(12);
        for p in &pats {
            s.save(p).unwrap();
        }
        let pinned_name = pin.map(|i| pats[i].name.clone());
        if let Some(n) = &pinned_name {
            let seq = s.rec_by_name(n).unwrap().seq;
            s.pinned = vec![seq];
        }
        let mut live: Vec<Pat> = Vec::new();
        for (i, p) in pats.iter().enumerate() {
            if i % 2 == 0 && Some(&p.name) != pinned_name.as_ref() {
                assert!(s.delete(&p.name));
            } else {
                live.push(p.clone());
            }
        }
        let before_dead = s.dead_bytes;
        assert!(s.compact(0), "compaction refused with pin {:?}", pin);
        assert_exactly(&mut s, &live, &format!("compaction with pin {:?}", pin));

        // Dead bytes left = exactly the hole the frozen pages force the
        // repack to leave. With nothing pinned that is zero, always; with
        // a pin it is under one erase page, because the only thing a
        // frozen region costs is the tail of its last page.
        let used: u32 = s.index.iter().map(|r| r.size()).sum();
        assert_eq!(
            s.dead_bytes,
            s.cursor - used,
            "pin {:?}: dead must be exactly what the cursor says is unclaimed",
            pin
        );
        match pin {
            // Nothing frozen: every dead byte comes back, always. This is
            // the number the device could never get back to (#379).
            None => assert_eq!(s.dead_bytes, 0, "no pin: a compaction reclaims every dead byte"),
            // A file pinned at offset 0 freezes only page 0, and whatever
            // shares that page stays where it is, so what is left over is
            // under one erase page.
            Some(0) => assert!(
                s.dead_bytes < PAGE,
                "pin at 0: only page 0 is frozen, {} B left",
                s.dead_bytes
            ),
            // A pin further up cannot move, so the dead space *below* it
            // stays dead — but everything above it still packs down.
            Some(_) => assert!(
                s.dead_bytes < before_dead,
                "pin {:?}: a compaction must still reclaim what it can ({} B of {} B)",
                pin,
                s.dead_bytes,
                before_dead
            ),
        }
    }
}

/// Random pins, random churn, hundreds of rounds: the live set is preserved
/// after **every** operation.
#[test]
fn pinned_churn_fuzz_preserves_the_live_set() {
    let mut rng = 0x9E37_79B9u32;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng
    };
    for run in 0..8u32 {
        let mut s = Store::new();
        let mut live: Vec<Pat> = Vec::new();
        let mut n = 0usize;
        let mut compactions = 0;
        for round in 0..300 {
            // repin: 0, 1 or 2 files an engine is executing in place
            let want_pins = (next() % 3) as usize;
            let mut pins = Vec::new();
            for _ in 0..want_pins.min(live.len()) {
                let i = (next() as usize) % live.len();
                if let Some(r) = s.rec_by_name(&live[i].name) {
                    pins.push(r.seq);
                }
            }
            s.pinned = pins;

            let what = next() % 10;
            if what < 3 && !live.is_empty() {
                let i = (next() as usize) % live.len();
                let name = live[i].name.clone();
                if s.rec_by_name(&name).map(|r| s.pinned.contains(&r.seq)) != Some(true) {
                    assert!(s.delete(&name));
                    live.remove(i);
                }
            } else if what < 5 && !live.is_empty() {
                // re-save an existing name with fresh bytes
                let i = (next() as usize) % live.len();
                let src = 400 + (next() % 5000) as usize;
                let bc = 400 + (next() % 4000) as usize;
                let p = Pat::new(&live[i].name.clone(), src, bc);
                if s.save(&p).is_ok() {
                    live[i] = p;
                }
            } else {
                let src = 400 + (next() % 6000) as usize;
                let bc = 400 + (next() % 5000) as usize;
                let p = Pat::new(&format!("f{}-{}", run, n), src, bc);
                n += 1;
                let before = s.compactions;
                if s.save(&p).is_ok() {
                    live.push(p);
                }
                compactions += (s.compactions - before) as u32;
            }
            assert_exactly(&mut s, &live, &format!("run {} round {}", run, round));
        }
        assert!(compactions > 0, "run {} never compacted", run);
    }
}

/// The guard itself: a plan that would not place every live record must be
/// refused, never executed.
///
/// An index holding two records whose extents overlap is the realistic way
/// to get one: a stale entry that the walk and the incremental index
/// disagree about. `plan`'s packing would place them on top of each other and the
/// compaction would write one file over another; the plan check has to
/// catch that before a single page is erased.
#[test]
fn a_plan_that_does_not_place_every_record_is_refused() {
    let mut s = Store::new();
    let pats = lib(6);
    for p in &pats {
        s.save(p).unwrap();
    }
    let good = s.index.clone();
    let mut places = Vec::new();
    assert!(
        crate::patlog::plan(&good, &[], &mut |p| places.push(p)).is_some(),
        "a well-formed index plans fine"
    );

    let mut bad = good.clone();
    let mut ghost = good[2];
    ghost.off += 8; // overlaps the record it was copied from
    ghost.seq = 900;
    bad.push(ghost);
    sort_by_off(&mut bad);
    places.clear();
    assert!(
        crate::patlog::plan(&bad, &[], &mut |p| places.push(p)).is_none(),
        "overlapping records must be refused, not packed on top of each other"
    );

    // ...and the store refuses to act on it: nothing erased, nothing
    // written, every file still there.
    s.index = bad;
    let before = s.f.mem.clone();
    let erases = s.f.erases;
    assert!(!s.compact(0), "compaction must refuse the bad plan");
    assert_eq!(s.refused, 1);
    assert_eq!(s.f.erases, erases, "a refused compaction erases nothing");
    assert!(s.f.mem == before, "a refused compaction writes nothing");
    s.reload();
    assert_exactly(&mut s, &pats, "after a refused compaction");
}

/// A power cut at every write boundary of a compaction **with a file
/// pinned**: at most the file in flight is lost, the pinned file is never
/// touched, everything that survives is byte-perfect, and the store still
/// works afterwards.
#[test]
fn a_cut_at_every_boundary_of_a_pinned_compaction_is_survivable() {
    // Small files on purpose: the whole store is rebuilt once per cut
    // point, so the scenario is sized to span several erase pages without
    // turning the suite into a minute of work.
    let pats: Vec<Pat> = (0..7)
        .map(|i| Pat::new(&format!("cut-{}", i), 500 + i * 220, 400 + i * 130))
        .collect();
    let build = |s: &mut Store| -> Vec<Pat> {
        let mut live = Vec::new();
        for p in &pats {
            s.save(p).unwrap();
        }
        let seq = s.rec_by_name(&pats[2].name).unwrap().seq;
        s.pinned = vec![seq];
        for (i, p) in pats.iter().enumerate() {
            if i % 3 == 0 && i != 2 {
                assert!(s.delete(&p.name));
            } else {
                live.push(p.clone());
            }
        }
        live
    };

    let mut probe = Store::new();
    build(&mut probe);
    let before = probe.f.ops;
    assert!(probe.compact(0));
    let total = probe.f.ops - before;
    assert!(total > 8, "a compaction must take several steps: {}", total);

    for cut in 0..=total {
        let mut s = Store::new();
        let live = build(&mut s);
        s.f.budget = s.f.ops + cut;
        s.compact(0);
        s.f.off = false;
        s.f.budget = usize::MAX;
        s.reload();

        let got = s.on_flash();
        for (n, src, bc) in &got {
            let p = pats.iter().find(|p| &p.name == n).expect("a foreign name appeared");
            assert_eq!(src, &p.src, "cut {}: {} has foreign source", cut, n);
            assert_eq!(bc, &p.bc, "cut {}: {} has foreign bytecode", cut, n);
        }
        assert!(
            got.iter().any(|(n, _, _)| n == &pats[2].name),
            "cut {}: the pinned file must never be lost",
            cut
        );
        assert!(
            got.len() + 1 >= live.len(),
            "cut {}: a compaction may lose the file in flight, not the log ({} of {})",
            cut,
            got.len(),
            live.len()
        );
        // and the store is usable again
        assert!(
            s.save(&Pat::new("after-the-cut", 300, 300)).is_ok() || s.compact(0),
            "cut {}: wedged",
            cut
        );
    }
    println!("survived {} cut points of a pinned compaction", total + 1);
}

/// Re-saving an existing pattern when the log has to compact first.
///
/// The save resolves the previous generation's record *before* it knows a
/// compaction is needed, and retires it *after*. The compaction moves every
/// unpinned file, so the address it captured up front is stale by then:
/// writing the DEAD word to it drops four zero bytes into the middle of
/// whichever file was repacked over those bytes, tearing it. That was half
/// of Gitea #379 — the half that needs no pin at all.
#[test]
fn a_re_save_that_compacts_retires_the_right_bytes() {
    let mut s = Store::new();
    let pats = lib(400);
    let mut live: Vec<Pat> = Vec::new();
    for p in &pats {
        if s.save(p).is_err() {
            break;
        }
        live.push(p.clone());
    }
    // Free space low down, leaving the tail full: the next save has
    // nowhere to append and must compact, and the compaction has to move
    // the files above the holes — including the one about to be re-saved.
    for p in live.clone().iter().skip(1).step_by(4) {
        assert!(s.delete(&p.name));
        live.retain(|q| q.name != p.name);
    }
    assert_exactly(&mut s, &live, "after the deletes");

    let victim = live[3].name.clone();
    let retired = s.rec_by_name(&victim).unwrap();
    let (was_at, retired) = (retired.off, retired.size());
    let fresh = Pat::new(&victim, 7000, 5000);
    let compactions = s.compactions;
    s.save(&fresh).expect("the re-save should fit after a compaction");
    assert!(s.compactions > compactions, "the re-save did not compact — nothing is proven");
    assert!(
        was_at < s.cursor,
        "the address the save captured up front ({}) must still be inside the packed log          ({} B) or the stale write lands harmlessly in erased flash and proves nothing",
        was_at,
        s.cursor
    );

    for p in live.iter_mut() {
        if p.name == victim {
            *p = fresh.clone();
        }
    }
    assert_exactly(&mut s, &live, "after a re-save that compacted");
    // The compaction reclaimed everything, so the only dead bytes left are
    // the generation this very save retired.
    assert_eq!(s.dead_bytes, retired, "dead must be exactly the generation just retired");

    // And it was retired at the address the compaction gave it, not at the
    // one the save read before compacting. Exactly one record on flash is
    // dead, it is that generation, it is not where the save first found it,
    // and nothing was torn by a stray DEAD word.
    let mut dead: Vec<(u32, u32)> = Vec::new();
    let stats = patlog::scan(&mut s.f, &mut |r: &Rec, _: &[u8]| {
        if r.dead {
            dead.push((r.off, r.size()));
        }
    });
    assert_eq!(stats.torn, 0, "a stale DEAD word tore a file");
    assert_eq!(dead.len(), 1, "exactly one retired generation, found {:?}", dead);
    assert_eq!(dead[0].1, retired, "the retired record is the old generation");
    assert_ne!(dead[0].0, was_at, "the DEAD word went to the pre-compaction address");
}
