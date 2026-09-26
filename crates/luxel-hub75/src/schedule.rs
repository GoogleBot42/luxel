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
//! rescan is `t + 2^(planes − t) − 1` row shifts, and the panel's peak
//! brightness is `lsb / W` of full. The trade is therefore continuous in `lsb`
//! (brightness) and steps in `t` (refresh): at `lsb = W` the schedule IS the
//! stock one, at `lsb = W/2` the rescan is nearly twice as fast at half the
//! brightness, and so on. Binary weights stay EXACT — plane `k` is lit
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
//! factor, so the weights stay exact and only the peak brightness moves.

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

    /// Peak brightness as a fraction of the stock schedule's, in permille:
    /// `1000 · lsb / W`. 1000 at full.
    #[must_use]
    pub fn on_time_permille(&self) -> u32 {
        if self.width == 0 {
            return 0;
        }
        (u32::from(self.lsb) * 1000 + u32::from(self.width) / 2) / u32::from(self.width)
    }

    /// Estimated rescan rate: `clock / (rows · cols · emissions)`.
    #[must_use]
    pub fn est_hz(&self, g: Geometry, clock_hz: u32) -> u32 {
        let clocks = g.rows as u64 * g.cols as u64 * self.emissions() as u64;
        if clocks == 0 {
            return 0;
        }
        (u64::from(clock_hz) / clocks) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(s.on_time_permille(), 1000);
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
            // brighter ↔ faster: never more shifts than stock, and the
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
        // half brightness doubles the refresh (bar the one extra shift)
        let half = Schedule::plan(bench(), Control::default(), 30);
        assert_eq!(half.trunc, 1);
        assert_eq!(half.emissions(), 64);
        assert_eq!(half.on_time_permille(), 492);
        // the refresh is monotonic in lsb, and so is the brightness
        let mut prev = Schedule::plan(bench(), Control::default(), 61);
        for lsb in (1..61u16).rev() {
            let s = Schedule::plan(bench(), Control::default(), lsb);
            assert!(s.emissions() <= prev.emissions(), "lsb {lsb}");
            assert!(s.on_time_permille() <= prev.on_time_permille(), "lsb {lsb}");
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
    }

    #[test]
    fn a_template_with_no_lit_clocks_plans_dark() {
        let g = Geometry::new(32, 16, 7);
        let s = Schedule::plan(g, Control::new(8, 3), 0);
        assert_eq!((s.width, s.lsb, s.trunc), (0, 0, 0));
        assert_eq!(s.emissions(), 127);
        assert_eq!(s.on_time_permille(), 0);
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
