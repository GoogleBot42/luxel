//! The ring driver's arithmetic, as pure functions (Gitea #856;
//! docs/hub75-ring-design.md §3, §4, §6): the row-major emission order of a
//! ring slot, its control template, the descriptor counts, the slack knob,
//! and the fill queue's claim / release / late rules. No hardware and no
//! statics — the firmware's `Hub75Ring` (#857) and `tools/ringsim.py`
//! consume these, and the tests here prove them on the host.
//!
//! # A slot is one row pair, emitted row-major
//!
//! Today's chain is plane-major: every row of plane 6, then every row of
//! plane 5… A ring slot instead holds ONE row pair — all its planes — and
//! the DMA reads the slot's rows in this order:
//!
//! ```text
//! ENTRY ×1, plane 0 ×reps(0), plane 1 ×reps(1), …, plane P−1 ×reps(P−1)
//! ```
//!
//! (plane 0 is the MSB, `Schedule::reps` the BCM repeats). Why the ENTRY
//! block: a HUB75 block SHIFTS one row's data in and, while OE is on,
//! DISPLAYS whatever the panel latched at the end of the block before it.
//! So the first emission of a row pair still shows the PREVIOUS row pair's
//! last plane (its LSB), and must name that row's address and that plane's
//! OE width — which the MSB plane's row, shared by its `reps(0)` emissions,
//! cannot do (it needs the new row's address and the MSB width for
//! emissions 2…). The ENTRY row carries dark data, the previous row's
//! address and the LSB's OE width; the MSB's first emission then displays
//! the dark latch, and every plane still gets exactly `reps(p)` displays at
//! `lit(p)` clocks — [`format_slot`]'s test walks the words and counts. It
//! is one extra emission per row pair (`1/E`) and the row-major form of the
//! plane-major driver's trailing block (`Geometry::trail`, Gitea #795).
//!
//! By the same rule, plane row `p ≥ 1`'s OE width is `lit(p − 1)`: its
//! first emission displays plane `p − 1`. That is consistent with its later
//! emissions (which display `p` at `lit(p)`) exactly when `lit(p − 1) ==
//! lit(p)` or `reps(p) == 1` — true for every [`Schedule`]: the untruncated
//! planes share one width, and a truncated plane is emitted once.
//!
//! # Memory and descriptors
//!
//! A slot is `planes + 1` rows of `cols` words ([`slot_words`]); the DMA
//! chain over `N` slots is [`ring_descriptors`] descriptors, one per
//! emission per `max_chunk`. The slot's control template ([`format_slot`])
//! is written once with address 0; [`set_slot_address`] stamps the row
//! address when the slot is claimed for a row pair, and the packer writes
//! the colour bits.
//!
//! # The queue
//!
//! Slots are numbered by an ABSOLUTE emission counter `a` (modulo
//! [`Ring::period`], a multiple of both `n` and `rows`): claim `a` fills
//! slot `a % n` with row pair `a % rows` of pass `a / rows`. The DMA's own
//! counter is `abs_dma` = [`Ring::abs`] of its ring wraps and current slot.
//! Claim `a` is [`Ring::fillable`] while `1 ≤ a − abs_dma ≤ n − 2`: the
//! slot was last read `n` emissions ago, and the slot the DMA is in and the
//! one it enters next are never written ([`GUARD_SLOTS`]). It is
//! [`Ring::late`] once the DMA has reached it. The frame each pass reads is
//! decided by whoever claims its row 0, and travels IN the claim word
//! ([`claim_next`]) so every other row of that pass sees the same choice
//! without a second atomic — frame-atomic by construction.

use crate::{Control, Geometry, Schedule, ADDR_MASK, LATCH, MAX_PLANES, OE_ACTIVE};

/// The slot row the ENTRY block reads (dark data; previous row's address).
pub const ENTRY_ROW: usize = 0;

/// Slots never written: the one the DMA is reading, and one more because
/// the position probe (`OUT_DSCR`) can run one descriptor AHEAD of the data
/// still being clocked out — the prefetch `tools/ringsim.py` models — so
/// the claim `n − 1` ahead of a probe reading may be the slot under the beam.
pub const GUARD_SLOTS: u32 = 2;

/// The slot row plane `p` (0 = MSB) lives in.
#[must_use]
pub const fn plane_row(p: usize) -> usize {
    p + 1
}

/// Rows of `cols` words in one slot: the ENTRY row plus one per plane.
#[must_use]
pub const fn slot_rows(planes: usize) -> usize {
    planes + 1
}

/// Words in one slot.
#[must_use]
pub const fn slot_words(planes: usize, cols: usize) -> usize {
    slot_rows(planes) * cols
}

/// Bytes in one slot.
#[must_use]
pub const fn slot_bytes(planes: usize, cols: usize) -> usize {
    slot_words(planes, cols) * 2
}

/// Row shifts per row pair: the ENTRY block plus the schedule's emissions.
#[must_use]
pub fn emissions_per_pair(s: &Schedule) -> usize {
    1 + s.emissions()
}

/// Clocks the DMA spends in one slot: every emission is `cols` words.
#[must_use]
pub fn slot_clocks(s: &Schedule, cols: usize) -> usize {
    emissions_per_pair(s) * cols
}

/// Descriptors one emission of `cols` words needs at `max_chunk` bytes each.
#[must_use]
pub fn descs_per_emission(cols: usize, max_chunk: usize) -> usize {
    (cols * 2).div_ceil(max_chunk.max(1))
}

/// Descriptors one slot needs.
#[must_use]
pub fn descs_per_slot(s: &Schedule, cols: usize, max_chunk: usize) -> usize {
    emissions_per_pair(s) * descs_per_emission(cols, max_chunk)
}

/// Descriptors a ring of `n` slots needs.
#[must_use]
pub fn ring_descriptors(n: usize, s: &Schedule, cols: usize, max_chunk: usize) -> usize {
    n * descs_per_slot(s, cols, max_chunk)
}

/// The slot rows the DMA reads, in order: [`ENTRY_ROW`], then each plane's
/// row `reps(p)` times. `emissions_per_pair(s)` items.
pub fn slot_emissions(s: &Schedule) -> impl Iterator<Item = usize> + '_ {
    let planes = usize::from(s.planes);
    core::iter::once(ENTRY_ROW)
        .chain((0..planes).flat_map(move |p| core::iter::repeat_n(plane_row(p), s.reps(p))))
}

/// Where ring descriptor `idx` sits: `(slot, emission within the slot,
/// chunk within the emission)`.
#[must_use]
pub fn locate(idx: usize, descs_per_slot: usize, descs_per_emission: usize) -> (usize, usize, usize) {
    let slot = idx / descs_per_slot;
    let within = idx % descs_per_slot;
    (slot, within / descs_per_emission, within % descs_per_emission)
}

/// Whether ring descriptor `idx` carries the EOF flag: the last descriptor
/// of every `eof_every`-th slot (slot `eof_every − 1`, `2·eof_every − 1`, …).
/// With `eof_every` dividing `n`, the ring's last descriptor always does,
/// which is the wrap the ISR counts.
#[must_use]
pub fn carries_eof(idx: usize, descs_per_slot: usize, eof_every: usize) -> bool {
    let end = idx + 1;
    end.is_multiple_of(descs_per_slot) && (end / descs_per_slot).is_multiple_of(eof_every.max(1))
}

/// OE width of each slot row: the ENTRY displays the previous row's LSB,
/// plane `p`'s row displays plane `p − 1` (or, for the MSB, the ENTRY's dark
/// latch — width `lit(0)` keeps its later emissions right).
fn row_widths(s: &Schedule) -> [usize; MAX_PLANES + 1] {
    let planes = usize::from(s.planes);
    let mut w = [0usize; MAX_PLANES + 1];
    w[ENTRY_ROW] = s.lit(planes - 1);
    for p in 0..planes {
        w[plane_row(p)] = s.lit(p.saturating_sub(1));
    }
    w
}

/// Write a slot's control template: OE window and latch per row, address
/// 0, colour 0. `words` is exactly `slot_words(g.planes, g.cols)`.
/// [`set_slot_address`] stamps the row; the packer writes the colour bits.
///
/// # Panics
/// On a wrong length, zero columns, or more than [`MAX_PLANES`] planes.
pub fn format_slot(words: &mut [u16], g: Geometry, c: Control, s: &Schedule) {
    assert!(g.planes >= 1 && g.planes <= MAX_PLANES, "planes");
    assert!(g.cols > 0, "cols");
    assert_eq!(words.len(), slot_words(g.planes, g.cols), "slot length");
    let cols = g.cols;
    let blank = usize::from(c.blank);
    let latch = usize::from(c.latch_clocks);
    let oe_from = blank;
    let oe_end = cols.saturating_sub(latch + blank);
    let latch_from = cols.saturating_sub(latch);
    let widths = row_widths(s);
    for (row, chunk) in words.chunks_exact_mut(cols).enumerate() {
        let oe_to = oe_end.min(oe_from.saturating_add(widths[row]));
        for (i, w) in chunk.iter_mut().enumerate() {
            let mut v = 0u16;
            if i >= oe_from && i < oe_to {
                v |= OE_ACTIVE;
            }
            if i >= latch_from {
                v |= LATCH;
            }
            *w = v;
        }
    }
}

/// [`format_slot`] and [`set_slot_address`] in one pass over the words —
/// what the driver does when it claims a slot for row pair `r`: the whole
/// control template with the row's addresses, colour bits cleared, ready
/// for the packer. One write per word instead of two.
pub fn format_slot_for(words: &mut [u16], g: Geometry, c: Control, s: &Schedule, r: usize) {
    assert!(g.planes >= 1 && g.planes <= MAX_PLANES, "planes");
    assert!(g.cols > 0 && g.rows > 0 && r < g.rows, "shape");
    assert_eq!(words.len(), slot_words(g.planes, g.cols), "slot length");
    let cols = g.cols;
    let blank = usize::from(c.blank);
    let latch = usize::from(c.latch_clocks);
    let oe_from = blank;
    let oe_end = cols.saturating_sub(latch + blank);
    let latch_from = cols.saturating_sub(latch);
    let widths = row_widths(s);
    let prev = ((r + g.rows - 1) % g.rows) as u16 & ADDR_MASK;
    let this = (r as u16) & ADDR_MASK;
    for (row, chunk) in words.chunks_exact_mut(cols).enumerate() {
        let oe_to = oe_end.min(oe_from.saturating_add(widths[row]));
        let addr = if row == ENTRY_ROW { prev } else { this };
        for (i, w) in chunk.iter_mut().enumerate() {
            let mut v = addr;
            if i >= oe_from && i < oe_to {
                v |= OE_ACTIVE;
            }
            if i >= latch_from {
                v |= LATCH;
            }
            *w = v;
        }
    }
}

/// Stamp row pair `r` of `rows` into a slot: the ENTRY row names the
/// previous row (`(r + rows − 1) % rows`), the plane rows name `r`. Every
/// other bit is left alone, so this is safe on a packed slot.
pub fn set_slot_address(words: &mut [u16], planes: usize, cols: usize, r: usize, rows: usize) {
    assert_eq!(words.len(), slot_words(planes, cols), "slot length");
    assert!(rows > 0 && r < rows, "row");
    let prev = ((r + rows - 1) % rows) as u16 & ADDR_MASK;
    let this = (r as u16) & ADDR_MASK;
    for (row, chunk) in words.chunks_exact_mut(cols).enumerate() {
        let addr = if row == ENTRY_ROW { prev } else { this };
        for w in chunk {
            *w = (*w & !ADDR_MASK) | addr;
        }
    }
}

/// Microseconds a packer has from a claim becoming fillable until it is
/// late: `n − GUARD_SLOTS` slots' worth of clocks.
#[must_use]
pub fn slack_us(n: u32, s: &Schedule, cols: usize, clock_hz: u32) -> u32 {
    let usable = u64::from(n.saturating_sub(GUARD_SLOTS));
    (usable * slot_clocks(s, cols) as u64 * 1_000_000 / u64::from(clock_hz.max(1))) as u32
}

/// The inverse of [`slack_us`]: the fewest slots that give at least
/// `ring_us` of slack — plus the guard — capped at `rows` (a ring the size
/// of the frame IS the frame) and never under `GUARD_SLOTS + 1`.
#[must_use]
pub fn slots_for_slack(ring_us: u32, s: &Schedule, cols: usize, clock_hz: u32, rows: usize) -> u32 {
    let clocks = u64::from(ring_us) * u64::from(clock_hz) / 1_000_000;
    let per_slot = slot_clocks(s, cols).max(1) as u64;
    let usable = clocks.div_ceil(per_slot) as u32;
    (usable + GUARD_SLOTS).max(GUARD_SLOTS + 1).min(rows.max(1) as u32)
}

/// The ring's counting rules: `n` slots over `rows` row pairs, with every
/// absolute counter taken modulo `period` — the largest multiple of both
/// under 2^30, so slot and row arithmetic survive the wrap and the top two
/// bits of a claim word are free for the frame choice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ring {
    pub n: u32,
    pub rows: u32,
    pub period: u32,
}

const fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

impl Ring {
    /// `n ≥ GUARD_SLOTS + 1` slots over `rows ≥ 1` row pairs.
    #[must_use]
    pub const fn new(n: u32, rows: u32) -> Self {
        assert!(n > GUARD_SLOTS && rows >= 1, "ring shape");
        let lcm = n / gcd(n, rows) * rows;
        let period = (1u32 << 30) / lcm * lcm;
        Self { n, rows, period }
    }

    /// Ring wraps before the wrap counter itself repeats: `period / n`.
    #[must_use]
    pub const fn wrap_period(&self) -> u32 {
        self.period / self.n
    }

    /// The DMA's absolute counter from its ring-wrap count and current slot.
    #[must_use]
    pub const fn abs(&self, wraps: u32, slot: u32) -> u32 {
        ((wraps % self.wrap_period()) * self.n + slot % self.n) % self.period
    }

    /// Slot claim `a` fills.
    #[must_use]
    pub const fn slot(&self, a: u32) -> u32 {
        a % self.n
    }

    /// Row pair claim `a` carries.
    #[must_use]
    pub const fn row(&self, a: u32) -> u32 {
        a % self.rows
    }

    /// Pass (frame) claim `a` belongs to, modulo the period.
    #[must_use]
    pub const fn pass(&self, a: u32) -> u32 {
        a / self.rows
    }

    /// Emissions from `from` forward to `to`, modulo the period.
    #[must_use]
    pub const fn dist(&self, from: u32, to: u32) -> u32 {
        (to + self.period - from) % self.period
    }

    /// Claim `a` may be packed now: its slot has been left by the DMA and is
    /// neither the slot being read nor the next one.
    #[must_use]
    pub const fn fillable(&self, a: u32, abs_dma: u32) -> bool {
        let d = self.dist(abs_dma, a);
        d >= 1 && d + GUARD_SLOTS <= self.n
    }

    /// Claim `a` is late: the DMA has reached (or passed) it.
    #[must_use]
    pub const fn late(&self, a: u32, abs_dma: u32) -> bool {
        let d = self.dist(abs_dma, a);
        d == 0 || d > self.period / 2
    }

    /// Emissions until claim `a` turns late, from `abs_dma`.
    #[must_use]
    pub const fn until_late(&self, a: u32, abs_dma: u32) -> u32 {
        if self.late(a, abs_dma) {
            0
        } else {
            self.dist(abs_dma, a)
        }
    }

    /// The first claim after boot: the ring is pre-filled with rows
    /// `0..n`, so the queue starts at `n`.
    #[must_use]
    pub const fn first_claim(&self) -> u32 {
        self.n
    }

    /// Where a queue head the beam has overtaken re-joins: the slot right
    /// after the one the beam is in — the earliest fillable claim. A late
    /// head can never become fillable on its own (the beam only moves
    /// away from it), so a packer that finds one jumps here and counts the
    /// claims it skipped; the rows between show their stale content at
    /// their own addresses, exactly as a skipped claim does.
    #[must_use]
    pub const fn catch_up(&self, abs_dma: u32) -> u32 {
        (abs_dma + 1) % self.period
    }
}

/// One claim out of the queue: which frame buffer to read, and the
/// absolute emission it fills.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Claim {
    /// Packed-frame index (0..=3) for this claim's pass — two bits, for
    /// the driver's four packed frames (#892). One bit here with four
    /// frames put every pass whose frame was 2 or 3 onto frame 0 or 1, a
    /// frame or two old: the snake's head ran backwards and forwards on
    /// the bench until Jeremy saw it (2026-09-30).
    pub buf: u8,
    /// Absolute emission counter, `< Ring::period`.
    pub abs: u32,
}

impl Claim {
    /// The queue word: frame choice in bit 31, counter below.
    #[must_use]
    pub const fn encode(self) -> u32 {
        ((self.buf as u32 & 3) << 30) | (self.abs & 0x3fff_ffff)
    }

    /// The claim a queue word denotes.
    #[must_use]
    pub const fn decode(word: u32) -> Self {
        Self { buf: (word >> 30) as u8, abs: word & 0x3fff_ffff }
    }
}

/// The claim a packer takes from queue word `word`, and the word it must
/// CAS in its place. A claim of a pass's row 0 picks `newest_buf` for the
/// whole pass; every later row of the pass inherits the choice from the
/// word, so the two frame buffers are read frame-atomically without any
/// second atomic or wait. Bounded by `ring.period`.
#[must_use]
pub const fn claim_next(word: u32, newest_buf: u8, ring: &Ring) -> (Claim, u32) {
    let cur = Claim::decode(word);
    let buf = if ring.row(cur.abs) == 0 { newest_buf & 3 } else { cur.buf };
    let claim = Claim { buf, abs: cur.abs };
    let next = Claim { buf, abs: (cur.abs + 1) % ring.period };
    (claim, next.encode())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::COLOR_MASK;
    use std::vec;
    use std::vec::Vec;

    /// The claim word carries FOUR packed-frame indices (the driver's
    /// `FRAMES`), not two: with one bit, a pass whose frame was 2 or 3
    /// read frame 0 or 1 — a frame or two old — and the picture stuttered
    /// backwards (2026-09-30, Jeremy's eye on the first ring-default boot).
    #[test]
    fn claim_word_carries_four_frames_and_the_full_period() {
        let ring = Ring::new(10, 32);
        for buf in 0..4u8 {
            for abs in [0u32, 1, ring.period - 1] {
                let c = Claim { buf, abs };
                assert_eq!(Claim::decode(c.encode()), c, "buf {buf} abs {abs}");
                // A pass's row 0 picks `newest`, whatever its index.
                let word = Claim { buf: 0, abs: 0 }.encode();
                let (claim, _) = claim_next(word, buf, &ring);
                assert_eq!(claim.buf, buf);
            }
        }
    }

    const G: Geometry = Geometry::new(32, 64, 7);

    #[test]
    fn emission_order_is_entry_then_planes_by_repeats() {
        let c = Control::default();
        for lsb in [0u16, 30, 15, 7, 3, 2] {
            let s = Schedule::plan(G, c, lsb);
            let order: Vec<usize> = slot_emissions(&s).collect();
            assert_eq!(order.len(), emissions_per_pair(&s));
            assert_eq!(order.len(), 1 + s.emissions());
            assert_eq!(order[0], ENTRY_ROW);
            let mut i = 1;
            for p in 0..7 {
                for _ in 0..s.reps(p) {
                    assert_eq!(order[i], plane_row(p), "lsb {lsb} emission {i}");
                    i += 1;
                }
            }
            assert_eq!(i, order.len());
        }
        // stock: 1 + 127 emissions, E from the design doc
        let stock = Schedule::plan(G, c, 0);
        assert_eq!(emissions_per_pair(&stock), 128);
        assert_eq!(slot_clocks(&stock, 64), 128 * 64);
        assert_eq!(slot_words(7, 64), 8 * 64);
        assert_eq!(slot_bytes(7, 256), 8 * 256 * 2);
    }

    #[test]
    fn descriptor_counts_and_eof_placement() {
        let c = Control::default();
        let s = Schedule::plan(G, c, 0);
        assert_eq!(descs_per_emission(64, 4092), 1);
        assert_eq!(descs_per_emission(2046, 4092), 1);
        assert_eq!(descs_per_emission(2047, 4092), 2);
        let per_slot = descs_per_slot(&s, 64, 4092);
        assert_eq!(per_slot, 128);
        assert_eq!(ring_descriptors(4, &s, 64, 4092), 512);
        // ~3 KB of descriptors for a 3 ms ring at 256 columns, per §4
        let s4 = Schedule::plan(Geometry::new(32, 256, 7), c, 7);
        let n = slots_for_slack(3000, &s4, 256, 20_000_000, 32);
        assert!(ring_descriptors(n as usize, &s4, 256, 4092) * 12 <= 4096, "n {n}");
        // locate round-trips
        for idx in 0..512 {
            let (slot, em, chunk) = locate(idx, per_slot, 1);
            assert_eq!(slot * per_slot + em + chunk, idx);
            assert!(em < per_slot && chunk == 0);
        }
        // EOF on the last descriptor of every 2nd slot, so on the ring's last
        let eofs: Vec<usize> = (0..512).filter(|&i| carries_eof(i, per_slot, 2)).collect();
        assert_eq!(eofs, vec![2 * 128 - 1, 4 * 128 - 1]);
        assert!(carries_eof(511, per_slot, 1));
        assert!(carries_eof(511, per_slot, 4));
        assert!(!carries_eof(510, per_slot, 1));
    }

    /// Walk a ring of slots the way the panel sees it (mirrors
    /// `on_time_per_row` in lib.rs): each OE-on word lights the latched row
    /// with the latched plane's data; each block's latch words load the row
    /// and plane it shifted. Returns on-time per (row, plane) for the second
    /// of two passes, and asserts OE never lights a row other than the
    /// latched one.
    fn on_time_per_row(g: Geometry, c: Control, s: &Schedule) -> Vec<Vec<usize>> {
        const DARK: usize = usize::MAX;
        let mut slot = vec![0u16; slot_words(g.planes, g.cols)];
        format_slot(&mut slot, g, c, s);
        let order: Vec<usize> = slot_emissions(s).collect();
        let mut on = vec![vec![0usize; g.planes]; g.rows];
        let mut dark_on = vec![0usize; g.rows];
        let (mut latched_row, mut latched_plane) = (g.rows - 1, g.planes - 1);
        for pass in 0..2 {
            for r in 0..g.rows {
                set_slot_address(&mut slot, g.planes, g.cols, r, g.rows);
                for &row in &order {
                    let block = &slot[row * g.cols..(row + 1) * g.cols];
                    let addr = usize::from(block[0] & ADDR_MASK);
                    let data_plane = if row == ENTRY_ROW { DARK } else { row - 1 };
                    for w in block {
                        assert_eq!(usize::from(w & ADDR_MASK), addr, "address constant within a block");
                        if w & OE_ACTIVE != 0 {
                            assert_eq!(addr, latched_row, "row {r} slot row {row}: OE lights a row that is not latched");
                            if pass == 1 {
                                if latched_plane == DARK {
                                    dark_on[addr] += 1;
                                } else {
                                    on[addr][latched_plane] += 1;
                                }
                            }
                        }
                    }
                    if block.iter().any(|w| w & LATCH != 0) {
                        latched_row = r;
                        latched_plane = data_plane;
                    }
                }
            }
        }
        // the MSB's first emission shows the ENTRY's dark latch, once per row
        for (r, &dark) in dark_on.iter().enumerate() {
            assert_eq!(dark, s.lit(0), "row {r}: dark display is one MSB window");
        }
        on
    }

    #[test]
    #[allow(clippy::needless_range_loop)] // (row, plane) indices name the assertion
    fn every_row_and_plane_gets_its_exact_weight() {
        let c = Control::default();
        for lsb in [0u16, 30, 15, 7, 3, 2, 1] {
            let s = Schedule::plan(G, c, lsb);
            let on = on_time_per_row(G, c, &s);
            for r in 0..G.rows {
                for p in 0..G.planes {
                    assert_eq!(on[r][p], s.on_time(p), "lsb {lsb} row {r} plane {p}");
                }
            }
        }
        // other shapes: a wide chain, a shallow scan, 8 planes, 4 planes
        for (rows, cols, planes, blank, latch) in
            [(32usize, 256usize, 7usize, 7u8, 1u8), (16, 32, 7, 1, 1), (8, 64, 8, 3, 3), (32, 128, 4, 1, 2)]
        {
            let g = Geometry::new(rows, cols, planes);
            let c = Control::new(blank, latch);
            for lsb in [0u16, 9, 3] {
                let s = Schedule::plan(g, c, lsb);
                let on = on_time_per_row(g, c, &s);
                for r in 0..rows {
                    for p in 0..planes {
                        assert_eq!(on[r][p], s.on_time(p), "{rows}x{cols}x{planes} lsb {lsb} row {r} plane {p}");
                    }
                }
            }
        }
    }

    #[test]
    fn slot_template_shape_and_address_stamp() {
        let c = Control::new(2, 1);
        let s = Schedule::plan(G, c, 7);
        let mut slot = vec![0u16; slot_words(7, 64)];
        format_slot(&mut slot, G, c, &s);
        // every row: OE off in the head/tail blanking, latch on the last word
        for (row, chunk) in slot.chunks_exact(64).enumerate() {
            assert!(chunk[..2].iter().all(|w| w & OE_ACTIVE == 0), "row {row} head blanking");
            assert!(chunk[61..].iter().all(|w| w & OE_ACTIVE == 0), "row {row} tail blanking");
            assert_eq!(chunk.iter().filter(|w| *w & LATCH != 0).count(), 1);
            assert!(chunk.iter().all(|w| w & (COLOR_MASK | ADDR_MASK) == 0));
        }
        // widths: entry = LSB's, plane p = plane p−1's
        let width = |row: usize| slot[row * 64..(row + 1) * 64].iter().filter(|w| *w & OE_ACTIVE != 0).count();
        assert_eq!(width(ENTRY_ROW), s.lit(6));
        assert_eq!(width(plane_row(0)), s.lit(0));
        for p in 1..7 {
            assert_eq!(width(plane_row(p)), s.lit(p - 1), "plane {p}");
        }
        // stamping the address touches only the address bits
        let before = slot.clone();
        for w in slot.iter_mut().step_by(5) {
            *w |= COLOR_MASK; // pretend the packer wrote colour
        }
        let packed = slot.clone();
        set_slot_address(&mut slot, 7, 64, 0, 32);
        for (row, chunk) in slot.chunks_exact(64).enumerate() {
            let want = if row == ENTRY_ROW { 31 } else { 0 };
            assert!(chunk.iter().all(|w| w & ADDR_MASK == want), "row {row}");
        }
        for (i, (a, b)) in slot.iter().zip(&packed).enumerate() {
            assert_eq!(a & !ADDR_MASK, b & !ADDR_MASK, "word {i}");
        }
        set_slot_address(&mut slot, 7, 64, 17, 32);
        assert!(slot[..64].iter().all(|w| w & ADDR_MASK == 16));
        assert!(slot[64..].iter().all(|w| w & ADDR_MASK == 17));
        let _ = before;
    }

    #[test]
    fn format_slot_for_equals_format_then_stamp() {
        let c = Control::new(3, 1);
        for lsb in [0u16, 7] {
            let s = Schedule::plan(G, c, lsb);
            for r in [0usize, 1, 17, 31] {
                let mut want = vec![0u16; slot_words(7, 64)];
                format_slot(&mut want, G, c, &s);
                set_slot_address(&mut want, 7, 64, r, 32);
                let mut got = vec![0xffffu16; slot_words(7, 64)];
                format_slot_for(&mut got, G, c, &s, r);
                assert_eq!(got, want, "lsb {lsb} row {r}");
            }
        }
    }

    #[test]
    fn slack_and_its_inverse() {
        let c = Control::new(7, 1);
        let g = Geometry::new(32, 256, 7);
        let clock = 20_000_000;
        for lsb in [0u16, 30, 15, 7] {
            let s = Schedule::plan(g, c, lsb);
            for ms in [1u32, 3, 10] {
                let n = slots_for_slack(ms * 1000, &s, 256, clock, 32);
                assert!(n > GUARD_SLOTS && n <= 32, "lsb {lsb} {ms} ms -> {n}");
                if n < 32 {
                    assert!(slack_us(n, &s, 256, clock) >= ms * 1000, "lsb {lsb} {ms} ms: {n} slots give {} us", slack_us(n, &s, 256, clock));
                    assert!(slack_us(n - 1, &s, 256, clock) < ms * 1000, "lsb {lsb} {ms} ms: {n} is not minimal");
                }
            }
        }
        // the design's §4 rows-for-3-ms column, plus the entry block and guard
        let stock = Schedule::plan(g, c, 0);
        assert_eq!(stock.emissions(), 127);
        assert_eq!(slots_for_slack(3000, &stock, 256, clock, 32), 2 + GUARD_SLOTS);
        // the ×16 step (E = 8) is whichever lsb the width allows
        let x16 = (1..=30u16).map(|l| Schedule::plan(g, c, l)).find(|s| s.emissions() == 8).expect("an E = 8 step");
        assert_eq!(slot_clocks(&x16, 256), 9 * 256);
        assert_eq!(slots_for_slack(3000, &x16, 256, clock, 32), 27 + GUARD_SLOTS);
        assert_eq!(slots_for_slack(1000, &x16, 256, clock, 32), 9 + GUARD_SLOTS);
        // capped at the frame, floored at the guard + 1
        assert_eq!(slots_for_slack(100_000, &x16, 256, clock, 32), 32);
        assert_eq!(slots_for_slack(0, &x16, 256, clock, 32), GUARD_SLOTS + 1);
        assert_eq!(slack_us(2, &stock, 256, clock), 0);
    }

    #[test]
    fn queue_rules() {
        let ring = Ring::new(6, 32);
        assert_eq!(ring.period % 6, 0);
        assert_eq!(ring.period % 32, 0);
        assert!(ring.period > 1 << 29 && ring.period < 1 << 30);
        assert_eq!(ring.first_claim(), 6);
        // the DMA sits at abs 10 (slot 4 of wrap 1)
        let dma = ring.abs(1, 4);
        assert_eq!(dma, 10);
        // claims 11..=14 are fillable: their slots were read at 5..=8
        for a in 11..=14 {
            assert!(ring.fillable(a, dma), "{a}");
            assert!(!ring.late(a, dma));
            assert_eq!(ring.until_late(a, dma), a - dma);
        }
        // 15 is n − 1 ahead: safe if the probe is exact, the slot under the
        // beam if the probe ran one descriptor ahead — the guard refuses it;
        // 16 is the DMA's own slot
        assert!(!ring.fillable(15, dma));
        assert!(!ring.fillable(16, dma));
        assert_eq!(ring.slot(16), ring.slot(dma));
        // 10 and below are late
        assert!(ring.late(10, dma) && ring.late(9, dma) && ring.late(0, dma));
        assert!(!ring.fillable(10, dma));
        assert_eq!(ring.until_late(10, dma), 0);
        // rows and passes
        assert_eq!(ring.row(35), 3);
        assert_eq!(ring.pass(35), 1);
        assert_eq!(ring.slot(35), 5);
        // across the period wrap the distances still hold
        let last = ring.period - 1;
        assert_eq!(ring.dist(last, 0), 1);
        assert!(ring.fillable(2, last));
        assert!(ring.late(last, 0));
        assert_eq!(ring.abs(ring.wrap_period() - 1, 5), last);
        assert_eq!(ring.abs(ring.wrap_period(), 0), 0);
        // the guard: exactly n − 2 fillable claims at any moment
        for dma in [0u32, 7, 31, 32, 1000, ring.period - 3] {
            let fillable = (0..ring.period).filter(|&a| ring.dist(dma, a) < 64 && ring.fillable(a, dma)).count();
            assert_eq!(fillable, (ring.n - GUARD_SLOTS) as usize, "dma {dma}");
        }
    }

    #[test]
    fn claims_carry_the_frame_choice_per_pass() {
        let ring = Ring::new(6, 32);
        let mut word = Claim { buf: 0, abs: ring.first_claim() }.encode();
        // rows 6..31 of pass 0 inherit buffer 0 whatever is newest
        for a in 6..32 {
            let (c, next) = claim_next(word, 1, &ring);
            assert_eq!(c, Claim { buf: 0, abs: a });
            word = next;
        }
        // row 0 of pass 1 picks the newest (1); the rest of the pass follow
        for a in 32..64 {
            let (c, next) = claim_next(word, if a == 32 { 1 } else { 0 }, &ring);
            assert_eq!(c, Claim { buf: 1, abs: a }, "abs {a}");
            word = next;
        }
        // pass 2 goes back to 0
        let (c, _) = claim_next(word, 0, &ring);
        assert_eq!(c, Claim { buf: 0, abs: 64 });
        // the word wraps at the period, buffer bit intact
        let end = Claim { buf: 1, abs: ring.period - 1 }.encode();
        let (c, next) = claim_next(end, 0, &ring);
        assert_eq!(c.abs, ring.period - 1);
        assert_eq!(Claim::decode(next), Claim { buf: 1, abs: 0 });
        assert_eq!(Claim::decode(Claim { buf: 1, abs: 12345 }.encode()), Claim { buf: 1, abs: 12345 });
    }

    /// The boot deadlock met on the Seengreat (2026-09-28): the DMA had
    /// run for seconds before the first drain, the queue head `n` was a
    /// thousand wraps behind the beam, and `fillable` stayed false forever.
    #[test]
    fn a_late_head_catches_up_to_a_fillable_claim() {
        let ring = Ring::new(10, 32);
        let head = ring.first_claim();
        for wraps in [0u32, 1, 3, 1000, ring.wrap_period() - 1] {
            for slot in 0..10 {
                let dma = ring.abs(wraps, slot);
                if ring.late(head, dma) {
                    assert!(!ring.fillable(head, dma));
                    let target = ring.catch_up(dma);
                    assert!(!ring.late(target, dma), "wraps {wraps} slot {slot}");
                    assert!(ring.fillable(target, dma), "wraps {wraps} slot {slot}");
                    assert_eq!(ring.dist(dma, target), 1);
                    // slot and row stay coherent: both come off the same counter
                    assert_eq!(ring.slot(target), (dma + 1) % 10);
                    assert_eq!(ring.row(target), (dma + 1) % 32);
                } else if wraps == 0 {
                    // the pre-filled first wrap: the head becomes fillable
                    // once the beam has left slot 0 by the guard
                    assert_eq!(ring.fillable(head, dma), slot >= GUARD_SLOTS, "wraps {wraps} slot {slot}");
                } else {
                    // the beam is in the last wrap before the period: the
                    // seeded head is a wrap or two AHEAD of it, not behind —
                    // too far ahead to fill, and not late either
                    assert!(!ring.fillable(head, dma), "wraps {wraps} slot {slot}");
                }
            }
        }
        // the very first wrap is never late for the seeded head
        assert!(!ring.late(head, ring.abs(0, 9)));
        // one wrap on, the head is late from slot 0 of wrap 1 onward
        assert!(ring.late(head, ring.abs(1, 0)));
    }

    /// The premise of the full-frame reuse (#896): with `n == rows` a
    /// claim's slot IS its row pair, on every pass, across the period —
    /// so a slot that holds row `r` of a frame holds exactly what the next
    /// pass's claim for that slot wants, until the frame changes. Below a
    /// full frame the same slot carries a different row every pass.
    #[test]
    fn a_full_frame_ring_maps_each_slot_to_one_row() {
        for rows in [1u32, 8, 16, 32] {
            let full = Ring::new(rows.max(GUARD_SLOTS + 1), rows);
            if full.n == rows {
                for a in (0..full.period).step_by(7).chain([full.period - 1]) {
                    assert_eq!(full.slot(a), full.row(a), "rows {rows} abs {a}");
                }
            }
        }
        let partial = Ring::new(10, 32);
        // the same slot, one pass apart, carries a different row pair
        let a = partial.first_claim();
        let b = a + 32;
        assert_eq!(partial.row(a), partial.row(b));
        assert_ne!(partial.slot(a), partial.slot(b));
        // and the ring's max is the frame: `slots_for_slack` never exceeds rows
        let s = Schedule::plan(Geometry::new(32, 64, 7), Control::new(7, 1), 15);
        assert_eq!(slots_for_slack(1_000_000, &s, 64, 20_000_000, 32), 32);
    }
}
