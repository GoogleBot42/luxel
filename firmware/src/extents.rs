//! Page-granular extent allocator for the pattern code arena.
//!
//! The arena is a run of 4 KiB erase pages in the mapped raw half of the
//! `storage` partition. A stored pattern's executable LXBC occupies a
//! **contiguous run of pages** there, so the VM can execute it in place
//! (docs/research/flash-mmap.md — contiguous and 4-byte aligned is the one
//! property XIP needs, and the reason no off-the-shelf flash FS fits).
//!
//! This module is the *planning* half and nothing else: a page bitmap,
//! first-fit, a compaction plan, and the on-flash directory format. It is
//! `no_std`, allocation-free, panic-free, and unit-tested on the host
//! (`tools/extent-check`, `cargo test --workspace`). All flash I/O — the
//! write/invalidate/hash discipline, the `ota::with_flash` door, the
//! persisted directory item — lives in `patterns.rs`.
//!
//! # Directory
//!
//! One [Extent] per cached pattern *bytecode generation*: `seq` + `gen`
//! identify it against the pattern index, `start`/`len` locate it, `hash`
//! proves the bytes. `len == 0` marks an unused table entry. A re-save of
//! the RUNNING pattern deliberately leaves its old generation's extent in
//! the table (its pages are still executing); it is swept once something
//! else runs, which is why the table holds a few more entries than there
//! are patterns.

/// Erase-page size — the allocation unit.
pub const PAGE: usize = 4096;
/// Bitmap capacity. The arena is a subset of the 128-page raw half; a
/// compile-time assert in `patterns.rs` keeps the real count under this.
pub const MAX_PAGES: usize = 128;
/// Directory capacity: `patterns::MAX_PATTERNS` (24) plus headroom for the
/// stale generations a re-save of the running pattern leaves behind.
pub const MAX_EXTENTS: usize = 28;
/// Directory blob version (bumped ⇒ old tables are dropped wholesale).
pub const DIR_VER: u8 = 2;
/// Bytes per serialized entry: seq(4) gen(1) start(2) len(4) hash(4).
const ENT_BYTES: usize = 15;
/// Header: ver(1) count(1) total_pages(2).
const HDR_BYTES: usize = 4;
/// Upper bound on a serialized directory.
pub const SER_MAX: usize = HDR_BYTES + MAX_EXTENTS * ENT_BYTES;

const WORDS: usize = MAX_PAGES / 32;

/// Pages a blob of `len` bytes occupies.
pub const fn pages_for(len: u32) -> u16 {
    ((len as usize + PAGE - 1) / PAGE) as u16
}

/// One cached pattern's contiguous run of arena pages.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Extent {
    /// Pattern seq (the store's monotonic id).
    pub seq: u32,
    /// Bytecode generation the blob came from.
    pub gen: u8,
    /// First page, relative to the arena base.
    pub start: u16,
    /// Blob length in bytes. `0` = unused table entry.
    pub len: u32,
    /// FNV-1a of the blob, checked against the mapped bytes at boot.
    pub hash: u32,
}

/// An unused directory entry.
pub const FREE: Extent = Extent { seq: 0, gen: 0, start: 0, len: 0, hash: 0 };

impl Extent {
    pub const fn pages(&self) -> u16 {
        pages_for(self.len)
    }
    pub const fn live(&self) -> bool {
        self.len != 0
    }
    /// One past the last page.
    pub const fn end(&self) -> u16 {
        self.start + pages_for(self.len)
    }
}

/// One compaction step: slide extent `idx` down from `from` to `to`.
/// `to < from` always, and the ranges may overlap — the executor copies
/// page by page in ascending order, which is safe for a downward move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Move {
    pub idx: usize,
    pub from: u16,
    pub to: u16,
    pub pages: u16,
}

/// The page bitmap + directory.
pub struct Dir {
    ents: [Extent; MAX_EXTENTS],
    total: u16,
    bits: [u32; WORDS],
}

impl Dir {
    pub const fn new(total_pages: u16) -> Dir {
        Dir { ents: [FREE; MAX_EXTENTS], total: total_pages, bits: [0; WORDS] }
    }

    pub const fn total_pages(&self) -> u16 {
        self.total
    }

    pub fn used_pages(&self) -> u16 {
        self.ents.iter().filter(|e| e.live()).map(|e| e.pages()).sum()
    }

    pub fn count(&self) -> usize {
        self.ents.iter().filter(|e| e.live()).count()
    }

    pub fn entries(&self) -> impl Iterator<Item = (usize, &Extent)> {
        self.ents.iter().enumerate().filter(|(_, e)| e.live())
    }

    pub fn get(&self, idx: usize) -> Option<&Extent> {
        self.ents.get(idx).filter(|e| e.live())
    }

    /// The extent holding `seq`'s `gen` bytecode, if any.
    pub fn find(&self, seq: u32, gen: u8) -> Option<(usize, Extent)> {
        self.ents
            .iter()
            .enumerate()
            .find(|(_, e)| e.live() && e.seq == seq && e.gen == gen)
            .map(|(i, e)| (i, *e))
    }

    /// Any extent of `seq` other than generation `keep_gen`.
    pub fn find_other_gen(&self, seq: u32, keep_gen: u8) -> Option<(usize, Extent)> {
        self.ents
            .iter()
            .enumerate()
            .find(|(_, e)| e.live() && e.seq == seq && e.gen != keep_gen)
            .map(|(i, e)| (i, *e))
    }

    fn bit(&self, p: u16) -> bool {
        let p = p as usize;
        p < MAX_PAGES && self.bits[p / 32] & (1 << (p % 32)) != 0
    }

    fn set_range(&mut self, start: u16, pages: u16, on: bool) {
        for p in start..start + pages {
            let p = p as usize;
            if p >= MAX_PAGES {
                return;
            }
            if on {
                self.bits[p / 32] |= 1 << (p % 32);
            } else {
                self.bits[p / 32] &= !(1 << (p % 32));
            }
        }
    }

    fn range_free(&self, start: u16, pages: u16) -> bool {
        if pages == 0 || start as usize + pages as usize > self.total as usize {
            return false;
        }
        (start..start + pages).all(|p| !self.bit(p))
    }

    /// Lowest start page of a free run of `pages`, or None.
    pub fn first_fit(&self, pages: u16) -> Option<u16> {
        if pages == 0 || pages > self.total {
            return None;
        }
        let mut run = 0u16;
        for p in 0..self.total {
            if self.bit(p) {
                run = 0;
            } else {
                run += 1;
                if run == pages {
                    return Some(p + 1 - pages);
                }
            }
        }
        None
    }

    /// Largest free run available right now (fragmentation, measured).
    pub fn largest_hole(&self) -> u16 {
        let (mut run, mut best) = (0u16, 0u16);
        for p in 0..self.total {
            if self.bit(p) {
                run = 0;
            } else {
                run += 1;
                if run > best {
                    best = run;
                }
            }
        }
        best
    }

    /// Claim `pages` at `start` for `e` (which carries its own `start`).
    /// None when the table is full or the range is not free.
    pub fn insert(&mut self, e: Extent) -> Option<usize> {
        if !e.live() || !self.range_free(e.start, e.pages()) {
            return None;
        }
        let i = self.ents.iter().position(|s| !s.live())?;
        self.set_range(e.start, e.pages(), true);
        self.ents[i] = e;
        Some(i)
    }

    pub fn remove(&mut self, idx: usize) -> bool {
        let Some(e) = self.ents.get(idx).copied().filter(|e| e.live()) else {
            return false;
        };
        self.set_range(e.start, e.pages(), false);
        self.ents[idx] = FREE;
        true
    }

    /// Drop every extent of `seq` (a delete). Returns how many went.
    pub fn remove_seq(&mut self, seq: u32) -> usize {
        let mut n = 0;
        for i in 0..MAX_EXTENTS {
            if self.ents[i].live() && self.ents[i].seq == seq {
                self.remove(i);
                n += 1;
            }
        }
        n
    }

    /// Drop every extent `keep` rejects. Returns how many went.
    pub fn retain(&mut self, mut keep: impl FnMut(&Extent) -> bool) -> usize {
        let mut n = 0;
        for i in 0..MAX_EXTENTS {
            if self.ents[i].live() && !keep(&self.ents[i]) {
                self.remove(i);
                n += 1;
            }
        }
        n
    }

    /// Live entry indices ordered by start page (insertion sort, no alloc).
    fn order(&self) -> ([usize; MAX_EXTENTS], usize) {
        let mut ord = [0usize; MAX_EXTENTS];
        let mut n = 0;
        for (i, e) in self.entries() {
            let mut k = n;
            while k > 0 && self.ents[ord[k - 1]].start > e.start {
                ord[k] = ord[k - 1];
                k -= 1;
            }
            ord[k] = i;
            n += 1;
        }
        (ord, n)
    }

    /// The next compaction step, or None when nothing more can slide down.
    /// `pinned` (the running pattern's seq) never moves; it splits the free
    /// space in two rather than blocking compaction.
    pub fn next_move(&self, pinned: Option<u32>) -> Option<Move> {
        let (ord, n) = self.order();
        let mut cursor = 0u16;
        for k in 0..n {
            let e = self.ents[ord[k]];
            if Some(e.seq) == pinned {
                cursor = e.end();
                continue;
            }
            if e.start > cursor {
                return Some(Move { idx: ord[k], from: e.start, to: cursor, pages: e.pages() });
            }
            cursor = e.end();
        }
        None
    }

    /// The largest free run compaction could produce, without doing any of
    /// it — the "is it worth erasing anything?" test before a save
    /// compacts. A pinned extent leaves at most one hole below it, so the
    /// answer is the larger of that hole and the tail.
    pub fn compacted_free_run(&self, pinned: Option<u32>) -> u16 {
        let (ord, n) = self.order();
        let (mut cursor, mut best) = (0u16, 0u16);
        for k in 0..n {
            let e = self.ents[ord[k]];
            if Some(e.seq) == pinned {
                if e.start > cursor && e.start - cursor > best {
                    best = e.start - cursor;
                }
                cursor = e.end();
            } else {
                cursor += e.pages();
            }
        }
        if self.total > cursor && self.total - cursor > best {
            best = self.total - cursor;
        }
        best
    }

    // --- the persisted blob ---

    /// Serialize into `out` (≥ [SER_MAX] bytes); returns the byte count, or
    /// 0 if the buffer is too small to be safe.
    pub fn to_bytes(&self, out: &mut [u8]) -> usize {
        if out.len() < SER_MAX {
            return 0;
        }
        let mut at = HDR_BYTES;
        let mut count = 0u8;
        for (_, e) in self.entries() {
            out[at..at + 4].copy_from_slice(&e.seq.to_le_bytes());
            out[at + 4] = e.gen;
            out[at + 5..at + 7].copy_from_slice(&e.start.to_le_bytes());
            out[at + 7..at + 11].copy_from_slice(&e.len.to_le_bytes());
            out[at + 11..at + 15].copy_from_slice(&e.hash.to_le_bytes());
            at += ENT_BYTES;
            count += 1;
        }
        out[0] = DIR_VER;
        out[1] = count;
        out[2..4].copy_from_slice(&self.total.to_le_bytes());
        at
    }

    /// Load a persisted directory for an arena of `total_pages`. Entries
    /// that do not fit the layout or overlap one already loaded are
    /// dropped (`dropped` counts them). A version or size mismatch yields
    /// an empty directory — never a wrong one.
    pub fn from_bytes(b: &[u8], total_pages: u16) -> (Dir, u32) {
        let mut d = Dir::new(total_pages);
        if b.len() < HDR_BYTES || b[0] != DIR_VER {
            return (d, 0);
        }
        if u16::from_le_bytes([b[2], b[3]]) != total_pages {
            return (d, 0);
        }
        let n = b[1] as usize;
        if b.len() < HDR_BYTES + n * ENT_BYTES {
            return (d, 0);
        }
        let mut dropped = 0u32;
        for i in 0..n {
            let r = &b[HDR_BYTES + i * ENT_BYTES..HDR_BYTES + (i + 1) * ENT_BYTES];
            let e = Extent {
                seq: u32::from_le_bytes([r[0], r[1], r[2], r[3]]),
                gen: r[4],
                start: u16::from_le_bytes([r[5], r[6]]),
                len: u32::from_le_bytes([r[7], r[8], r[9], r[10]]),
                hash: u32::from_le_bytes([r[11], r[12], r[13], r[14]]),
            };
            if d.insert(e).is_none() {
                dropped += 1;
            }
        }
        (d, dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ext(seq: u32, start: u16, pages: u16) -> Extent {
        Extent { seq, gen: 1, start, len: pages as u32 * PAGE as u32, hash: seq ^ 0xa5a5 }
    }

    /// What `patterns.rs::cache_code` does around its flash write: first-fit
    /// a run, then publish the extent there.
    fn alloc(d: &mut Dir, seq: u32, gen: u8, len: u32, hash: u32) -> Option<usize> {
        let start = d.first_fit(pages_for(len))?;
        d.insert(Extent { seq, gen, start, len, hash })
    }

    /// Full compaction, driven exactly the way `patterns.rs` drives it:
    /// un-publish the extent, copy, re-publish at the new start. (The
    /// un-publish is what bounds a power cut mid-move to one extent, and
    /// it is also what frees the destination when the ranges overlap.)
    fn compact(d: &mut Dir, pinned: Option<u32>) -> usize {
        let mut moves = 0;
        while let Some(mv) = d.next_move(pinned) {
            assert!(mv.to < mv.from, "a move must go downward");
            let e = *d.get(mv.idx).expect("planned move names a live extent");
            assert_eq!(e.pages(), mv.pages);
            assert!(d.remove(mv.idx));
            let moved = Extent { start: mv.to, ..e };
            assert!(d.insert(moved).is_some(), "the planned destination must be free");
            moves += 1;
            assert!(moves < 1000, "compaction must terminate");
        }
        moves
    }

    #[test]
    fn pages_for_rounds_up() {
        assert_eq!(pages_for(0), 0);
        assert_eq!(pages_for(1), 1);
        assert_eq!(pages_for(4096), 1);
        assert_eq!(pages_for(4097), 2);
        assert_eq!(pages_for(38400), 10); // MAX_BC
    }

    #[test]
    fn alloc_is_first_fit_and_accounts_pages() {
        let mut d = Dir::new(87);
        assert_eq!(d.used_pages(), 0);
        let a = alloc(&mut d, 1, 0, 900, 0xaa).unwrap();
        let b = alloc(&mut d, 2, 0, 9000, 0xbb).unwrap();
        assert_eq!(d.get(a).unwrap().start, 0);
        assert_eq!(d.get(a).unwrap().pages(), 1);
        assert_eq!(d.get(b).unwrap().start, 1);
        assert_eq!(d.get(b).unwrap().pages(), 3);
        assert_eq!(d.used_pages(), 4);
        assert_eq!(d.total_pages(), 87);
        assert_eq!(d.largest_hole(), 83);
    }

    #[test]
    fn free_leaves_a_hole_first_fit_reuses() {
        let mut d = Dir::new(16);
        let a = alloc(&mut d, 1, 0, 2 * PAGE as u32, 0).unwrap();
        alloc(&mut d, 2, 0, PAGE as u32, 0).unwrap();
        alloc(&mut d, 3, 0, PAGE as u32, 0).unwrap();
        assert_eq!(d.used_pages(), 4);
        d.remove(a);
        assert_eq!(d.used_pages(), 2);
        assert_eq!(d.first_fit(2), Some(0));
        assert_eq!(d.first_fit(1), Some(0));
        // 3 pages does not fit the hole; it goes after the tail
        assert_eq!(d.first_fit(3), Some(4));
        let c = alloc(&mut d, 4, 0, 2 * PAGE as u32, 0).unwrap();
        assert_eq!(d.get(c).unwrap().start, 0);
    }

    #[test]
    fn alloc_fails_when_the_pool_is_full() {
        let mut d = Dir::new(4);
        assert!(alloc(&mut d, 1, 0, 4 * PAGE as u32, 0).is_some());
        assert!(alloc(&mut d, 2, 0, 1, 0).is_none());
        assert_eq!(d.first_fit(1), None);
        assert_eq!(d.used_pages(), 4);
    }

    #[test]
    fn table_capacity_is_enforced() {
        let mut d = Dir::new(MAX_PAGES as u16);
        for i in 0..MAX_EXTENTS {
            assert!(alloc(&mut d, i as u32 + 1, 0, 1, 0).is_some(), "entry {i}");
        }
        assert!(alloc(&mut d, 999, 0, 1, 0).is_none(), "table full");
        assert_eq!(d.count(), MAX_EXTENTS);
    }

    #[test]
    fn overlapping_insert_is_refused() {
        let mut d = Dir::new(16);
        assert!(d.insert(ext(1, 2, 3)).is_some());
        assert!(d.insert(ext(2, 4, 2)).is_none(), "overlap");
        assert!(d.insert(ext(3, 15, 2)).is_none(), "past the end");
        assert!(d.insert(ext(4, 5, 2)).is_some());
    }

    #[test]
    fn compaction_packs_everything_down() {
        let mut d = Dir::new(20);
        d.insert(ext(1, 3, 2)).unwrap();
        d.insert(ext(2, 9, 1)).unwrap();
        d.insert(ext(3, 14, 4)).unwrap();
        // holes: 0..3, 5..9, 10..14, 18..20 — 13 free pages, best run 4
        assert_eq!(d.largest_hole(), 4);
        assert_eq!(d.compacted_free_run(None), 13);
        let moves = compact(&mut d, None);
        assert_eq!(moves, 3);
        assert_eq!(d.find(1, 1).unwrap().1.start, 0);
        assert_eq!(d.find(2, 1).unwrap().1.start, 2);
        assert_eq!(d.find(3, 1).unwrap().1.start, 3);
        assert_eq!(d.largest_hole(), 13);
        assert_eq!(d.used_pages(), 7);
        assert_eq!(d.next_move(None), None, "idempotent once compact");
    }

    #[test]
    fn compaction_never_moves_the_running_extent() {
        let mut d = Dir::new(20);
        d.insert(ext(1, 3, 2)).unwrap();
        d.insert(ext(2, 9, 1)).unwrap(); // pinned: the running pattern
        d.insert(ext(3, 14, 4)).unwrap();
        let predicted = d.compacted_free_run(Some(2));
        compact(&mut d, Some(2));
        assert_eq!(d.find(2, 1).unwrap().1.start, 9, "pinned extent stayed put");
        assert_eq!(d.find(1, 1).unwrap().1.start, 0);
        assert_eq!(d.find(3, 1).unwrap().1.start, 10);
        // holes: pages 2..9 (7) below the pin, 14..20 (6) above
        assert_eq!(d.largest_hole(), 7);
        assert_eq!(predicted, 7, "compacted_free_run predicted the outcome");
    }

    #[test]
    fn compacted_free_run_matches_reality_on_many_layouts() {
        // exhaustive-ish: every 3-extent layout in a 12-page arena
        for a in 0..6u16 {
            for b in a + 2..9u16 {
                for c in b + 1..12u16 {
                    for pin in [None, Some(1u32), Some(2), Some(3)] {
                        let mut d = Dir::new(12);
                        d.insert(ext(1, a, 2)).unwrap();
                        d.insert(ext(2, b, 1)).unwrap();
                        d.insert(ext(3, c, 1)).unwrap();
                        let predicted = d.compacted_free_run(pin);
                        compact(&mut d, pin);
                        assert_eq!(
                            predicted,
                            d.largest_hole(),
                            "layout {a},{b},{c} pin {pin:?}"
                        );
                        assert_eq!(d.used_pages(), 4);
                        if let Some(p) = pin {
                            let orig = [a, b, c][p as usize - 1];
                            assert_eq!(d.find(p, 1).unwrap().1.start, orig, "pin moved");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn compaction_opens_a_run_a_save_needs() {
        // 12 pages, three 2-page extents at 0, 5, 10 → largest hole 3,
        // but 6 free pages. A 5-page save fits only after compaction.
        let mut d = Dir::new(12);
        d.insert(ext(1, 0, 2)).unwrap();
        d.insert(ext(2, 5, 2)).unwrap();
        d.insert(ext(3, 10, 2)).unwrap();
        assert_eq!(d.first_fit(5), None);
        assert!(d.compacted_free_run(None) >= 5);
        compact(&mut d, None);
        assert_eq!(d.first_fit(5), Some(6));
    }

    #[test]
    fn a_pin_can_make_compaction_insufficient() {
        let mut d = Dir::new(12);
        d.insert(ext(1, 0, 2)).unwrap();
        d.insert(ext(2, 5, 2)).unwrap(); // pinned in the middle
        d.insert(ext(3, 10, 2)).unwrap();
        // packed: #1 at 0..2, the pin stays at 5..7, #3 slides to 7..9 —
        // two 3-page holes (2..5 and 9..12), so a 4-page save still fails
        assert_eq!(d.compacted_free_run(Some(2)), 3);
        compact(&mut d, Some(2));
        assert_eq!(d.find(3, 1).unwrap().1.start, 7);
        assert_eq!(d.largest_hole(), 3);
        assert_eq!(d.first_fit(4), None);
    }

    #[test]
    fn resave_allocates_elsewhere_then_frees_the_old_extent() {
        let mut d = Dir::new(16);
        let old = alloc(&mut d, 7, 1, 3 * PAGE as u32, 0x11).unwrap();
        let old_start = d.get(old).unwrap().start;
        // gen 2 must land somewhere else — the old bytes may be executing
        let new = alloc(&mut d, 7, 2, 3 * PAGE as u32, 0x22).unwrap();
        assert_ne!(d.get(new).unwrap().start, old_start);
        assert_eq!(d.used_pages(), 6);
        assert!(d.find(7, 1).is_some() && d.find(7, 2).is_some());
        // once it is no longer running, the old generation is swept
        let (i, _) = d.find_other_gen(7, 2).unwrap();
        assert_eq!(i, old);
        d.remove(i);
        assert_eq!(d.used_pages(), 3);
        assert!(d.find(7, 1).is_none());
    }

    #[test]
    fn remove_seq_and_retain() {
        let mut d = Dir::new(16);
        d.insert(ext(1, 0, 1)).unwrap();
        d.insert(Extent { seq: 1, gen: 2, start: 1, len: PAGE as u32, hash: 0 }).unwrap();
        d.insert(ext(2, 2, 1)).unwrap();
        assert_eq!(d.remove_seq(1), 2);
        assert_eq!(d.count(), 1);
        assert_eq!(d.used_pages(), 1);
        assert_eq!(d.retain(|e| e.seq == 99), 1);
        assert_eq!(d.count(), 0);
        assert_eq!(d.used_pages(), 0);
        assert_eq!(d.largest_hole(), 16);
    }

    #[test]
    fn serialization_round_trips() {
        let mut d = Dir::new(87);
        alloc(&mut d, 3, 1, 900, 0xdead_beef).unwrap();
        alloc(&mut d, 4, 2, 20_000, 0x0bad_f00d).unwrap();
        alloc(&mut d, 5, 0, 4096, 1).unwrap();
        let mut buf = [0u8; SER_MAX];
        let n = d.to_bytes(&mut buf);
        assert_eq!(n, HDR_BYTES + 3 * ENT_BYTES);
        let (back, dropped) = Dir::from_bytes(&buf[..n], 87);
        assert_eq!(dropped, 0);
        assert_eq!(back.count(), 3);
        assert_eq!(back.used_pages(), d.used_pages());
        for (_, e) in d.entries() {
            assert_eq!(back.find(e.seq, e.gen).unwrap().1, *e);
        }
    }

    #[test]
    fn serialization_survives_a_full_table() {
        let mut d = Dir::new(MAX_PAGES as u16);
        for i in 0..MAX_EXTENTS {
            alloc(&mut d, i as u32 + 1, 0, 1, i as u32).unwrap();
        }
        let mut buf = [0u8; SER_MAX];
        let n = d.to_bytes(&mut buf);
        assert!(n <= SER_MAX);
        let (back, dropped) = Dir::from_bytes(&buf[..n], MAX_PAGES as u16);
        assert_eq!((back.count(), dropped), (MAX_EXTENTS, 0));
    }

    #[test]
    fn a_torn_or_foreign_directory_loads_empty() {
        let mut d = Dir::new(87);
        alloc(&mut d, 1, 0, 900, 0).unwrap();
        let mut buf = [0u8; SER_MAX];
        let n = d.to_bytes(&mut buf);

        // wrong version
        let mut v = buf;
        v[0] = DIR_VER ^ 0xff;
        assert_eq!(Dir::from_bytes(&v[..n], 87).0.count(), 0);
        // arena resized under us (a layout change in a new firmware)
        assert_eq!(Dir::from_bytes(&buf[..n], 70).0.count(), 0);
        // truncated
        assert_eq!(Dir::from_bytes(&buf[..n - 1], 87).0.count(), 0);
        assert_eq!(Dir::from_bytes(&[], 87).0.count(), 0);
    }

    #[test]
    fn overlapping_and_out_of_range_entries_are_dropped_on_load() {
        // hand-built blob: two overlapping extents + one past the end
        let mut buf = [0u8; SER_MAX];
        let mk = |b: &mut [u8], seq: u32, start: u16, pages: u16| {
            b[0..4].copy_from_slice(&seq.to_le_bytes());
            b[4] = 1;
            b[5..7].copy_from_slice(&start.to_le_bytes());
            b[7..11].copy_from_slice(&(pages as u32 * PAGE as u32).to_le_bytes());
            b[11..15].copy_from_slice(&0u32.to_le_bytes());
        };
        buf[0] = DIR_VER;
        buf[1] = 3;
        buf[2..4].copy_from_slice(&20u16.to_le_bytes());
        mk(&mut buf[HDR_BYTES..], 1, 0, 4);
        mk(&mut buf[HDR_BYTES + ENT_BYTES..], 2, 2, 4); // overlaps #1
        mk(&mut buf[HDR_BYTES + 2 * ENT_BYTES..], 3, 18, 4); // past the end
        let (d, dropped) = Dir::from_bytes(&buf[..HDR_BYTES + 3 * ENT_BYTES], 20);
        assert_eq!(dropped, 2);
        assert_eq!(d.count(), 1);
        assert!(d.find(1, 1).is_some());
        assert_eq!(d.used_pages(), 4);
    }

    #[test]
    fn churn_never_corrupts_the_bitmap() {
        // deterministic pseudo-random save/delete/compact churn; the
        // invariant is that used_pages always equals the bitmap and no two
        // extents overlap.
        let mut d = Dir::new(40);
        let mut rng = 0x1234_5678u32;
        let mut next = |rng: &mut u32| {
            *rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *rng >> 16
        };
        for step in 0..4000 {
            let seq = next(&mut rng) % 20 + 1;
            match next(&mut rng) % 3 {
                0 => {
                    let len = (next(&mut rng) % 9 + 1) * PAGE as u32;
                    let pinned = Some(1u32);
                    if d.first_fit(pages_for(len)).is_none()
                        && d.compacted_free_run(pinned) >= pages_for(len)
                    {
                        compact(&mut d, pinned);
                    }
                    let _ = alloc(&mut d, seq, (step % 3) as u8, len, seq);
                }
                1 => {
                    d.remove_seq(seq);
                }
                _ => {
                    compact(&mut d, Some(1));
                }
            }
            // invariants
            let mut seen = [false; MAX_PAGES];
            let mut n = 0u16;
            for (_, e) in d.entries() {
                assert!(e.end() <= d.total_pages(), "step {step}: extent past the arena");
                for p in e.start..e.end() {
                    assert!(!seen[p as usize], "step {step}: page {p} double-booked");
                    seen[p as usize] = true;
                }
                n += e.pages();
            }
            assert_eq!(n, d.used_pages(), "step {step}: page accounting");
            for p in 0..d.total_pages() {
                assert_eq!(seen[p as usize], d.bit(p), "step {step}: bitmap page {p}");
            }
            assert!(d.used_pages() + d.largest_hole() <= d.total_pages());
        }
        assert!(d.count() > 0, "the churn should leave something cached");
    }
}
