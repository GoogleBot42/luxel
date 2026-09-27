//! The BCM emission schedule — how many times each bitplane's row blocks are
//! shifted out per rescan, and for how many clocks OE is asserted in each
//! (Gitea #460 / #789).
//!
//! ## Plain BCM, and what it costs
//!
//! A framebuffer word carries the panel's OE bit ([`crate::OE_ACTIVE`]), so
//! the time a row is LIT is the number of words in its block with OE set, and
//! the time a row block COSTS is `cols` clocks whatever its OE bits say. Stock
//! BCM lights every plane's block for the whole lit width `W = cols − latch −
//! 2·blank` and gets the binary weights by re-shifting plane `k` (0 = LSB)
//! `2^k` times, so a rescan is `2^planes − 1` row shifts — 127 at 7 planes —
//! and the LSB is on for a whole row shift per rescan even though its weight
//! only needs it lit for a fraction of one.
//!
//! ## Truncated low planes
//!
//! Emit plane `k` for exactly `lsb · 2^k` clocks instead, where `lsb` is the
//! LSB's on-time in pixel clocks: planes whose on-time fits inside one row
//! shift are shifted ONCE with OE cut off after `lsb · 2^k` words, and only
//! the planes above them keep descriptor repeats. With `t` such planes a
//! rescan is `E = t + 2^(planes − t) − 1` row shifts.
//!
//! **What it costs in brightness is small, not proportional.** Perceived
//! brightness is on-time per unit TIME, and the pass shrinks along with the
//! on-time: at full white the LEDs are on `lsb · (2^planes − 1)` of every
//! `cols · E` clocks, against `W · (2^planes − 1)` of `cols · (2^planes − 1)`
//! for stock, so relative brightness is `lsb · (2^planes − 1) / (W · E)`. At
//! the top of each step (`lsb = W >> t`) that is ~96 % for one truncated plane,
//! ~95 % for two, ~81 % for three on the 64-wide bench panel — the refresh
//! doubles per step and the brightness barely moves, which is the whole point
//! (measured on the Seengreat 2026-09-26: the panel looked as bright as stock
//! at four times the rescan). Within a step the refresh is constant and the
//! brightness falls linearly with `lsb`, so the step tops are the only
//! positions worth offering. `lsb = W` (or 0) IS the stock schedule.
//! Binary weights stay EXACT — plane `k` is lit
//! `lsb · 2^k` clocks whether truncated or repeated — which is what keeps
//! the grey ramp monotonic; the C++ library's version of this trick got the
//! arithmetic wrong (mrfaptastic #862) and is not what this is.
//!
//! Precisely: `t` is the largest `t ≤ planes − 1` with `lsb << t ≤ W`, so the
//! repeated planes are each lit `L = lsb << t` clocks per shift (`≤ W`; equal
//! to `W` when `lsb` divides it evenly) and plane `k ≥ t` is shifted
//! `2^(k − t)` times. The MSB is always a repeated plane, so the ring still
//! starts with the MSB run the spare-plane swap relies on.
//!
//! ## Live blanking
//!
//! `blank` applies without a reboot and shrinks `W`; `t` cannot move without
//! rebuilding the DMA descriptor chain, so [`Schedule::refit`] keeps `t` and
//! re-clamps `lsb` to `W >> t`. Every plane's on-time scales by the same
//! factor, so the weights stay exact and only the brightness moves.

use crate::{Control, Geometry, MAX_PLANES};

/// One rescan's emission plan for a framebuffer of [`Geometry`] at a
/// [`Control`] template. Plane index 0 is the MSB throughout this crate;
/// `k = planes − 1 − plane_idx` is the bit weight.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Schedule {
    /// Bitplanes, `1..=MAX_PLANES`.
    pub planes: u8,
    /// Truncated (single-shift, OE cut early) planes, counted from the LSB.
    /// `0` = the stock schedule.
    pub trunc: u8,
    /// Effective LSB on-time in pixel clocks. `0` only when the template has
    /// no lit clocks at all (`width == 0`).
    pub lsb: u16,
    /// Lit clocks available per row block at this template:
    /// `cols − latch − 2·blank`.
    pub width: u16,
    /// The configured value the plan was made from (`0` = full), kept so a
    /// [`Schedule::refit`] can re-clamp from the user's number rather than
    /// from an already-clamped one.
    configured: u16,
}

impl Schedule {
    /// The lit clocks of a row block under `c`: `cols − latch − 2·blank`.
    #[must_use]
    pub fn lit_width(cols: usize, c: Control) -> usize {
        cols.saturating_sub(usize::from(c.latch_clocks) + 2 * usize::from(c.blank))
    }

    /// Plan a boot: choose `t` for the configured `lsb` (`0` = full).
    #[must_use]
    pub fn plan(g: Geometry, c: Control, lsb: u16) -> Self {
        let planes = g.planes.clamp(1, MAX_PLANES) as u8;
        let w = Self::lit_width(g.cols, c).min(usize::from(u16::MAX));
        let eff0 = if lsb == 0 || usize::from(lsb) > w { w } else { usize::from(lsb) };
        let mut t = 0u8;
        if eff0 > 0 {
            while t + 1 <= planes - 1 && (eff0 << (t + 1)) <= w {
                t += 1;
            }
        }
        Self { planes, trunc: t, lsb: eff0 as u16, width: w as u16, configured: lsb }
    }

    /// Re-fit this plan to a new template (a live `blank` change): `trunc` is
    /// fixed — the descriptor chain was built from it — and `lsb` is
    /// re-clamped so `lsb << trunc` still fits the new lit width.
    #[must_use]
    pub fn refit(self, g: Geometry, c: Control) -> Self {
        let w = Self::lit_width(g.cols, c).min(usize::from(u16::MAX));
        let eff0 = if self.configured == 0 || usize::from(self.configured) > w {
            w
        } else {
            usize::from(self.configured)
        };
        let eff = eff0.min(w >> self.trunc);
        Self { lsb: eff as u16, width: w as u16, ..self }
    }

    /// The configured `lsb` this plan was made from (`0` = full).
    #[must_use]
    pub fn configured(&self) -> u16 {
        self.configured
    }

    /// Bit weight of `plane_idx`: `planes − 1 − plane_idx`.
    fn k(&self, plane_idx: usize) -> u32 {
        (usize::from(self.planes) - 1 - plane_idx) as u32
    }

    /// Row shifts of `plane_idx` (0 = MSB) per rescan.
    #[must_use]
    pub fn reps(&self, plane_idx: usize) -> usize {
        let k = self.k(plane_idx);
        if k < u32::from(self.trunc) {
            1
        } else {
            1 << (k - u32::from(self.trunc))
        }
    }

    /// Clocks OE is asserted in each of `plane_idx`'s row blocks.
    #[must_use]
    pub fn lit(&self, plane_idx: usize) -> usize {
        let k = self.k(plane_idx);
        let shift = k.min(u32::from(self.trunc));
        usize::from(self.lsb) << shift
    }

    /// Total on-time of `plane_idx` per rescan, in clocks — `lsb << k`, the
    /// exact binary weight, however the plane is emitted.
    #[must_use]
    pub fn on_time(&self, plane_idx: usize) -> usize {
        self.reps(plane_idx) * self.lit(plane_idx)
    }

    /// Row shifts per rescan: `t + 2^(planes − t) − 1`.
    #[must_use]
    pub fn emissions(&self) -> usize {
        (0..usize::from(self.planes)).map(|p| self.reps(p)).sum()
    }

    /// Row shifts per rescan of the stock schedule at this depth,
    /// `2^planes − 1` — the yardstick the refresh gain is against.
    #[must_use]
    pub fn full_emissions(&self) -> usize {
        (1usize << self.planes) - 1
    }

    /// Per-plane repeat counts as the DMA driver takes them
    /// (`esp_hub75::set_plane_repeats`). Entries past `planes` are 0.
    #[must_use]
    pub fn reps_u8(&self) -> [u8; MAX_PLANES] {
        let mut out = [0u8; MAX_PLANES];
        for (p, slot) in out.iter_mut().enumerate().take(usize::from(self.planes)) {
            *slot = self.reps(p).min(255) as u8;
        }
        out
    }

    /// DMA descriptors one ring needs: every row shift of a plane is one
    /// descriptor run over that plane's `plane_bytes`, chunked at
    /// `max_chunk` — the same arithmetic as `esp_hub75::dma_descriptor_count`
    /// with the repeats replaced by this schedule's.
    #[must_use]
    pub fn descriptors(&self, plane_bytes: usize, max_chunk: usize) -> usize {
        plane_bytes.div_ceil(max_chunk.max(1)) * self.emissions()
    }

    /// Descriptors from the ring head to the end of the MSB run — plane 0's
    /// repeats — which is the window the spare-plane swap copies inside.
    #[must_use]
    pub fn msb_descriptors(&self, plane_bytes: usize, max_chunk: usize) -> usize {
        plane_bytes.div_ceil(max_chunk.max(1)) * self.reps(0)
    }

    /// For each plane (0 = MSB), the ring descriptor index just PAST its last
    /// descriptor — the point from which the DMA has finished reading that
    /// plane for the current pass and a copy into it cannot tear this pass
    /// (Gitea #829, the chase). `ends[planes − 1]` is the ring length.
    /// Entries past `planes` are 0.
    #[must_use]
    pub fn plane_end_descs(&self, plane_bytes: usize, max_chunk: usize) -> [usize; MAX_PLANES] {
        let chunks = plane_bytes.div_ceil(max_chunk.max(1));
        let mut ends = [0usize; MAX_PLANES];
        let mut acc = 0;
        for (p, slot) in ends.iter_mut().enumerate().take(usize::from(self.planes)) {
            acc += self.reps(p) * chunks;
            *slot = acc;
        }
        ends
    }

    /// Peak brightness as a fraction of the stock schedule's, in permille:
    /// `1000 · lsb · (2^planes − 1) / (W · emissions)` — on-time per unit
    /// TIME, since the pass shortens with the on-time (module docs). 1000 at
    /// full.
    #[must_use]
    pub fn brightness_permille(&self) -> u32 {
        self.brightness_permille_at(Geometry::new(32, usize::from(self.width), usize::from(self.planes)))
    }

    /// [`Schedule::brightness_permille`] against the pass the geometry really
    /// clocks out: the trailing display block ([`Geometry::trail`]) lengthens
    /// a truncated pass by `1/rows` without adding on-time, and the stock pass
    /// has no such block.
    #[must_use]
    pub fn brightness_permille_at(&self, g: Geometry) -> u32 {
        if self.width == 0 || g.rows == 0 {
            return 0;
        }
        let num = u64::from(self.lsb) * self.full_emissions() as u64 * g.rows as u64 * 1000;
        let den = u64::from(self.width) * self.emissions() as u64 * g.blocks() as u64;
        ((num + den / 2) / den) as u32
    }

    /// The `lsb` at the top of step `t` — `W >> t` — where the refresh gain
    /// of truncating `t` planes costs the least brightness. `0` (= full) at
    /// `t = 0`; `None` when `t` is not a valid step for this depth/width.
    #[must_use]
    pub fn step_lsb(&self, t: u8) -> Option<u16> {
        if t >= self.planes {
            return None;
        }
        if t == 0 {
            return Some(0);
        }
        let v = self.width >> t;
        (v >= 1).then_some(v)
    }

    /// Whether this schedule needs the trailing display block
    /// ([`Geometry::trail`]): any truncated plane makes the OE widths differ
    /// between planes, which is what mis-weights the last row.
    #[must_use]
    pub fn needs_trail(&self) -> bool {
        self.trunc > 0
    }

    /// Estimated rescan rate: `clock / (blocks · cols · emissions)`, `blocks`
    /// being `rows` plus the trailing display block when `g` carries one.
    #[must_use]
    pub fn est_hz(&self, g: Geometry, clock_hz: u32) -> u32 {
        let clocks = g.blocks() as u64 * g.cols as u64 * self.emissions() as u64;
        if clocks == 0 {
            return 0;
        }
        (u64::from(clock_hz) / clocks) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    fn bench() -> Geometry {
        Geometry::new(32, 64, 7)
    }

    #[test]
    fn full_is_the_stock_schedule() {
        let s = Schedule::plan(bench(), Control::default(), 0);
        assert_eq!((s.trunc, s.lsb, s.width), (0, 61, 61));
        assert_eq!(s.emissions(), 127);
        assert_eq!(s.full_emissions(), 127);
        for p in 0..7 {
            assert_eq!(s.reps(p), 1 << (6 - p));
            assert_eq!(s.lit(p), 61);
        }
        assert_eq!(s.brightness_permille(), 1000);
        assert_eq!(s.est_hz(bench(), 30_000_000), 115);
        // an lsb at or past the width is the same plan (bar what it remembers
        // it was configured as)
        let at = Schedule::plan(bench(), Control::default(), 61);
        assert_eq!((at.trunc, at.lsb, at.width, at.emissions()), (0, 61, 61, 127));
        assert_eq!(Schedule::plan(bench(), Control::default(), 9999).lsb, 61);
        assert_eq!(s.configured(), 0);
        assert_eq!(at.configured(), 61);
    }

    #[test]
    fn weights_are_exact_powers_of_two_at_every_lsb() {
        for lsb in 1..=61u16 {
            let s = Schedule::plan(bench(), Control::default(), lsb);
            assert_eq!(s.lsb, lsb);
            for p in 0..7 {
                let k = 6 - p;
                assert_eq!(s.on_time(p), usize::from(lsb) << k, "lsb {lsb} plane {p}");
                assert!(s.lit(p) <= 61, "lsb {lsb} plane {p} lit {} > width", s.lit(p));
                assert!(s.lit(p) >= 1);
            }
            // the MSB is always a repeated plane, so the ring still opens
            // with the MSB run
            assert!(s.reps(0) >= 1 && s.trunc < 7);
            // faster, never slower: never more shifts than stock, and the
            // emission count is exactly t + 2^(P − t) − 1
            let t = usize::from(s.trunc);
            assert_eq!(s.emissions(), t + (1 << (7 - t)) - 1);
            assert!(s.emissions() <= 127);
        }
    }

    #[test]
    fn the_refresh_steps_where_the_ticket_says() {
        // #460's numbers on the bench 64x64: T_lsb 16 → 33 shifts → ~4x
        let s = Schedule::plan(bench(), Control::default(), 16);
        assert_eq!(s.trunc, 1); // 16·2 = 32 ≤ 61, 16·4 = 64 > 61
        assert_eq!(s.emissions(), 1 + 63);
        // and the ticket's 33 needs W ≥ 64, i.e. the bare geometry
        let bare = Schedule::plan(bench(), Control::new(0, 0), 16);
        assert_eq!((bare.width, bare.trunc), (64, 2));
        assert_eq!(bare.emissions(), 2 + 31);
        assert_eq!(bare.est_hz(bench(), 30_000_000), 30_000_000 / (32 * 64 * 33));
        // one truncated plane doubles the refresh (bar the one extra shift)
        // at ~2.4 % of brightness — on-time per unit time, not per pass
        let half = Schedule::plan(bench(), Control::default(), 30);
        assert_eq!(half.trunc, 1);
        assert_eq!(half.emissions(), 64);
        assert_eq!(half.brightness_permille(), 976); // 30·127 / (61·64)
        // the step tops: refresh doubles per step, brightness barely moves
        let full = Schedule::plan(bench(), Control::default(), 0);
        let tops: Vec<(u8, u16, usize, u32)> = (0..6u8)
            .map(|t| {
                let lsb = full.step_lsb(t).unwrap();
                let s = Schedule::plan(bench(), Control::default(), lsb);
                (t, lsb, s.emissions(), s.brightness_permille())
            })
            .collect();
        assert_eq!(
            tops,
            vec![
                (0, 0, 127, 1000),
                (1, 30, 64, 976),
                (2, 15, 33, 946),
                (3, 7, 18, 810),
                (4, 3, 11, 568),
                (5, 1, 8, 260),
            ]
        );
        // W >> 6 = 0: there is no sixth step on a 61-clock window, and no
        // step at the depth itself
        assert_eq!(full.step_lsb(6), None);
        assert_eq!(full.step_lsb(7), None);
        // Jeremy's panel (20 MHz, blank 2 → W 59): the three measured points
        let g20 = bench();
        for (lsb, e, b, hz) in [(29u16, 64usize, 975u32, 152u32), (14, 33, 913, 295), (7, 18, 837, 542)] {
            let s = Schedule::plan(g20, Control::new(2, 1), lsb);
            assert_eq!((s.emissions(), s.brightness_permille()), (e, b), "lsb {lsb}");
            assert_eq!(s.est_hz(g20, 20_000_000), hz, "lsb {lsb}");
        }
        // the refresh is monotonic in lsb; within a step the brightness falls
        // linearly, and a step top always beats every position below it
        let mut prev = Schedule::plan(bench(), Control::default(), 61);
        for lsb in (1..61u16).rev() {
            let s = Schedule::plan(bench(), Control::default(), lsb);
            assert!(s.emissions() <= prev.emissions(), "lsb {lsb}");
            if s.trunc == prev.trunc {
                assert!(s.brightness_permille() <= prev.brightness_permille(), "lsb {lsb}");
            }
            prev = s;
        }
    }

    #[test]
    fn a_grey_ramp_is_monotonic_in_displayed_on_time() {
        // Every 7-bit value's on-time is the sum of its set planes' on-times;
        // exact binary weights make that strictly increasing. This is the
        // property the C++ library's version broke.
        for lsb in [1u16, 3, 7, 16, 30, 61] {
            let s = Schedule::plan(bench(), Control::default(), lsb);
            let on = |v: u32| -> usize {
                (0..7).filter(|k| v & (1 << k) != 0).map(|k| s.on_time(6 - k)).sum()
            };
            for v in 1..128u32 {
                assert!(on(v) > on(v - 1), "lsb {lsb} value {v}");
            }
        }
    }

    #[test]
    fn refit_keeps_t_and_the_weights_when_blank_moves() {
        let g = bench();
        // lsb 30 at blank 1: W 61, t 1, L 60
        let s = Schedule::plan(g, Control::new(1, 1), 30);
        assert_eq!((s.trunc, s.lsb, s.lit(0)), (1, 30, 60));
        // blank 2: W 59 — L 60 no longer fits, lsb re-clamps to 29
        let r = s.refit(g, Control::new(2, 1));
        assert_eq!((r.trunc, r.lsb, r.width, r.lit(0)), (1, 29, 59, 58));
        for p in 0..7 {
            assert_eq!(r.on_time(p), 29usize << (6 - p));
        }
        // and back to blank 1 restores the configured value
        assert_eq!(r.refit(g, Control::new(1, 1)), s);
        // a full plan follows the width exactly, like the stock template
        let f = Schedule::plan(g, Control::new(1, 1), 0).refit(g, Control::new(4, 1));
        assert_eq!((f.trunc, f.lsb, f.width), (0, 55, 55));
    }

    #[test]
    fn descriptor_counts_follow_the_repeats() {
        let g = bench();
        let full = Schedule::plan(g, Control::default(), 0);
        // one 4096 B plane is 2 descriptors at the S3's 4092 B chunk
        assert_eq!(full.descriptors(g.plane_bytes(), 4092), 2 * 127);
        assert_eq!(full.msb_descriptors(g.plane_bytes(), 4092), 2 * 64);
        let fast = Schedule::plan(g, Control::default(), 8);
        assert_eq!(fast.trunc, 2);
        assert_eq!(fast.descriptors(g.plane_bytes(), 4092), 2 * (2 + 31));
        assert_eq!(fast.msb_descriptors(g.plane_bytes(), 4092), 2 * 16);
        assert_eq!(fast.reps_u8()[..7], [16, 8, 4, 2, 1, 1, 1]);
        assert_eq!(fast.reps_u8()[7], 0);
        // the chase's per-plane consumption points: cumulative descriptors
        let ends = fast.plane_end_descs(g.plane_bytes(), 4092);
        assert_eq!(ends, [32, 48, 56, 60, 62, 64, 66, 0]);
        assert_eq!(ends[6], fast.descriptors(g.plane_bytes(), 4092));
        assert_eq!(ends[0], fast.msb_descriptors(g.plane_bytes(), 4092));
        let full_ends = full.plane_end_descs(g.plane_bytes(), 4092);
        assert_eq!(full_ends[..7], [128, 192, 224, 240, 248, 252, 254]);
    }

    #[test]
    fn a_template_with_no_lit_clocks_plans_dark() {
        let g = Geometry::new(32, 16, 7);
        let s = Schedule::plan(g, Control::new(8, 3), 0);
        assert_eq!((s.width, s.lsb, s.trunc), (0, 0, 0));
        assert_eq!(s.emissions(), 127);
        assert_eq!(s.brightness_permille(), 0);
        for p in 0..7 {
            assert_eq!(s.lit(p), 0);
        }
    }

    #[test]
    fn eight_planes_and_a_wide_chain() {
        // 256-column chain (a 2x2 of 64x64), 8 planes, the #460 T_lsb 32 row
        let g = Geometry::new(32, 256, 8);
        let s = Schedule::plan(g, Control::default(), 32);
        // 32·8 = 256 > 253, so t = 2 (32·4 = 128 ≤ 253)
        assert_eq!(s.trunc, 2);
        assert_eq!(s.emissions(), 2 + 63);
        assert_eq!(s.reps(0), 32);
        assert_eq!(s.lit(0), 128);
        assert_eq!(s.on_time(0), 32 << 7);
    }
}
