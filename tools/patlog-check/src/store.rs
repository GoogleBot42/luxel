//! A host replica of the *store state machine* in `firmware/src/patterns.rs`
//! — the layer above `patlog`: the RAM index, the cursor, the seq/stamp
//! counters, and the save / delete / compact sequence that drives them.
//!
//! `patlog` itself is host-compiled and has its own suite (the NOR
//! simulator, the power-cut sweeps, the churn fuzz). What that suite could
//! not see is the *caller*: its tests hand `compact()` a freshly derived
//! live list and never model the index the firmware actually keeps, nor the
//! order in which a save reads it, compacts, and then retires the previous
//! generation. Gitea #379 was exactly there — a compaction silently losing
//! the lowest-offset files on metal while every patlog test stayed green.
//!
//! So this module mirrors `patterns.rs` step for step, including the parts
//! that look redundant, and the tests below assert the one invariant the
//! device broke: **a compaction is a total function over the live set.**
//!
//! Keep it in step with `patterns.rs`. Every deviation is a test that
//! proves nothing.

use crate::patlog::{self, Arena, Rec, Step, PAGE};

/// `patterns.rs`' `LOG_LEN`: the packed file log is the tail of the extent
/// region, 183 erase pages on every board.
pub const LOG_PAGES: u32 = 183;
pub const LOG_LEN: u32 = LOG_PAGES * PAGE;
/// `patterns.rs`' `MAX_RECS`.
pub const MAX_RECS: usize = 192;

/// A NOR flash: erase sets 0xFF, a write only ever clears bits, and every
/// 4-byte word (and every page erase) is a step power can die between.
pub struct Nor {
    pub mem: Vec<u8>,
    /// Steps this power supply has left (`usize::MAX` = mains).
    pub budget: usize,
    pub ops: usize,
    pub off: bool,
    pub erases: usize,
}

impl Nor {
    pub fn new(len: u32) -> Nor {
        Nor { mem: vec![0xFF; len as usize], budget: usize::MAX, ops: 0, off: false, erases: 0 }
    }
    fn step(&mut self) -> bool {
        if self.off {
            return false;
        }
        if self.ops >= self.budget {
            self.off = true;
            return false;
        }
        self.ops += 1;
        true
    }
    fn erase(&mut self, page: u32) -> bool {
        if !self.step() {
            return false;
        }
        self.erases += 1;
        let a = (page * PAGE) as usize;
        self.mem[a..a + PAGE as usize].fill(0xFF);
        true
    }
    fn write(&mut self, off: u32, data: &[u8]) -> bool {
        assert_eq!(off % 4, 0, "write offset must be word aligned");
        // `write_at` pads a trailing partial word with 0xFF.
        let mut padded = data.to_vec();
        while padded.len() % 4 != 0 {
            padded.push(0xFF);
        }
        for (i, chunk) in padded.chunks(4).enumerate() {
            if !self.step() {
                return false;
            }
            let at = off as usize + i * 4;
            for (k, &b) in chunk.iter().enumerate() {
                self.mem[at + k] &= b; // NOR: 1 -> 0 only
            }
        }
        true
    }
}

impl Arena for Nor {
    fn len(&self) -> u32 {
        self.mem.len() as u32
    }
    fn view(&mut self, off: u32, want: usize) -> Option<&[u8]> {
        let at = off as usize;
        if at >= self.mem.len() {
            return None;
        }
        Some(&self.mem[at..(at + want).min(self.mem.len())])
    }
}

/// One stored pattern's bytes, as the API hands them in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pat {
    pub name: String,
    pub src: Vec<u8>,
    pub bc: Vec<u8>,
}

impl Pat {
    pub fn new(name: &str, src_len: usize, bc_len: usize) -> Pat {
        let seed = name.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
        let src = (0..src_len)
            .map(|i| b' ' + ((i as u32).wrapping_mul(7).wrapping_add(seed) % 90) as u8)
            .collect();
        let bc = (0..bc_len)
            .map(|i| (i as u32).wrapping_mul(31).wrapping_add(seed.wrapping_mul(13)) as u8)
            .collect();
        Pat { name: name.to_string(), src, bc }
    }
    fn size(&self) -> u32 {
        Rec::bytes(self.name.len() as u8, self.src.len() as u32, self.bc.len() as u32)
    }
}

/// Everything `patterns.rs` keeps in RAM, plus the flash it keeps it about.
pub struct Store {
    pub f: Nor,
    /// `INDEX`: one live record per stored pattern, ascending by offset.
    pub index: Vec<Rec>,
    pub cursor: u32,
    pub next_seq: u32,
    pub next_stamp: u32,
    pub dead_bytes: u32,
    pub overfull: bool,
    /// `pins()`: seqs an engine is executing in place.
    pub pinned: Vec<u32>,
    pub compactions: usize,
    /// Set when a compaction refused to run (the #379 guard).
    pub refused: usize,
}

/// `patterns.rs::sort_by_off`.
fn sort_by_off(v: &mut [Rec]) {
    for i in 1..v.len() {
        let mut k = i;
        while k > 0 && v[k - 1].off > v[k].off {
            v.swap(k - 1, k);
            k -= 1;
        }
    }
}

impl Store {
    pub fn new() -> Store {
        Store {
            f: Nor::new(LOG_LEN),
            index: Vec::new(),
            cursor: 0,
            next_seq: 0,
            next_stamp: 1,
            dead_bytes: 0,
            overfull: false,
            pinned: Vec::new(),
            compactions: 0,
            refused: 0,
        }
    }

    /// `patterns.rs::rec_name` — names live in flash, never in the index.
    pub fn rec_name(&mut self, r: &Rec) -> Option<String> {
        let n = r.name_len as usize;
        let v = self.f.view(r.name_off(), n)?;
        String::from_utf8(v[..n.min(v.len())].to_vec()).ok()
    }

    fn rec_by_name(&mut self, name: &str) -> Option<Rec> {
        let recs = self.index.clone();
        recs.into_iter().find(|r| self.rec_name(r).as_deref() == Some(name))
    }

    /// `patterns.rs::store_stats` → (used, patterns, dead).
    pub fn stats(&self) -> (u32, usize, u32) {
        (self.index.iter().map(|r| r.size()).sum(), self.index.len(), self.dead_bytes)
    }

    /// `patterns.rs::reload` — rebuild every byte of RAM state from flash.
    pub fn reload(&mut self) -> patlog::Scan {
        let mut live: Vec<Rec> = Vec::new();
        let mut seq = 0u32;
        let mut stamp = 1u32;
        let mut over = false;
        let stats = patlog::scan(&mut self.f, &mut |r: &Rec, _: &[u8]| {
            seq = seq.max(r.seq.wrapping_add(1));
            stamp = stamp.max(r.stamp.wrapping_add(1));
            if r.dead {
                return;
            }
            match live.iter_mut().find(|l| l.seq == r.seq) {
                Some(slot) => {
                    if r.stamp > slot.stamp {
                        *slot = *r;
                    }
                }
                None => {
                    if live.len() >= MAX_RECS {
                        over = true;
                    } else {
                        live.push(*r);
                    }
                }
            }
        });
        sort_by_off(&mut live);
        let used: u32 = live.iter().map(|r| r.size()).sum();
        self.next_seq = seq;
        self.next_stamp = stamp;
        self.cursor = stats.cursor;
        self.dead_bytes = stats.cursor.saturating_sub(used);
        self.overfull = over;
        self.index = live;
        stats
    }

    fn mark_dead(&mut self, r: &Rec) -> bool {
        if !self.f.write(r.dead_off(), &patlog::DEAD.to_le_bytes()) {
            return false;
        }
        self.dead_bytes = self.dead_bytes.saturating_add(r.size());
        true
    }

    /// `patterns.rs::erase_pages`.
    fn erase_pages(&mut self, from: u32, to: u32) -> bool {
        for p in from..to {
            if patlog::erased(&mut self.f, p * PAGE, PAGE) {
                continue;
            }
            if !self.f.erase(p) {
                return false;
            }
        }
        true
    }

    /// `patterns.rs::compact` — repack the log over its dead space.
    pub fn compact(&mut self, need: u32) -> bool {
        let pinned = self.pinned.clone();
        let live = self.index.clone();
        let mut keep = live.clone();
        let mut extra: Vec<Rec> = Vec::new();
        patlog::scan(&mut self.f, &mut |r: &Rec, _: &[u8]| {
            if pinned.contains(&r.seq) && !live.iter().any(|l| l.off == r.off) {
                extra.push(*r);
            }
        });
        keep.extend(extra);
        sort_by_off(&mut keep);

        let mut places: Vec<patlog::Place> = Vec::new();
        let Some(packed) = patlog::plan(&keep, &pinned, &mut |p| places.push(p)) else {
            self.refused += 1;
            return false;
        };
        let old_end = self.cursor;
        if packed + need > LOG_LEN {
            return false;
        }
        self.compactions += 1;

        let mut buf = vec![0u8; PAGE as usize];
        for page in 0..patlog::align_page(packed) / PAGE {
            if patlog::frozen_page(&keep, &pinned, page) {
                continue;
            }
            if !patlog::build_page(&mut self.f, page, &keep, &places, &mut buf) {
                break;
            }
            if self.f.view(page * PAGE, PAGE as usize) == Some(&buf[..]) {
                continue;
            }
            if !self.erase_pages(page, page + 1) || !self.f.write(page * PAGE, &buf) {
                break;
            }
        }
        let (from, to) = patlog::sweep_pages(packed, old_end);
        let _ = self.erase_pages(from, to);
        self.reload();
        true
    }

    /// `patterns.rs::save` — upsert by name.
    pub fn save(&mut self, p: &Pat) -> Result<u32, String> {
        if p.name.is_empty() || p.name.len() > patlog::MAX_NAME {
            return Err("name must be 1..=64 bytes".into());
        }
        let old = self.rec_by_name(&p.name);
        let seq = match &old {
            Some(r) => r.seq,
            None => {
                if self.overfull || self.index.len() >= MAX_RECS {
                    return Err("the device library is full".into());
                }
                self.next_seq
            }
        };
        if self.overfull {
            return Err("the device library is full".into());
        }
        // Whether the name was already stored — not the same question as
        // whether `old` still holds a record after a compaction.
        let existed = old.is_some();

        let size = p.size();
        let mut old = old;
        let mut off = patlog::place(&mut self.f, self.cursor, size);
        if off.is_none() {
            if self.compact(size) {
                off = patlog::place(&mut self.f, self.cursor, size);
            }
            // A compaction moved every unpinned file: `old` is a stale
            // address now. Take its new home from the rebuilt index.
            if old.is_some() {
                old = self.index.iter().find(|r| r.seq == seq).copied();
            }
        }
        let Some(off) = off else {
            return Err("the device's pattern storage is full".into());
        };

        let rec = Rec {
            off,
            stamp: self.next_stamp,
            seq,
            src_len: p.src.len() as u32,
            bc_len: p.bc.len() as u32,
            src_hash: patlog::fnv1a(&p.src),
            bc_hash: patlog::fnv1a(&p.bc),
            name_len: p.name.len() as u8,
            dead: false,
        };
        for step in patlog::append_plan(&rec) {
            let ok = match step {
                Step::Erase(a, b) => self.erase_pages(a, b),
                Step::Header(at) => {
                    let mut hdr = [0u8; patlog::HDR_PREFIX];
                    patlog::encode_header(&rec, p.name.as_bytes(), &mut hdr);
                    self.f.write(at, &hdr)
                }
                Step::Name(at) => self.f.write(at, p.name.as_bytes()),
                Step::Src(at) => self.f.write(at, &p.src),
                Step::Bc(at) => self.f.write(at, &p.bc),
                Step::Commit(at) => {
                    let good = patlog::hash_range(&mut self.f, rec.src_off(), rec.src_len)
                        == Some(rec.src_hash)
                        && patlog::hash_range(&mut self.f, rec.bc_off(), rec.bc_len)
                            == Some(rec.bc_hash);
                    good && self.f.write(at, &patlog::COMMIT.to_le_bytes())
                }
            };
            if !ok {
                return Err("couldn't write the pattern to flash".into());
            }
        }

        match self.index.iter_mut().find(|r| r.seq == seq) {
            Some(slot) => *slot = rec,
            None => self.index.push(rec),
        }
        sort_by_off(&mut self.index);
        self.cursor = rec.end();
        self.next_stamp = self.next_stamp.wrapping_add(1);
        if !existed {
            self.next_seq = seq.wrapping_add(1);
        }
        if let Some(o) = old {
            self.mark_dead(&o);
        }
        Ok(seq)
    }

    /// `patterns.rs::delete`.
    pub fn delete(&mut self, name: &str) -> bool {
        let Some(r) = self.rec_by_name(name) else { return false };
        if !self.mark_dead(&r) {
            return false;
        }
        self.index.retain(|x| x.seq != r.seq);
        true
    }

    /// What the log itself says is stored, by name — the ground truth a
    /// reboot would rebuild, with every payload re-hashed.
    pub fn on_flash(&mut self) -> Vec<(String, Vec<u8>, Vec<u8>)> {
        let mut best: Vec<(Rec, String)> = Vec::new();
        let mut found: Vec<(Rec, String)> = Vec::new();
        patlog::scan(&mut self.f, &mut |r: &Rec, n: &[u8]| {
            found.push((*r, String::from_utf8_lossy(n).into_owned()))
        });
        for (r, n) in found {
            if r.dead {
                continue;
            }
            match best.iter_mut().find(|(b, _)| b.seq == r.seq) {
                Some(slot) => {
                    if r.stamp > slot.0.stamp {
                        *slot = (r, n);
                    }
                }
                None => best.push((r, n)),
            }
        }
        best.sort_by_key(|(r, _)| r.off);
        best.into_iter()
            .map(|(r, n)| {
                let src = self.f.mem[r.src_off() as usize..(r.src_off() + r.src_len) as usize].to_vec();
                let bc = self.f.mem[r.bc_off() as usize..(r.bc_off() + r.bc_len) as usize].to_vec();
                (n, src, bc)
            })
            .collect()
    }
}

impl Default for Store {
    fn default() -> Store {
        Store::new()
    }
}

#[cfg(test)]
mod tests;
