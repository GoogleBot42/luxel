//! The pattern store's on-flash format: a packed, append-only log of
//! self-describing files in the mapped extent region (Gitea #340).
//!
//! # Why not pages
//!
//! Gitea #330 replaced the fixed 40 KiB slots with a *page-granular* extent
//! allocator: every blob took a whole number of 4 KiB erase pages, and the
//! whole directory was ONE `sequential-storage` item, whose one-page cap is
//! what pinned the store to 32 patterns. Jeremy's #340: *"We should have
//! properly sized files instead (the size required is known after all) just
//! held sequentially in memory to exact size. Adding a new file is easy,
//! just add it right after the last file. It may mean walking a linked list
//! to get to a desired file or to enumerate the existing files but that's
//! ok."*
//!
//! Both halves of that are this module. A **file** (one stored pattern:
//! header + name + source text + LXBC) occupies exactly the bytes it needs,
//! rounded only to 4, and the next file starts immediately after. The log
//! is self-describing — there is no directory anywhere — so boot enumerates
//! by walking the headers through the flash mapping and the pattern count is
//! bounded by *bytes*, not by a table.
//!
//! 4 is both the floor and the ceiling of the alignment tax:
//! `bytecode::deserialize_lean_static` borrows a blob's word region only
//! when it is 4-byte aligned in memory (it silently copies otherwise), and
//! `esp-storage`'s `WRITE_SIZE` is 4 — every flash write is a 4-byte-aligned
//! offset and a multiple-of-4 length, no more and no less.
//!
//! # The record
//!
//! ```text
//!  0  u32 magic       "PXL1"
//!  4  u32 self_off    this record's own arena offset -- the resync anchor
//!  8  u32 stamp       monotonic; the highest stamp wins for a seq
//! 12  u32 seq         pattern identity (API id = seq ^ ID_MASK)
//! 16  u32 src_len     source text, exact bytes
//! 20  u32 bc_len      LXBC, exact bytes
//! 24  u32 src_hash    FNV-1a of the source
//! 28  u32 bc_hash     FNV-1a of the bytecode
//! 32  u8  name_len | u8 ver | u16 0
//! 36  u32 hdr_hash    FNV-1a of bytes 0..36 ++ the name
//! --- written LAST, and what makes the record real -------------------
//! 40  u32 commit      COMMIT, or 0xFFFFFFFF while the record is torn
//! --- written by a delete or a re-save (NOR 1 -> 0, no erase) --------
//! 44  u32 dead        0xFFFFFFFF while live
//! -------------------------------------------------------------------
//! 48      name bytes, padded to 4
//!         source bytes, padded to 4
//!         bytecode bytes, padded to 4   <- 4-aligned, so the VM borrows it
//! ```
//!
//! # Power-cut safety
//!
//! One append is `erase the pages it will touch` → `header prefix (magic
//! first)` → `name` → `source` → `bytecode` → `commit`. A cut anywhere
//! before the commit word leaves a record the scan refuses; a cut anywhere
//! after it leaves a whole one. The payload hashes are checked at every boot,
//! so a record that got its commit word but not all of its bytes (impossible
//! in this order, but not in a compaction's page write) is refused too.
//!
//! `self_off` + `hdr_hash` are what make recovery *local*. The scan walks
//! record to record by length; when it lands on something that is not a
//! record it steps forward 4 bytes at a time until it finds one again. A
//! header only validates at the offset it was written for, so a stale copy
//! that a compaction left behind is still readable at its old home while the
//! new copy is readable at its new one, and a false positive would have to
//! forge a 32-bit hash over its own address. The worst a cut can cost is the
//! one record that was being written.
//!
//! # Freeing
//!
//! An erase unit is still 4 KiB while a file is now byte-sized, so a delete
//! frees nothing: it writes the `dead` word (NOR clears bits without an
//! erase; this build uses plain `FlashStorage`, never the encrypted one, so
//! a sub-page byte write is legal) and the bytes stay. Space comes back only
//! from a **compaction**, which repacks the live records toward offset 0 and
//! rewrites the log one destination page at a time — see [plan] for the
//! placement rule and `patterns.rs` for the executor.
//!
//! This module is `no_std`, allocation-free, panic-free and host-tested
//! (`tools/patlog-check`, `cargo test --workspace`): the flash simulator in
//! its test suite models NOR semantics (erase sets 0xFF, a write only clears
//! bits) and cuts power at every write boundary of every operation.

/// Erase-page size: the unit of *reclaim*, not of allocation.
pub const PAGE: u32 = 4096;
/// Fixed part of a record header.
pub const HDR: u32 = 48;
/// The header's first write: magic .. hdr_hash. `commit` and `dead` are
/// left erased and written later.
pub const HDR_PREFIX: usize = 40;
/// Longest pattern name (bytes).
pub const MAX_NAME: usize = 64;
/// Record-format version. Bumping it makes every older record stop being a
/// record, so a format change costs no boot-time erase of the arena: the
/// scan finds nothing and the first append erases the pages it lands on.
pub const VER: u8 = 1;
/// Largest source text the store accepts. Unchanged from #330.
pub const MAX_SOURCE: u32 = 32 * 1024;
/// Largest LXBC the store accepts. LXBC can run larger than its source.
pub const MAX_BC: u32 = 40 * 1024;

/// `"PXL1"` little-endian.
pub const MAGIC: u32 = 0x314C_5850;
/// The commit word's value once a record is whole.
pub const COMMIT: u32 = 0x4B4F_4B4F; // "OKOK"
/// Erased NOR.
pub const ERASED: u32 = 0xFFFF_FFFF;
/// What a delete writes into the `dead` word.
pub const DEAD: u32 = 0x0000_0000;

const O_MAGIC: usize = 0;
const O_SELF: usize = 4;
const O_STAMP: usize = 8;
const O_SEQ: usize = 12;
const O_SRC_LEN: usize = 16;
const O_BC_LEN: usize = 20;
const O_SRC_HASH: usize = 24;
const O_BC_HASH: usize = 28;
const O_NAME_LEN: usize = 32;
const O_VER: usize = 33;
const O_HDR_HASH: usize = 36;
/// Offset of the commit word inside a header.
pub const O_COMMIT: u32 = 40;
/// Offset of the dead word inside a header.
pub const O_DEAD: u32 = 44;

/// Round up to the 4-byte write granularity.
pub const fn align4(n: u32) -> u32 {
    (n + 3) & !3
}
/// Round up to the erase page.
pub const fn align_page(n: u32) -> u32 {
    (n + PAGE - 1) & !(PAGE - 1)
}

const FNV_INIT: u32 = 0x811c_9dc5;

/// Incremental FNV-1a — the same function as `luxel_core::netin::fnv1a`,
/// which is what the rest of the store already hashes with.
pub fn fnv1a_update(mut h: u32, bytes: &[u8]) -> u32 {
    for &b in bytes {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h
}

pub fn fnv1a(bytes: &[u8]) -> u32 {
    fnv1a_update(FNV_INIT, bytes)
}

fn rd(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

// ---------------------------------------------------------------------------

/// One stored file's header, as the scan understood it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rec {
    /// Arena offset the record starts at (== its `self_off` field).
    pub off: u32,
    /// Monotonic write stamp. For one `seq`, the highest stamp is current.
    pub stamp: u32,
    /// Pattern identity.
    pub seq: u32,
    pub src_len: u32,
    pub bc_len: u32,
    pub src_hash: u32,
    pub bc_hash: u32,
    pub name_len: u8,
    /// The `dead` word has been written: superseded or deleted.
    pub dead: bool,
}

impl Rec {
    pub const fn name_off(&self) -> u32 {
        self.off + HDR
    }
    pub const fn src_off(&self) -> u32 {
        self.name_off() + align4(self.name_len as u32)
    }
    pub const fn bc_off(&self) -> u32 {
        self.src_off() + align4(self.src_len)
    }
    pub const fn end(&self) -> u32 {
        self.bc_off() + align4(self.bc_len)
    }
    pub const fn size(&self) -> u32 {
        self.end() - self.off
    }
    pub const fn commit_off(&self) -> u32 {
        self.off + O_COMMIT
    }
    pub const fn dead_off(&self) -> u32 {
        self.off + O_DEAD
    }
    /// Total bytes a record of these dimensions occupies.
    pub const fn bytes(name_len: u8, src_len: u32, bc_len: u32) -> u32 {
        HDR + align4(name_len as u32) + align4(src_len) + align4(bc_len)
    }
    /// Rejects a header whose fields could not have come from this store.
    pub fn plausible(&self) -> bool {
        self.name_len as usize >= 1
            && self.name_len as usize <= MAX_NAME
            && self.src_len >= 1
            && self.src_len <= MAX_SOURCE
            && self.bc_len >= 1
            && self.bc_len <= MAX_BC
    }
}

/// Write the header's first 40 bytes for `rec` (which must carry its final
/// `off`) with `name`. `commit` and `dead` are NOT part of this write.
pub fn encode_header(rec: &Rec, name: &[u8], out: &mut [u8; HDR_PREFIX]) {
    out.fill(0);
    out[O_MAGIC..O_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
    out[O_SELF..O_SELF + 4].copy_from_slice(&rec.off.to_le_bytes());
    out[O_STAMP..O_STAMP + 4].copy_from_slice(&rec.stamp.to_le_bytes());
    out[O_SEQ..O_SEQ + 4].copy_from_slice(&rec.seq.to_le_bytes());
    out[O_SRC_LEN..O_SRC_LEN + 4].copy_from_slice(&rec.src_len.to_le_bytes());
    out[O_BC_LEN..O_BC_LEN + 4].copy_from_slice(&rec.bc_len.to_le_bytes());
    out[O_SRC_HASH..O_SRC_HASH + 4].copy_from_slice(&rec.src_hash.to_le_bytes());
    out[O_BC_HASH..O_BC_HASH + 4].copy_from_slice(&rec.bc_hash.to_le_bytes());
    out[O_NAME_LEN] = rec.name_len;
    out[O_VER] = VER;
    let h = fnv1a_update(fnv1a(&out[..O_HDR_HASH]), &name[..rec.name_len as usize]);
    out[O_HDR_HASH..O_HDR_HASH + 4].copy_from_slice(&h.to_le_bytes());
}

/// Parse a committed record at `off`. `b` starts at `off` and should hold at
/// least `HDR + MAX_NAME` bytes (fewer is fine near the arena's end — a
/// record whose name does not fit the slice is simply not recognised).
///
/// Returns the record and its name. `None` covers erased flash, a torn
/// header, a header written for a different offset, and an uncommitted one.
pub fn parse_header(off: u32, b: &[u8]) -> Option<(Rec, &[u8])> {
    if b.len() < HDR as usize {
        return None;
    }
    if rd(b, O_MAGIC) != MAGIC || rd(b, O_SELF) != off {
        return None;
    }
    if b[O_VER] != VER {
        return None;
    }
    let name_len = b[O_NAME_LEN];
    let nl = name_len as usize;
    if nl == 0 || nl > MAX_NAME || b.len() < HDR as usize + nl {
        return None;
    }
    let name = &b[HDR as usize..HDR as usize + nl];
    if fnv1a_update(fnv1a(&b[..O_HDR_HASH]), name) != rd(b, O_HDR_HASH) {
        return None;
    }
    if rd(b, O_COMMIT as usize) != COMMIT {
        return None;
    }
    let rec = Rec {
        off,
        stamp: rd(b, O_STAMP),
        seq: rd(b, O_SEQ),
        src_len: rd(b, O_SRC_LEN),
        bc_len: rd(b, O_BC_LEN),
        src_hash: rd(b, O_SRC_HASH),
        bc_hash: rd(b, O_BC_HASH),
        name_len,
        dead: rd(b, O_DEAD as usize) != ERASED,
    };
    if !rec.plausible() {
        return None;
    }
    Some((rec, name))
}

// ---------------------------------------------------------------------------

/// Read-side view of the arena. The mapped implementation hands back the
/// flash mapping itself (no copy); the `flashmap-off` one reads through the
/// flash controller into a page buffer.
pub trait Arena {
    /// Arena length in bytes.
    fn len(&self) -> u32;
    /// Up to `want` bytes at `off`. May return fewer (near the end, or at a
    /// buffer boundary) but never zero unless `off >= len()`. `None` = a
    /// read failure.
    fn view(&mut self, off: u32, want: usize) -> Option<&[u8]>;
}

/// FNV-1a of `len` arena bytes at `off`, streamed through whatever the
/// [Arena] gives us.
pub fn hash_range<A: Arena + ?Sized>(a: &mut A, off: u32, len: u32) -> Option<u32> {
    if off.checked_add(len)? > a.len() {
        return None;
    }
    let mut h = FNV_INIT;
    let mut at = off;
    let end = off + len;
    while at < end {
        let v = a.view(at, (end - at) as usize)?;
        if v.is_empty() {
            return None;
        }
        let n = v.len().min((end - at) as usize);
        h = fnv1a_update(h, &v[..n]);
        at += n as u32;
    }
    Some(h)
}

/// Is every byte of `[off, off+len)` still erased (0xFF)?
pub fn erased<A: Arena + ?Sized>(a: &mut A, off: u32, len: u32) -> bool {
    if len == 0 {
        return true;
    }
    if off.checked_add(len).map(|e| e > a.len()).unwrap_or(true) {
        return false;
    }
    let mut at = off;
    let end = off + len;
    while at < end {
        let Some(v) = a.view(at, (end - at) as usize) else {
            return false;
        };
        if v.is_empty() {
            return false;
        }
        let n = v.len().min((end - at) as usize);
        if v[..n].iter().any(|&b| b != 0xFF) {
            return false;
        }
        at += n as u32;
    }
    true
}

/// What one scan of the arena found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// First byte past the highest record the scan accepted — where the
    /// next append goes (subject to [place]).
    pub cursor: u32,
    /// Bytes held by accepted records that are live.
    pub live: u32,
    /// Bytes held by accepted records that are dead.
    pub dead: u32,
    /// Records accepted (live + dead).
    pub recs: u32,
    /// Records whose header was whole and committed but whose payload did
    /// not hash — a page write cut in half, or a stale header a frozen page
    /// preserved past the data it described. Its length is not trusted: the
    /// walk resyncs through it rather than stepping over it (Gitea #379).
    pub torn: u32,
    /// 4-byte steps spent resynchronising after something that was not a
    /// record. Zero on a clean store.
    pub resync: u32,
}

/// Walk the log to its end. `emit` sees every committed record whose
/// payload hashes, live and dead alike, in ascending offset order, with its
/// name.
///
/// Records are found by walking header to header; anything that is not a
/// record costs a 4-byte step (see the module docs on `self_off`).
///
/// The walk is deliberately **uncapped**: `cursor` has to be the end of the
/// LAST record in the log or an append would land on top of live ones, so
/// a caller who can only index so many records caps what it keeps, never
/// what this walks.
///
/// `emit` is `dyn` on purpose: the firmware calls this from two places with
/// different closures, and one monomorphization of a whole arena walk is
/// worth more than the indirect calls (OTA slot, #310).
pub fn scan(a: &mut dyn Arena, emit: &mut dyn FnMut(&Rec, &[u8])) -> Scan {
    let mut s = Scan::default();
    let len = a.len();
    let mut at = 0u32;
    let mut namebuf = [0u8; MAX_NAME];
    while at + HDR <= len {
        let mut found: Option<Rec> = None;
        let mut erased_here = false;
        if let Some(v) = a.view(at, HDR as usize + MAX_NAME) {
            // Cheap probe first: most of the arena is erased tail, and a
            // 4-byte compare per step keeps that sweep to a couple of
            // milliseconds through the mapping.
            let head = rd(v, 0);
            erased_here = head == ERASED;
            if head == MAGIC {
                if let Some((rec, name)) = parse_header(at, v) {
                    if rec.end() <= len {
                        namebuf[..name.len()].copy_from_slice(name);
                        found = Some(rec);
                    }
                }
            }
        }
        let Some(rec) = found else {
            at += 4;
            if !erased_here {
                // junk, not erased flash: a torn tail, or the seam a cut
                // compaction left. Counted so boot can say so.
                s.resync += 1;
            }
            continue;
        };
        let whole = hash_range(a, rec.src_off(), rec.src_len) == Some(rec.src_hash)
            && hash_range(a, rec.bc_off(), rec.bc_len) == Some(rec.bc_hash);
        if !whole {
            // A header whose payload does not hash is NOT a record, so its
            // length field is not a length: it describes bytes that are no
            // longer the ones it was written for. Stepping over `end()`
            // here would jump the walk over whatever really lives in that
            // span — and after a compaction that span is exactly where the
            // repacked files went, so every file under a stale header
            // vanished from the store (Gitea #379). Resync instead: 4 bytes
            // at a time, like any other junk, so a real record inside is
            // still found. Its bytes are not counted anywhere; whatever the
            // walk does not attribute to a record is reclaimable space, and
            // `patterns.rs` derives that from the cursor.
            s.torn += 1;
            s.resync += 1;
            at = rec.off + 4;
            continue;
        }
        s.recs += 1;
        if rec.dead {
            s.dead += rec.size();
        } else {
            s.live += rec.size();
        }
        emit(&rec, &namebuf[..rec.name_len as usize]);
        at = rec.end();
        s.cursor = at;
    }
    s
}

/// Where an append of `size` bytes may start, given the scan's `cursor`.
///
/// The cursor's own page may hold live records below it, so it can never be
/// erased: the bytes the record would take there have to be erased already.
/// They normally are — that is how the scan found the cursor — but a torn
/// tail leaves junk, and then the record starts at the next page boundary
/// instead (whole pages past the cursor's are always erasable: nothing live
/// can be up there, since the log packs from 0).
pub fn place<A: Arena + ?Sized>(a: &mut A, cursor: u32, size: u32) -> Option<u32> {
    let mut at = align4(cursor);
    loop {
        if at.checked_add(size)? > a.len() {
            return None;
        }
        if at % PAGE == 0 {
            return Some(at);
        }
        let page_end = align_page(at + 1);
        let head = size.min(page_end - at);
        if erased(a, at, head) {
            return Some(at);
        }
        at = page_end;
    }
}

/// One write of an append, in the order they must happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Erase arena pages `[first, last)` — the whole pages the record will
    /// occupy past its own first, partial one. Skip pages already erased.
    Erase(u32, u32),
    /// The 40-byte header prefix at this offset. Magic goes down first, so
    /// a cut inside this write leaves either nothing or an unverifiable
    /// header — never a usable one.
    Header(u32),
    /// The name, padded to 4.
    Name(u32),
    /// The source text, padded to 4.
    Src(u32),
    /// The bytecode, padded to 4.
    Bc(u32),
    /// The commit word. LAST: this is what publishes the record.
    Commit(u32),
}

/// The canonical write order of one append. The firmware executes these
/// asynchronously through the fenced flash door; the host tests execute them
/// against a NOR simulator and cut power between (and inside) each one.
pub fn append_plan(rec: &Rec) -> [Step; 6] {
    [
        Step::Erase(align_page(rec.off) / PAGE, align_page(rec.end()) / PAGE),
        Step::Header(rec.off),
        Step::Name(rec.name_off()),
        Step::Src(rec.src_off()),
        Step::Bc(rec.bc_off()),
        Step::Commit(rec.commit_off()),
    ]
}

// ---------------------------------------------------------------------------
// compaction placement

/// Where a record ends up when the log is repacked. `to == from` means it
/// does not move (it is pinned, or there was nothing below it to reclaim).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub idx: usize,
    pub from: u32,
    pub to: u32,
    pub size: u32,
}

impl Place {
    /// Used by the host suite's assertions; the firmware never asks.
    #[allow(dead_code)]
    pub const fn moved(&self) -> bool {
        self.to != self.from
    }
}

/// Does `[off, off+size)` touch a page any record in `keep[..]` that is
/// pinned occupies?
fn hits_frozen(keep: &[Rec], pinned: &[u32], off: u32, size: u32) -> Option<u32> {
    let (lo, hi) = (off & !(PAGE - 1), align_page(off + size));
    keep.iter()
        .filter(|r| pinned.contains(&r.seq))
        .find(|r| {
            let (a, b) = (r.off & !(PAGE - 1), align_page(r.end()));
            a < hi && lo < b
        })
        .map(|r| align_page(r.end()))
}

/// Plan a compaction: pack `keep` (every record that must survive, sorted
/// by ascending offset) toward offset 0, checking the plan as it builds it.
///
/// Rules, in order of importance:
///
/// 1. **A pinned record never moves and its pages are never erased.** An
///    engine may be executing its bytecode in place (`patterns.rs`' pin set,
///    Gitea #260), so those pages are frozen and every other record is
///    placed around them: one that cannot pack down below a frozen page is
///    placed *after* it, or left exactly where it is if it is already past
///    it. Never dropped — a compaction is a total function over `keep`.
/// 2. **Nothing ever moves up.** The executor rewrites destination pages in
///    ascending order and every byte it needs for page P lives at an offset
///    ≥ P's start, which is exactly what makes an overlapping repack safe.
///
/// `Some(packed length)` — the new write cursor — when the plan holds all
/// of that: every record placed once, in order, no two overlapping, a
/// pinned one at its own address, and a *moved* one never landing in a page
/// [build_page] will skip. `None` means it does not, and then the caller
/// must erase nothing and fail the operation loudly: proceeding on a plan
/// that does not place every record is how Gitea #379 lost files silently.
/// `out` has already seen the [Place]s produced up to that point; on `None`
/// they are not a plan and must be discarded.
pub fn plan(keep: &[Rec], pinned: &[u32], out: &mut dyn FnMut(Place)) -> Option<u32> {
    let mut dst = 0u32;
    let mut cursor = 0u32;
    for (idx, r) in keep.iter().enumerate() {
        let size = r.size();
        let is_pinned = pinned.contains(&r.seq);
        let mut to = if is_pinned {
            r.off
        } else {
            let mut t = dst;
            // step over any frozen page range standing in the way
            for _ in 0..keep.len() + 1 {
                match hits_frozen(keep, pinned, t, size) {
                    Some(after) if after > t => t = after,
                    _ => break,
                }
            }
            if t > r.off {
                r.off // cannot pack down past a frozen page: stay put
            } else {
                t
            }
        };
        if to > r.off {
            to = r.off;
        }
        // The plan check. Cheap, and the only thing standing between a
        // mis-sorted or overlapping `keep` and a compaction that writes one
        // file over another.
        if to < dst || (!is_pinned && to != r.off && hits_frozen(keep, pinned, to, size).is_some())
        {
            return None;
        }
        out(Place { idx, from: r.off, to, size });
        dst = align4(to + size);
        if dst > cursor {
            cursor = dst;
        }
    }
    Some(cursor)
}

/// Is this erase page one a pinned record occupies? The executor must never
/// erase one: an engine is executing bytecode out of it.
pub fn frozen_page(keep: &[Rec], pinned: &[u32], page: u32) -> bool {
    let (p0, p1) = (page * PAGE, page * PAGE + PAGE);
    keep.iter()
        .filter(|r| pinned.contains(&r.seq))
        .any(|r| r.off < p1 && p0 < r.end())
}

/// Build the post-compaction content of erase page `page` into `out`
/// (exactly [PAGE] bytes). Header bytes are *regenerated* — `self_off` and
/// therefore `hdr_hash` change when a record moves — and payload bytes are
/// read from the record's current home. Bytes no record covers read 0xFF.
///
/// The caller must build pages in ascending order and must skip
/// [frozen_page]s. Every byte this needs lives at an offset ≥ the page's
/// start ([plan] never moves a record up), so buffering the page before
/// erasing it is the whole safety argument for an overlapping repack.
pub fn build_page<A: Arena + ?Sized>(
    a: &mut A,
    page: u32,
    keep: &[Rec],
    places: &[Place],
    out: &mut [u8],
) -> bool {
    if out.len() != PAGE as usize {
        return false;
    }
    out.fill(0xFF);
    let (p0, p1) = (page * PAGE, page * PAGE + PAGE);
    for pl in places {
        let Some(r) = keep.get(pl.idx) else { return false };
        let (n0, n1) = (pl.to, pl.to + pl.size);
        if n1 <= p0 || n0 >= p1 {
            continue;
        }
        // header: regenerated, never copied
        let nl = r.name_len as usize;
        let mut name = [0u8; MAX_NAME];
        match a.view(r.name_off(), nl) {
            Some(v) if v.len() >= nl => name[..nl].copy_from_slice(&v[..nl]),
            _ => return false,
        }
        let mut hdr = [0u8; HDR as usize];
        let mut prefix = [0u8; HDR_PREFIX];
        encode_header(&Rec { off: pl.to, ..*r }, &name, &mut prefix);
        hdr[..HDR_PREFIX].copy_from_slice(&prefix);
        hdr[O_COMMIT as usize..O_COMMIT as usize + 4].copy_from_slice(&COMMIT.to_le_bytes());
        let d = if r.dead { DEAD } else { ERASED };
        hdr[O_DEAD as usize..O_DEAD as usize + 4].copy_from_slice(&d.to_le_bytes());
        for i in 0..HDR {
            let at = n0 + i;
            if at >= p0 && at < p1 {
                out[(at - p0) as usize] = hdr[i as usize];
            }
        }
        // payload (name + source + bytecode + their padding): a straight copy
        let body0 = (n0 + HDR).max(p0);
        let body1 = n1.min(p1);
        let mut at = body0;
        while at < body1 {
            let src = at - pl.to + pl.from;
            let want = (body1 - at) as usize;
            let Some(v) = a.view(src, want) else { return false };
            if v.is_empty() {
                return false;
            }
            let n = v.len().min(want);
            out[(at - p0) as usize..(at - p0) as usize + n].copy_from_slice(&v[..n]);
            at += n as u32;
        }
    }
    true
}

/// The pages a finished compaction still has to erase: every whole page
/// above the new cursor that the old log reached. They hold nothing but the
/// stale copies of the records the repack moved down — self-consistent
/// headers at their old addresses, which is exactly what makes a *cut*
/// compaction recoverable and what a *finished* one must clean up, or the
/// next boot's scan would keep finding them and the cursor would never come
/// back down.
///
/// This runs LAST, after every destination page is written: until then
/// those bytes are still the source data.
pub fn sweep_pages(new_cursor: u32, old_cursor: u32) -> (u32, u32) {
    (align_page(new_cursor) / PAGE, align_page(old_cursor) / PAGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A NOR flash: erase sets 0xFF, a write only ever clears bits, and
    /// every 4-byte word (and every page erase) is a step power can die
    /// between. `budget` is how many of those steps this power supply has
    /// left.
    struct Sim {
        mem: Vec<u8>,
        budget: usize,
        ops: usize,
        dead: bool,
        erases: usize,
        cur: (u32, u32), // the view buffer's coverage, unused for the mapped sim
    }

    impl Sim {
        fn new(len: u32) -> Sim {
            Sim {
                mem: vec![0xFF; len as usize],
                budget: usize::MAX,
                ops: 0,
                dead: false,
                erases: 0,
                cur: (0, 0),
            }
        }
        fn step(&mut self) -> bool {
            if self.dead {
                return false;
            }
            if self.ops >= self.budget {
                self.dead = true;
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
        /// `off` and `data.len()` must be 4-aligned (esp-storage's contract).
        fn write(&mut self, off: u32, data: &[u8]) -> bool {
            assert_eq!(off % 4, 0, "write offset must be word aligned");
            assert_eq!(data.len() % 4, 0, "write length must be a whole number of words");
            for (i, chunk) in data.chunks(4).enumerate() {
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
        fn word(&self, off: u32) -> u32 {
            rd(&self.mem, off as usize)
        }
    }

    impl Arena for Sim {
        fn len(&self) -> u32 {
            self.mem.len() as u32
        }
        fn view(&mut self, off: u32, want: usize) -> Option<&[u8]> {
            let _ = self.cur;
            let at = off as usize;
            if at >= self.mem.len() {
                return None;
            }
            let end = (at + want).min(self.mem.len());
            Some(&self.mem[at..end])
        }
    }

    /// A pattern to store.
    #[derive(Clone)]
    struct Pat {
        seq: u32,
        name: String,
        src: Vec<u8>,
        bc: Vec<u8>,
    }

    fn pat(seq: u32, name: &str, src_len: usize, bc_len: usize) -> Pat {
        let src: Vec<u8> = (0..src_len).map(|i| b'a' + ((i * 7 + seq as usize) % 26) as u8).collect();
        let bc: Vec<u8> = (0..bc_len).map(|i| (i * 31 + seq as usize * 13) as u8).collect();
        Pat { seq, name: name.to_string(), src, bc }
    }

    fn rec_of(p: &Pat, off: u32, stamp: u32) -> Rec {
        Rec {
            off,
            stamp,
            seq: p.seq,
            src_len: p.src.len() as u32,
            bc_len: p.bc.len() as u32,
            src_hash: fnv1a(&p.src),
            bc_hash: fnv1a(&p.bc),
            name_len: p.name.len() as u8,
            dead: false,
        }
    }

    fn padded(b: &[u8]) -> Vec<u8> {
        let mut v = b.to_vec();
        while v.len() % 4 != 0 {
            v.push(0xFF);
        }
        v
    }

    /// The append, executed exactly in [append_plan] order. This is the
    /// reference the firmware's async executor mirrors.
    fn append(sim: &mut Sim, p: &Pat, cursor: u32, stamp: u32) -> Option<Rec> {
        let size = Rec::bytes(p.name.len() as u8, p.src.len() as u32, p.bc.len() as u32);
        let off = place(sim, cursor, size)?;
        let rec = rec_of(p, off, stamp);
        for step in append_plan(&rec) {
            let ok = match step {
                Step::Erase(a, b) => {
                    let mut ok = true;
                    for pg in a..b {
                        if !erased(sim, pg * PAGE, PAGE) && !sim.erase(pg) {
                            ok = false;
                            break;
                        }
                    }
                    ok
                }
                Step::Header(at) => {
                    let mut hdr = [0u8; HDR_PREFIX];
                    encode_header(&rec, p.name.as_bytes(), &mut hdr);
                    sim.write(at, &hdr)
                }
                Step::Name(at) => sim.write(at, &padded(p.name.as_bytes())),
                Step::Src(at) => sim.write(at, &padded(&p.src)),
                Step::Bc(at) => sim.write(at, &padded(&p.bc)),
                Step::Commit(at) => sim.write(at, &COMMIT.to_le_bytes()),
            };
            if !ok {
                return None;
            }
        }
        Some(rec)
    }

    fn mark_dead(sim: &mut Sim, rec: &Rec) -> bool {
        sim.write(rec.dead_off(), &DEAD.to_le_bytes())
    }

    /// Everything the scan accepts, in offset order.
    fn all(sim: &mut Sim) -> (Vec<(Rec, String)>, Scan) {
        let mut out = Vec::new();
        let s = scan(sim, &mut |r: &Rec, n: &[u8]| {
            out.push((*r, String::from_utf8_lossy(n).into_owned()))
        });
        (out, s)
    }

    /// The RAM index the firmware builds: one live record per seq, the one
    /// with the highest stamp (ties go to the lowest offset — a compaction's
    /// new copy sits below the stale one it has not overwritten yet).
    fn index(sim: &mut Sim) -> Vec<(Rec, String)> {
        let (recs, _) = all(sim);
        let mut best: Vec<(Rec, String)> = Vec::new();
        for (r, n) in recs {
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
        best
    }

    fn read(sim: &Sim, off: u32, len: u32) -> Vec<u8> {
        sim.mem[off as usize..(off + len) as usize].to_vec()
    }

    // --- format ---------------------------------------------------------

    #[test]
    fn header_round_trips_and_is_offset_bound() {
        let p = pat(3, "sunset", 100, 200);
        let rec = rec_of(&p, 4 * 4096 + 64, 9);
        let mut hdr = [0u8; HDR_PREFIX];
        encode_header(&rec, p.name.as_bytes(), &mut hdr);
        let mut buf = vec![0xFFu8; HDR as usize + MAX_NAME];
        buf[..HDR_PREFIX].copy_from_slice(&hdr);
        buf[O_COMMIT as usize..O_COMMIT as usize + 4].copy_from_slice(&COMMIT.to_le_bytes());
        buf[HDR as usize..HDR as usize + p.name.len()].copy_from_slice(p.name.as_bytes());
        let (got, name) = parse_header(rec.off, &buf).expect("parses at its own offset");
        assert_eq!(got, rec);
        assert_eq!(name, p.name.as_bytes());
        // the same bytes anywhere else are not a record
        assert!(parse_header(rec.off + 4, &buf).is_none());
        assert!(parse_header(0, &buf).is_none());
        // no commit word: not a record yet
        let mut torn = buf.clone();
        torn[O_COMMIT as usize..O_COMMIT as usize + 4].copy_from_slice(&ERASED.to_le_bytes());
        assert!(parse_header(rec.off, &torn).is_none());
        // a flipped payload-length bit breaks the header hash
        let mut bad = buf.clone();
        bad[O_SRC_LEN] ^= 1;
        assert!(parse_header(rec.off, &bad).is_none());
    }

    #[test]
    fn layout_is_exact_and_four_byte_aligned() {
        // 1-byte source, 1-byte bytecode, 1-byte name: the whole file is
        // one header plus three words, not three erase pages.
        let r = Rec { off: 0, name_len: 1, src_len: 1, bc_len: 1, ..Default::default() };
        assert_eq!(r.size(), HDR + 4 + 4 + 4);
        // the real median: 2,852 B source, 3 KB bytecode, 12-char name
        let r = Rec { off: 0, name_len: 12, src_len: 2852, bc_len: 3072, ..Default::default() };
        assert_eq!(r.size(), 48 + 12 + 2852 + 3072);
        assert_eq!(r.size() % 4, 0);
        // every payload offset is 4-aligned whatever the lengths
        for nl in 1..=MAX_NAME as u32 {
            for sl in [1u32, 2, 3, 4, 5, 4095, 4096, 4097] {
                let r = Rec { off: 4, name_len: nl as u8, src_len: sl, bc_len: 7, ..Default::default() };
                assert_eq!(r.name_off() % 4, 0);
                assert_eq!(r.src_off() % 4, 0);
                assert_eq!(r.bc_off() % 4, 0, "the VM borrows the bytecode in place");
                assert_eq!(r.end() % 4, 0);
            }
        }
    }

    // --- append / enumerate --------------------------------------------

    #[test]
    fn appends_pack_back_to_back() {
        let mut sim = Sim::new(64 * PAGE);
        let mut cursor = 0;
        let pats = [pat(0, "a", 100, 200), pat(1, "bb", 33, 7), pat(2, "ccc", 4000, 9000)];
        let mut recs = Vec::new();
        for (i, p) in pats.iter().enumerate() {
            let r = append(&mut sim, p, cursor, i as u32 + 1).expect("append fits");
            assert_eq!(r.off, cursor, "a new file starts right after the last one");
            cursor = r.end();
            recs.push(r);
        }
        let (found, s) = all(&mut sim);
        assert_eq!(found.len(), 3);
        assert_eq!(s.resync, 0, "a clean log needs no resynchronisation");
        assert_eq!(s.cursor, cursor);
        assert_eq!(s.live, recs.iter().map(|r| r.size()).sum::<u32>());
        assert_eq!(s.dead, 0);
        for (i, (r, n)) in found.iter().enumerate() {
            assert_eq!(*r, recs[i]);
            assert_eq!(n, &pats[i].name);
            assert_eq!(read(&sim, r.src_off(), r.src_len), pats[i].src);
            assert_eq!(read(&sim, r.bc_off(), r.bc_len), pats[i].bc);
        }
        // exact packing, not page rounding: three files, 13 KB of payload,
        // inside four erase pages
        assert!(cursor <= 4 * PAGE, "packed into {} B", cursor);
    }

    #[test]
    fn the_pattern_count_is_bounded_by_bytes_not_by_a_table() {
        let mut sim = Sim::new(183 * PAGE);
        let mut cursor = 0;
        let mut n = 0u32;
        loop {
            let p = pat(n, "tiny", 40, 60);
            match append(&mut sim, &p, cursor, n + 1) {
                Some(r) => {
                    cursor = r.end();
                    n += 1;
                }
                None => break,
            }
            assert!(n < 6000, "the arena must fill eventually");
        }
        // 183 pages / (48 + 4 + 40 -> 40 + 60 -> 60) = 4,000-odd files, and
        // the old store's ceiling was 32.
        assert!(n > 3000, "only {} tiny files fit", n);
        let (found, s) = all(&mut sim);
        assert_eq!(found.len() as u32, n);
        assert_eq!(s.recs, n);
    }

    #[test]
    fn dead_marks_survive_and_the_scan_reports_them() {
        let mut sim = Sim::new(16 * PAGE);
        let a = append(&mut sim, &pat(0, "a", 500, 500), 0, 1).unwrap();
        let b = append(&mut sim, &pat(1, "b", 500, 500), a.end(), 2).unwrap();
        assert!(mark_dead(&mut sim, &a));
        let (found, s) = all(&mut sim);
        assert_eq!(found.len(), 2, "a dead record is still a record");
        assert!(found[0].0.dead && !found[1].0.dead);
        assert_eq!(s.live, b.size());
        assert_eq!(s.dead, a.size());
        assert_eq!(index(&mut sim).len(), 1, "the index only holds live files");
        // the dead word is a 1 -> 0 write; nothing was erased for it
        assert_eq!(sim.erases, 0, "nothing needed erasing: the pages were already clean");
        assert_eq!(sim.word(a.dead_off()), DEAD);
    }

    #[test]
    fn a_resave_supersedes_by_stamp() {
        let mut sim = Sim::new(16 * PAGE);
        let v1 = pat(0, "shift", 300, 400);
        let mut v2 = v1.clone();
        v2.src = vec![b'z'; 900];
        v2.bc = vec![7u8; 111];
        let a = append(&mut sim, &v1, 0, 1).unwrap();
        let b = append(&mut sim, &v2, a.end(), 2).unwrap();
        // the window a power cut can land in: both live, same seq
        let idx = index(&mut sim);
        assert_eq!(idx.len(), 1);
        assert_eq!(idx[0].0, b, "the higher stamp wins");
        assert!(mark_dead(&mut sim, &a));
        let idx = index(&mut sim);
        assert_eq!(idx.len(), 1);
        assert_eq!(idx[0].0, b);
        assert_eq!(read(&sim, b.src_off(), b.src_len), v2.src);
    }

    #[test]
    fn a_torn_tail_costs_a_page_not_the_log() {
        let mut sim = Sim::new(16 * PAGE);
        let a = append(&mut sim, &pat(0, "keep", 200, 200), 0, 1).unwrap();
        // a header that never got its commit word, right after it
        let torn = pat(1, "torn", 300, 300);
        let rec = rec_of(&torn, a.end(), 2);
        let mut hdr = [0u8; HDR_PREFIX];
        encode_header(&rec, torn.name.as_bytes(), &mut hdr);
        assert!(sim.write(rec.off, &hdr));
        assert!(sim.write(rec.name_off(), &padded(torn.name.as_bytes())));
        let (found, s) = all(&mut sim);
        assert_eq!(found.len(), 1, "an uncommitted record is not a record");
        assert_eq!(s.cursor, a.end());
        // ...and the next append steps over the junk to the next page
        let next = append(&mut sim, &pat(2, "next", 100, 100), s.cursor, 3).unwrap();
        assert_eq!(next.off, PAGE, "packing resumes at the next erasable page");
        let (found, _) = all(&mut sim);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].0, next);
    }

    // --- power cuts ------------------------------------------------------

    /// The whole store, as a reader sees it: for every live file, its name
    /// and its exact bytes. Anything the scan accepted has already had both
    /// payload hashes checked, so this is a *consistency* check on top.
    fn snapshot(sim: &mut Sim) -> Vec<(u32, String, Vec<u8>, Vec<u8>)> {
        index(sim)
            .into_iter()
            .map(|(r, n)| {
                let src = sim.mem[r.src_off() as usize..(r.src_off() + r.src_len) as usize].to_vec();
                let bc = sim.mem[r.bc_off() as usize..(r.bc_off() + r.bc_len) as usize].to_vec();
                assert_eq!(fnv1a(&src), r.src_hash);
                assert_eq!(fnv1a(&bc), r.bc_hash);
                (r.seq, n, src, bc)
            })
            .collect()
    }

    #[test]
    fn a_cut_at_every_write_boundary_of_a_save_is_survivable() {
        let v1 = pat(0, "shift", 700, 900);
        let mut v2 = v1.clone();
        v2.src = vec![b'q'; 1500];
        v2.bc = vec![0x5A; 2200];
        let other = pat(1, "other", 300, 300);

        // how many steps a whole save takes, measured once
        let mut probe = Sim::new(16 * PAGE);
        let a = append(&mut probe, &v1, 0, 1).unwrap();
        let o = append(&mut probe, &other, a.end(), 2).unwrap();
        let before = probe.ops;
        let _b = append(&mut probe, &v2, o.end(), 3).unwrap();
        assert!(mark_dead(&mut probe, &a));
        let total = probe.ops - before;
        assert!(total > 900, "the sweep must have real granularity: {} steps", total);

        for cut in 0..=total {
            let mut sim = Sim::new(16 * PAGE);
            let a = append(&mut sim, &v1, 0, 1).unwrap();
            let o = append(&mut sim, &other, a.end(), 2).unwrap();
            sim.budget = sim.ops + cut;
            let saved = append(&mut sim, &v2, o.end(), 3);
            if saved.is_some() {
                mark_dead(&mut sim, &a);
            }

            let snap = snapshot(&mut sim);
            // the untouched pattern is always whole
            let others: Vec<_> = snap.iter().filter(|s| s.0 == 1).collect();
            assert_eq!(others.len(), 1, "cut {}: the bystander must survive", cut);
            assert_eq!(others[0].2, other.src);
            assert_eq!(others[0].3, other.bc);
            // the saved pattern is exactly one of its two versions, never a mix
            let mine: Vec<_> = snap.iter().filter(|s| s.0 == 0).collect();
            assert_eq!(mine.len(), 1, "cut {}: exactly one live version", cut);
            let is_v1 = mine[0].2 == v1.src && mine[0].3 == v1.bc;
            let is_v2 = mine[0].2 == v2.src && mine[0].3 == v2.bc;
            assert!(is_v1 || is_v2, "cut {}: a version that was never written", cut);
            assert_eq!(mine[0].1, "shift");

            // and the store still works afterwards
            sim.dead = false;
            sim.budget = usize::MAX;
            let (_, s) = all(&mut sim);
            let again = append(&mut sim, &pat(2, "after", 250, 250), s.cursor, 4);
            assert!(again.is_some(), "cut {}: the store must still accept a save", cut);
            let snap = snapshot(&mut sim);
            assert_eq!(snap.len(), 3, "cut {}: {:?}", cut, snap.iter().map(|s| s.0).collect::<Vec<_>>());
        }
    }

    #[test]
    fn a_cut_at_every_write_boundary_of_a_delete_is_survivable() {
        let a0 = pat(0, "a", 200, 200);
        let b0 = pat(1, "b", 200, 200);
        for cut in 0..4 {
            let mut sim = Sim::new(8 * PAGE);
            let a = append(&mut sim, &a0, 0, 1).unwrap();
            let b = append(&mut sim, &b0, a.end(), 2).unwrap();
            sim.budget = sim.ops + cut;
            mark_dead(&mut sim, &a);
            sim.dead = false;
            sim.budget = usize::MAX;
            let snap = snapshot(&mut sim);
            // either the delete landed or it did not; b is untouched either way
            assert!(snap.iter().any(|s| s.0 == 1));
            assert!(snap.len() == 1 || snap.len() == 2, "cut {}", cut);
            assert_eq!(all(&mut sim).1.cursor, b.end());
        }
    }

    // --- compaction ------------------------------------------------------

    /// Run a whole compaction against the simulator, exactly the way
    /// `patterns.rs` runs it: plan, then rebuild each non-frozen
    /// destination page in ascending order, skipping pages that already
    /// hold what they should.
    fn compact(sim: &mut Sim, keep: &[Rec], pinned: &[u32]) -> (Vec<Place>, u32) {
        let old_end = all(sim).1.cursor;
        let mut places = Vec::new();
        let cursor = plan(keep, pinned, &mut |p| places.push(p)).expect("a well-formed keep plans");
        let mut buf = vec![0u8; PAGE as usize];
        for page in 0..align_page(cursor) / PAGE {
            if frozen_page(keep, pinned, page) {
                continue;
            }
            if !build_page(sim, page, keep, &places, &mut buf) {
                return (places, cursor);
            }
            let a = (page * PAGE) as usize;
            if sim.mem[a..a + PAGE as usize] == buf[..] {
                continue;
            }
            if !sim.erase(page) || !sim.write(page * PAGE, &buf) {
                return (places, cursor);
            }
        }
        // last, once every destination page is written: the stale copies
        // the repack left above the new cursor
        let (from, to) = sweep_pages(cursor, old_end);
        for page in from..to {
            if !erased(sim, page * PAGE, PAGE) && !sim.erase(page) {
                break;
            }
        }
        (places, cursor)
    }

    #[test]
    fn compaction_reclaims_dead_space_exactly() {
        let mut sim = Sim::new(32 * PAGE);
        let pats: Vec<Pat> = (0..6)
            .map(|i| pat(i, &format!("p{}", i), 900 + i as usize * 100, 1300))
            .collect();
        let mut cursor = 0;
        let mut recs = Vec::new();
        for (i, p) in pats.iter().enumerate() {
            let r = append(&mut sim, p, cursor, i as u32 + 1).unwrap();
            cursor = r.end();
            recs.push(r);
        }
        // delete every other one
        for i in [0usize, 2, 4] {
            assert!(mark_dead(&mut sim, &recs[i]));
        }
        let live: Vec<Rec> = index(&mut sim).into_iter().map(|(r, _)| r).collect();
        let want: u32 = live.iter().map(|r| r.size()).sum();
        let (places, cursor) = compact(&mut sim, &live, &[]);
        assert_eq!(cursor, want, "packed length is the sum of the live files");
        assert!(places.iter().all(|p| p.to <= p.from), "nothing ever moves up");
        let snap = snapshot(&mut sim);
        assert_eq!(snap.len(), 3);
        for (i, s) in [1usize, 3, 5].iter().zip(&snap) {
            assert_eq!(s.1, pats[*i].name);
            assert_eq!(s.2, pats[*i].src);
            assert_eq!(s.3, pats[*i].bc);
        }
        let (_, st) = all(&mut sim);
        assert_eq!(st.dead, 0, "no dead bytes survive a compaction");
        assert_eq!(st.cursor, cursor);
        assert_eq!(st.resync, 0);
    }

    #[test]
    fn compaction_never_touches_a_pinned_file() {
        let mut sim = Sim::new(32 * PAGE);
        let pats: Vec<Pat> = (0..5)
            .map(|i| pat(i, &format!("p{}", i), 2000, 2600))
            .collect();
        let mut cursor = 0;
        let mut recs = Vec::new();
        for (i, p) in pats.iter().enumerate() {
            let r = append(&mut sim, p, cursor, i as u32 + 1).unwrap();
            cursor = r.end();
            recs.push(r);
        }
        assert!(mark_dead(&mut sim, &recs[0]));
        assert!(mark_dead(&mut sim, &recs[1]));
        let pinned = [3u32]; // an engine is executing p3 in place
        let pinned_bytes = read(&sim, recs[3].off, recs[3].size());
        let live: Vec<Rec> = index(&mut sim).into_iter().map(|(r, _)| r).collect();
        let (places, _) = compact(&mut sim, &live, &pinned);
        let p3 = places.iter().find(|p| live[p.idx].seq == 3).unwrap();
        assert!(!p3.moved(), "a pinned file must keep its address");
        assert_eq!(read(&sim, recs[3].off, recs[3].size()), pinned_bytes, "byte for byte");
        let snap = snapshot(&mut sim);
        assert_eq!(snap.len(), 3);
        for s in &snap {
            let p = &pats[s.0 as usize];
            assert_eq!(&s.2, &p.src);
            assert_eq!(&s.3, &p.bc);
        }
    }

    #[test]
    fn a_cut_at_every_write_boundary_of_a_compaction_loses_at_most_one_file() {
        let pats: Vec<Pat> = (0..5)
            .map(|i| pat(i, &format!("p{}", i), 700 + i as usize * 300, 1100))
            .collect();
        let build = |sim: &mut Sim| -> Vec<Rec> {
            let mut cursor = 0;
            let mut recs = Vec::new();
            for (i, p) in pats.iter().enumerate() {
                let r = append(sim, p, cursor, i as u32 + 1).unwrap();
                cursor = r.end();
                recs.push(r);
            }
            for i in [0usize, 2] {
                assert!(mark_dead(sim, &recs[i]));
            }
            recs
        };
        let mut probe = Sim::new(32 * PAGE);
        build(&mut probe);
        let live: Vec<Rec> = index(&mut probe).into_iter().map(|(r, _)| r).collect();
        let before = probe.ops;
        compact(&mut probe, &live, &[]);
        let total = probe.ops - before;
        assert!(total > 4, "compaction must take several steps: {}", total);

        for cut in 0..=total {
            let mut sim = Sim::new(32 * PAGE);
            build(&mut sim);
            let live: Vec<Rec> = index(&mut sim).into_iter().map(|(r, _)| r).collect();
            sim.budget = sim.ops + cut;
            compact(&mut sim, &live, &[]);
            sim.dead = false;
            sim.budget = usize::MAX;

            let snap = snapshot(&mut sim); // re-hashes every live file
            let survivors: Vec<u32> = snap.iter().map(|s| s.0).collect();
            for s in &snap {
                let p = &pats[s.0 as usize];
                assert_eq!(&s.2, &p.src, "cut {}: seq {} has foreign source", cut, s.0);
                assert_eq!(&s.3, &p.bc, "cut {}: seq {} has foreign bytecode", cut, s.0);
                assert_eq!(&s.1, &p.name);
            }
            assert!(
                survivors.len() >= 2,
                "cut {}: a compaction may lose the file in flight, not the log ({:?})",
                cut,
                survivors
            );
            // and the store is usable again
            let (_, st) = all(&mut sim);
            let live: Vec<Rec> = index(&mut sim).into_iter().map(|(r, _)| r).collect();
            assert!(append(&mut sim, &pat(9, "after", 200, 200), st.cursor, 99).is_some()
                || compact(&mut sim, &live, &[]).1 > 0, "cut {}: wedged", cut);
        }
    }

    #[test]
    fn pack_is_stable_when_there_is_nothing_to_reclaim() {
        let mut sim = Sim::new(16 * PAGE);
        let mut cursor = 0;
        for i in 0..4u32 {
            let r = append(&mut sim, &pat(i, "p", 400, 400), cursor, i + 1).unwrap();
            cursor = r.end();
        }
        let live: Vec<Rec> = index(&mut sim).into_iter().map(|(r, _)| r).collect();
        let erases = sim.erases;
        let (places, packed) = compact(&mut sim, &live, &[]);
        assert!(places.iter().all(|p| !p.moved()));
        assert_eq!(packed, cursor);
        assert_eq!(sim.erases, erases, "a no-op compaction must not erase anything");
    }

    #[test]
    fn churn_fuzz_keeps_every_live_file_readable() {
        let mut sim = Sim::new(48 * PAGE);
        let mut cursor = 0u32;
        let mut stamp = 1u32;
        let mut want: Vec<(u32, Pat)> = Vec::new();
        let mut rng = 0x1234_5678u32;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            rng
        };
        let mut compactions = 0;
        for round in 0..600 {
            let seq = next() % 12;
            let src = 200 + (next() % 3000) as usize;
            let bc = 200 + (next() % 3000) as usize;
            let p = pat(seq, &format!("n{}", seq), src, bc);
            if next() % 5 == 0 && !want.is_empty() {
                // delete
                let victim = (next() as usize) % want.len();
                let (s, _) = want.remove(victim);
                let live: Vec<(Rec, String)> = index(&mut sim);
                if let Some((r, _)) = live.iter().find(|(r, _)| r.seq == s) {
                    assert!(mark_dead(&mut sim, r));
                }
                continue;
            }
            let size = Rec::bytes(p.name.len() as u8, src as u32, bc as u32);
            if place(&mut sim, cursor, size).is_none() {
                let live: Vec<Rec> = index(&mut sim).into_iter().map(|(r, _)| r).collect();
                let (_, c) = compact(&mut sim, &live, &[]);
                cursor = c;
                compactions += 1;
                if place(&mut sim, cursor, size).is_none() {
                    continue; // genuinely full
                }
            }
            let old: Option<Rec> = index(&mut sim).into_iter().find(|(r, _)| r.seq == seq).map(|(r, _)| r);
            let r = append(&mut sim, &p, cursor, stamp).expect("planned to fit");
            stamp += 1;
            cursor = r.end();
            if let Some(o) = old {
                assert!(mark_dead(&mut sim, &o));
            }
            want.retain(|(s, _)| *s != seq);
            want.push((seq, p));

            // every live file must read back exactly, every round
            let snap = snapshot(&mut sim);
            assert_eq!(snap.len(), want.len(), "round {}", round);
            for (s, p) in &want {
                let got = snap.iter().find(|g| g.0 == *s).expect("stored file is findable");
                assert_eq!(&got.2, &p.src, "round {} seq {}", round, s);
                assert_eq!(&got.3, &p.bc, "round {} seq {}", round, s);
            }
            let (_, st) = all(&mut sim);
            assert_eq!(st.resync, 0, "round {}: the log stayed walkable", round);
            assert!(st.cursor <= sim.len());
        }
        assert!(compactions > 2, "the fuzz must actually compact ({})", compactions);
    }
}
