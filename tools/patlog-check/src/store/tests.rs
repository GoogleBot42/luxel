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

/// Pinning the HIGHEST file makes the *tail* unreclaimable: a pinned record
/// never moves, so the packed length can never fall below its end. Every
/// file must survive that (Gitea #379) — and, since #388, the store must
/// also still hand back the space the repack frees BELOW the pin instead of
/// refusing saves with most of the log idle.
#[test]
fn a_pinned_tail_keeps_every_live_file_and_still_yields_its_space() {
    let (compacted, _) = fill_churn_and_check(Some(usize::MAX));
    assert!(compacted > 0, "a pinned tail must still reclaim what is under it (#388)");
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
    let mut total_compactions = 0u32;
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
        total_compactions += compactions;
    }
    // Per-run this is no longer guaranteed: since #388 a save can be served
    // out of a hole an earlier repack left below a pin, so a lucky run
    // never has to compact at all. Over the whole fuzz it must still
    // happen, or nothing here exercises the compaction path.
    assert!(total_compactions > 0, "the fuzz never compacted");
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

// ---------------------------------------------------------------------------
// Gitea #388: a compaction must reclaim a damaged or deleted-heavy log, and
// a PINNED file must not cost the store the space underneath it.
//
// The device's symptom was `store.dead` never coming back to 0 and capacity
// falling to 49 of the documented 119 patterns. The trigger is a *stale*
// decode pin (`patterns::pin_code`, released by `unpin_code` since this
// pass) naming a file the engine stopped executing long ago — but the store
// must survive a legitimate pin too, so these tests seed the pin directly
// and demand the capacity back either way.

/// A pattern the size of the one the issue's comment saved.
fn churn_pat(name: &str) -> Pat {
    // 8,340 B on the wire = header + name + source + bytecode
    Pat::new(name, 4600, 3600)
}

/// Fill the log, delete every pattern in it, and return the store with the
/// arena exactly as the rig was found: every record below the cursor dead,
/// nothing live, nothing indexed.
fn wholly_dead_log() -> (Store, Vec<Pat>) {
    let mut s = Store::new();
    let pats = lib(400);
    let mut placed: Vec<Pat> = Vec::new();
    for p in &pats {
        if s.save(p).is_err() {
            break;
        }
        placed.push(p.clone());
    }
    assert!(placed.len() > 60, "only {} patterns fit", placed.len());
    // Pack the tail tight with progressively smaller files: with room left
    // at the top, the save under test would simply append and nothing would
    // have to be reclaimed at all.
    let mut k = 0;
    for sz in [3000usize, 1200, 400, 120] {
        loop {
            let p = Pat::new(&format!("filler-{}", k), sz, sz);
            k += 1;
            match s.save(&p) {
                Ok(_) => placed.push(p),
                Err(_) => break,
            }
        }
    }
    assert!(LOG_LEN - s.cursor < 512, "{} B still free at the tail", LOG_LEN - s.cursor);
    for p in &placed {
        assert!(s.delete(&p.name));
    }
    let (used, n, dead) = s.stats();
    assert_eq!((used, n), (0, 0), "every pattern was deleted");
    assert!(dead > LOG_LEN / 2, "the log should be almost entirely dead, not {} B", dead);
    (s, placed)
}

/// Gitea #388, the comment's repro exactly: a log where every record below
/// the cursor is dead, one save, nothing pinned. The compaction must give
/// back **every** byte.
#[test]
fn one_save_reclaims_a_wholly_dead_log() {
    let (mut s, _) = wholly_dead_log();
    let p = churn_pat("churn");
    s.save(&p).expect("the save must fit after the compaction");
    assert_eq!(s.compactions, 1, "exactly one compaction");
    assert_exactly(&mut s, &[p.clone()], "after re-seeding a wholly dead log");
    assert_eq!(s.dead_bytes, 0, "a compaction with nothing pinned reclaims every dead byte");
    assert_eq!(s.cursor, p.size(), "the log holds nothing but the new file");
    assert_eq!(s.sweep_failed, 0);
}

/// The same log with a pin left on a file near the TOP — the shape the rig
/// was actually in, and the one that stalled its reclamation. The pinned
/// file cannot move, so the cursor cannot come down; what must NOT happen is
/// the store then refusing to use the pages under it.
#[test]
fn a_pin_high_in_a_dead_log_does_not_cost_the_store_its_capacity() {
    // baseline: the same fill with nothing pinned
    let (mut s, _) = wholly_dead_log();
    let mut free_run: Vec<Pat> = Vec::new();
    for i in 0..400 {
        let p = Pat::new(&format!("fresh-{:03}", i), 600 + (i * 733) % 5200, 500 + (i * 449) % 4400);
        if s.save(&p).is_err() {
            break;
        }
        free_run.push(p);
    }
    let baseline = free_run.len();
    assert!(baseline > 60, "baseline fill only reached {}", baseline);

    let (mut s, placed) = wholly_dead_log();
    // A file near the top of the log, deleted like all the others, still
    // named by a pin: `keep` carries it, `plan` leaves it where it is, and
    // the packed length can never fall below its end.
    let victim = &placed[placed.len() - 4];
    let pinned = {
        let mut found = None;
        patlog::scan(&mut s.f, &mut |r: &Rec, n: &[u8]| {
            if n == victim.name.as_bytes() {
                found = Some(*r);
            }
        });
        found.expect("the deleted file is still on flash")
    };
    s.pinned = vec![pinned.seq];
    let pinned_bytes =
        s.f.mem[pinned.off as usize..pinned.end() as usize].to_vec();

    let mut live: Vec<Pat> = Vec::new();
    for i in 0..400 {
        let p = Pat::new(&format!("fresh-{:03}", i), 600 + (i * 733) % 5200, 500 + (i * 449) % 4400);
        if s.save(&p).is_err() {
            break;
        }
        live.push(p.clone());
        assert_exactly(&mut s, &live, &format!("after saving {} with a stale pin", p.name));
    }
    println!(
        "stale pin at {} B: {} patterns stored (baseline {}), dead {} B, {} compactions",
        pinned.off,
        live.len(),
        baseline,
        s.dead_bytes,
        s.compactions
    );
    // The pinned file's own bytes are untouched — that is the whole point
    // of the pin, and the one thing a fix here must never trade away.
    assert_eq!(
        s.f.mem[pinned.off as usize..pinned.end() as usize],
        pinned_bytes[..],
        "the pinned file was moved or erased"
    );
    // Before #388 the store used only the tail above the pin: with the pin
    // this high that was a handful of patterns, not most of the log.
    assert!(
        live.len() * 10 >= baseline * 9,
        "a pin cost the store {} of {} patterns",
        baseline - live.len(),
        baseline
    );
}

/// A pin on a LIVE file high in the log — the legitimate case (the running
/// pattern) — must cost no capacity either.
#[test]
fn a_pin_on_the_highest_live_file_does_not_cost_the_store_its_capacity() {
    let mut s = Store::new();
    let mut live: Vec<Pat> = Vec::new();
    for p in lib(400) {
        if s.save(&p).is_err() {
            break;
        }
        live.push(p);
    }
    let baseline = live.len();
    // pin the highest file, then delete every other one: only a compaction
    // can hand that space back, and the pin stops the cursor coming down.
    let top = s.index.last().copied().expect("a filled log has an index");
    s.pinned = vec![top.seq];
    let top_name = s.rec_name(&top).unwrap();
    let doomed: Vec<String> =
        live.iter().map(|p| p.name.clone()).filter(|n| n != &top_name).collect();
    for n in &doomed {
        assert!(s.delete(n));
    }
    live.retain(|p| p.name == top_name);
    let top_bytes = s.f.mem[top.off as usize..top.end() as usize].to_vec();

    for i in 0..400 {
        let p = Pat::new(&format!("after-{:03}", i), 600 + (i * 733) % 5200, 500 + (i * 449) % 4400);
        if s.save(&p).is_err() {
            break;
        }
        live.push(p);
    }
    println!(
        "live pin at {} B: {} patterns stored (baseline {}), dead {} B",
        top.off,
        live.len(),
        baseline,
        s.dead_bytes
    );
    assert_eq!(
        s.f.mem[top.off as usize..top.end() as usize],
        top_bytes[..],
        "the pinned file was moved or erased"
    );
    assert_exactly(&mut s, &live, "after refilling around a pinned live file");
    assert!(
        live.len() * 10 >= baseline * 9,
        "a live pin cost the store {} of {} patterns",
        baseline - live.len(),
        baseline
    );
}

/// A log carrying junk and stale-header damage between its records — the
/// issue's "damaged log" framing. One save must still reclaim all of it,
/// and every live file must come through byte-identical.
#[test]
fn a_compaction_reclaims_a_log_damaged_between_its_records() {
    let mut s = Store::new();
    let mut live: Vec<Pat> = Vec::new();
    for p in lib(400) {
        if s.save(&p).is_err() {
            break;
        }
        live.push(p);
    }
    // Delete every other file, then scribble over what the deletes left:
    // a torn header keeps its magic and its self_off but its payload no
    // longer hashes, which is what the #379 walk has to resync through.
    let doomed: Vec<String> = live.iter().step_by(2).map(|p| p.name.clone()).collect();
    let mut dead_recs: Vec<Rec> = Vec::new();
    for n in &doomed {
        let r = s.rec_by_name(n).expect("still stored");
        assert!(s.delete(n));
        dead_recs.push(r);
    }
    live.retain(|p| !doomed.contains(&p.name));
    for (i, r) in dead_recs.iter().enumerate() {
        // NOR can only clear bits, so scribble by ANDing — exactly what a
        // half-finished write leaves behind.
        let at = (r.src_off() + (i as u32 * 37) % 64) as usize;
        for b in s.f.mem[at..(at + 96).min(r.end() as usize)].iter_mut() {
            *b &= 0x5A;
        }
    }
    let torn = patlog::scan(&mut s.f, &mut |_: &Rec, _: &[u8]| {}).torn;
    assert!(torn > 0, "the damage did not produce a torn record");
    assert_exactly(&mut s, &live, "after damaging the dead records");

    // Fill the tail so the next save has to compact over the damage.
    for i in 0..400 {
        let p = churn_pat(&format!("post-{:03}", i));
        if s.save(&p).is_err() {
            break;
        }
        live.push(p.clone());
        assert_exactly(&mut s, &live, &format!("after saving {} over a damaged log", p.name));
    }
    assert!(s.compactions > 0, "the run never compacted — the test proves nothing");
    let used: u32 = s.index.iter().map(|r| r.size()).sum();
    assert_eq!(s.dead_bytes, s.cursor - used, "dead must be exactly what the cursor says is unclaimed");
    assert_eq!(s.sweep_failed, 0, "the closing sweep failed");
    println!(
        "damaged log: {} torn records, {} patterns, {} compactions, dead {} B",
        torn,
        live.len(),
        s.compactions,
        s.dead_bytes
    );
}

/// Issue #388 §4: a *healthy* near-full log was seen settling at a small
/// non-zero residue (4,096 B on a 119-pattern fill, 6,496 B under
/// `flashmap-off`) rather than at 0. This pins down what that is.
///
/// With nothing pinned the answer is: nothing. A compaction packs from 0
/// and `place()` resumes at the packed length itself, whose page is erased
/// under it, so there is no gap and `dead` is exactly 0. The residue the
/// rig saw is the *frozen page* a pin costs — under one erase page, which
/// is what the second half of this test measures.
#[test]
fn a_healthy_logs_residue_is_zero_and_a_pinned_ones_is_under_a_page() {
    let mut s = Store::new();
    let mut live: Vec<Pat> = Vec::new();
    for p in lib(400) {
        if s.save(&p).is_err() {
            break;
        }
        live.push(p);
    }
    // Churn: retire a quarter of the files, then re-save one name until the
    // log has to reclaim. Every superseded generation is dead weight.
    let doomed: Vec<String> = live.iter().step_by(4).map(|p| p.name.clone()).collect();
    for n in &doomed {
        assert!(s.delete(n));
    }
    live.retain(|p| !doomed.contains(&p.name));
    let victim = live[live.len() / 2].name.clone();
    for i in 0..30 {
        let p = Pat::new(&victim, 4600 + (i * 13) % 400, 3600 + (i * 29) % 400);
        if s.save(&p).is_err() {
            break;
        }
        for q in live.iter_mut() {
            if q.name == victim {
                *q = p.clone();
            }
        }
        assert_exactly(&mut s, &live, "during the churn");
    }
    assert!(s.dead_bytes > 0, "the churn left nothing to reclaim");

    // Nothing pinned: the residue is exactly zero.
    assert!(s.compact(0), "compaction refused on a healthy log");
    assert_exactly(&mut s, &live, "after a healthy compaction");
    let used: u32 = s.index.iter().map(|r| r.size()).sum();
    assert_eq!(s.dead_bytes, s.cursor - used);
    assert_eq!(s.dead_bytes, 0, "a healthy log keeps no residue at all");

    // One file pinned at the bottom: its page is frozen, so whatever shares
    // it stays put. That — and only that — is the small residue #388 §4
    // measured on the rig.
    let low = s.index[0];
    s.pinned = vec![low.seq];
    let churn = Pat::new(&victim, 5200, 4100);
    s.save(&churn).expect("a churn save on a packed log");
    for q in live.iter_mut() {
        if q.name == victim {
            *q = churn.clone();
        }
    }
    assert!(s.compact(0), "compaction refused with the lowest file pinned");
    assert_exactly(&mut s, &live, "after a pinned compaction");
    let used: u32 = s.index.iter().map(|r| r.size()).sum();
    assert_eq!(s.dead_bytes, s.cursor - used);
    assert!(
        s.dead_bytes < PAGE,
        "a pin at the bottom cost {} B, more than the frozen page it should",
        s.dead_bytes
    );
    println!("healthy residue 0 B; residue with the lowest file pinned {} B", s.dead_bytes);
}
