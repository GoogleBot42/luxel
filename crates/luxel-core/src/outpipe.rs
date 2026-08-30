//! Output pipeline: per-installation transforms applied to the final RGB
//! frame just before protocol encoding — color-order remap (strips differ
//! from their datasheets constantly), a global gamma LUT, and a power cap
//! that scales frames down when their estimated current draw exceeds a
//! configured budget. Pure `no_std` so the firmware and mirror share it
//! and it's testable on the host.

/// Wire color order: which logical channel each output slot carries.
/// Codes are stable (flash-persisted).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ColorOrder(pub u8);

impl ColorOrder {
    pub const RGB: ColorOrder = ColorOrder(0);
    const NAMES: [&'static str; 6] = ["rgb", "rbg", "grb", "gbr", "brg", "bgr"];
    const PERMS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];

    pub fn from_name(s: &str) -> Option<ColorOrder> {
        let lower = s.trim().to_ascii_lowercase();
        Self::NAMES
            .iter()
            .position(|n| *n == lower)
            .map(|i| ColorOrder(i as u8))
    }

    pub fn name(self) -> &'static str {
        Self::NAMES[(self.0 as usize).min(5)]
    }

    pub fn perm(self) -> [usize; 3] {
        Self::PERMS[(self.0 as usize).min(5)]
    }
}

/// Gamma LUT for `gamma_tenths`/10 (e.g. 22 → γ 2.2). 0 and 10 mean "off"
/// (identity would be wasted work — callers skip the LUT entirely).
pub fn gamma_lut(gamma_tenths: u8) -> [u8; 256] {
    use crate::fixed::Fx;
    let g = Fx::from_raw(gamma_tenths as i32 * 65536 / 10);
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let v = Fx::from_raw(((i as i32) << 16) / 255);
        let out = crate::fmath::pow(v, g);
        *slot = ((out.clamp(Fx::ZERO, Fx::ONE).raw() as i64 * 255) >> 16) as u8;
    }
    lut[255] = 255; // full stays full regardless of rounding
    lut
}

/// Perceptual brightness curve for the master dimmer: `brightness5` (0–31)
/// reshaped by `curve_tenths`/10 so a linear slider *feels* linear. 0 and 10
/// mean "off" (identity). γ > 1 pushes the useful range up the slider (the
/// usual fix for "everything above 20% looks the same"); γ < 1 does the
/// opposite. A non-zero input never curves to 0 — a lit strip must stay lit.
///
/// This is the *control* transfer, not the *content* one: [gamma_lut] shapes
/// every pixel's channels, this shapes only the global dimmer.
pub fn curve_brightness(brightness5: u8, curve_tenths: u8) -> u8 {
    use crate::fixed::Fx;
    let b = brightness5 & 0x1F;
    if curve_tenths == 0 || curve_tenths == 10 || b == 0 || b >= 31 {
        return b;
    }
    let g = Fx::from_raw(curve_tenths as i32 * 65536 / 10);
    let v = Fx::from_raw(((b as i32) << 16) / 31);
    let out = crate::fmath::pow(v, g).clamp(Fx::ZERO, Fx::ONE);
    (((out.raw() as i64 * 31 + 32768) >> 16) as u8).max(1)
}

/// Rec.601-ish luma of a frame pixel, 0–255 — the index the palette-remap
/// stage looks colors up by.
pub fn luma(px: [u8; 3]) -> u8 {
    ((px[0] as u32 * 54 + px[1] as u32 * 183 + px[2] as u32 * 19) >> 8) as u8
}

/// Recolor a finished frame through a 256-entry palette LUT indexed by
/// [luma]: the pattern's *structure* survives, its hues are replaced.
/// `amount` is the blend in 1/256ths (256 = full replace, 0 = no-op).
pub fn palette_remap_frame(frame: &mut [[u8; 3]], lut: &[[u8; 3]; 256], amount: u32) {
    if amount == 0 {
        return;
    }
    let a = amount.min(256);
    for px in frame.iter_mut() {
        let target = lut[luma(*px) as usize];
        if a >= 256 {
            *px = target;
            continue;
        }
        for c in 0..3 {
            px[c] = ((px[c] as u32 * (256 - a) + target[c] as u32 * a) >> 8) as u8;
        }
    }
}

/// 3-tap blur along the pixel index, in place and allocation-free (the one
/// value it needs to remember is the previous pixel's pre-blur color).
/// `k` is each neighbor's weight in 1/256ths, 0–128: 128 is a pure neighbor
/// average, 64 the classic 1-2-1 kernel, 0 a no-op. Ends clamp (the last
/// pixel stands in for its own missing neighbor). `passes` widens the
/// effective radius at O(n) each.
///
/// Index space, not map space: on a strip that is physical order, on a
/// serpentine matrix it follows the wiring path.
pub fn blur_frame(frame: &mut [[u8; 3]], k: u32, passes: u8) {
    let k = k.min(128);
    if k == 0 || frame.len() < 2 {
        return;
    }
    let center = 256 - 2 * k;
    let last = frame.len() - 1;
    for _ in 0..passes.max(1) {
        let mut prev = frame[0];
        for i in 0..frame.len() {
            let cur = frame[i];
            let next = frame[(i + 1).min(last)];
            for c in 0..3 {
                let v = cur[c] as u32 * center + prev[c] as u32 * k + next[c] as u32 * k;
                frame[i][c] = ((v + 128) >> 8) as u8;
            }
            prev = cur;
        }
    }
}

/// Light-bleed bloom along the pixel index: each pixel takes the brighter of
/// itself and `g`/256 of its brightest neighbor, so highlights spread without
/// the frame losing energy the way [blur_frame] does. `g` 0 is a no-op.
/// Allocation-free, one pass, same index-space caveat as [blur_frame].
pub fn glow_frame(frame: &mut [[u8; 3]], g: u32) {
    let g = g.min(256);
    if g == 0 || frame.len() < 2 {
        return;
    }
    let last = frame.len() - 1;
    let mut prev = frame[0];
    for i in 0..frame.len() {
        let cur = frame[i];
        let next = frame[(i + 1).min(last)];
        for c in 0..3 {
            let bleed = ((prev[c].max(next[c]) as u32 * g) >> 8) as u8;
            frame[i][c] = cur[c].max(bleed);
        }
        prev = cur;
    }
}

/// Per-output current model for the power cap — a HUB75 matrix draws very
/// differently from an addressable strip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PowerModel {
    /// Addressable strips: every pixel's LEDs conduct simultaneously,
    /// ~20 mA per full channel per pixel (the WS2812-class rule of thumb).
    Strip,
    /// HUB75 matrix: rows are time-multiplexed — only one row-pair per
    /// scan group conducts at any instant — so average draw is the strip
    /// estimate divided by the scan ratio (`scan` = panel rows / 2; 32
    /// for a 1/32-scan 64-row panel). Deliberately conservative: lands
    /// ~2x a typical 64x64 panel's rated full-white draw, and a power
    /// cap should overestimate.
    Hub75 { scan: u16 },
}

/// Estimated frame current in mA under `model`, scaled by the effective
/// brightness.
pub fn estimate_ma(frame: &[[u8; 3]], brightness5: u8, model: PowerModel) -> u32 {
    let sum: u64 = frame
        .iter()
        .map(|px| px[0] as u64 + px[1] as u64 + px[2] as u64)
        .sum();
    let strip = (sum * 20 * (brightness5 & 0x1F) as u64) / (255 * 31);
    match model {
        PowerModel::Strip => strip as u32,
        PowerModel::Hub75 { scan } => (strip / scan.max(1) as u64) as u32,
    }
}

/// Apply the pipeline in place: gamma LUT (if any), color-order remap, and
/// the power cap (uniform scale when the estimate exceeds `cap_ma`; 0 = no
/// cap). Returns the scale numerator/256 applied (255+ = none) for status.
pub fn apply(
    frame: &mut [[u8; 3]],
    order: ColorOrder,
    lut: Option<&[u8; 256]>,
    cap_ma: u32,
    brightness5: u8,
    model: PowerModel,
) -> u32 {
    if let Some(lut) = lut {
        for px in frame.iter_mut() {
            for c in px.iter_mut() {
                *c = lut[*c as usize];
            }
        }
    }
    if order != ColorOrder::RGB {
        let p = order.perm();
        for px in frame.iter_mut() {
            *px = [px[p[0]], px[p[1]], px[p[2]]];
        }
    }
    let mut scale = 256u32;
    if cap_ma > 0 {
        let est = estimate_ma(frame, brightness5, model);
        if est > cap_ma {
            scale = (cap_ma * 256 / est).max(1);
            for px in frame.iter_mut() {
                for c in px.iter_mut() {
                    *c = ((*c as u32 * scale) >> 8) as u8;
                }
            }
        }
    }
    scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_round_trip_and_remap() {
        assert_eq!(ColorOrder::from_name("GRB").unwrap().0, 2);
        assert_eq!(ColorOrder(5).name(), "bgr");
        assert!(ColorOrder::from_name("xyz").is_none());
        let mut f = [[10, 20, 30]];
        apply(&mut f, ColorOrder::from_name("grb").unwrap(), None, 0, 31, PowerModel::Strip);
        assert_eq!(f[0], [20, 10, 30]); // wire gets G,R,B
        let mut f = [[10, 20, 30]];
        apply(&mut f, ColorOrder::from_name("bgr").unwrap(), None, 0, 31, PowerModel::Strip);
        assert_eq!(f[0], [30, 20, 10]);
    }

    #[test]
    fn gamma_curves_darken_midtones() {
        let lut = gamma_lut(22); // γ 2.2
        assert_eq!(lut[0], 0);
        assert_eq!(lut[255], 255);
        assert!(lut[128] < 60, "γ2.2 midpoint ≈ 55, got {}", lut[128]);
        // monotonic
        assert!(lut.windows(2).all(|w| w[0] <= w[1]));
        let mut f = [[128, 128, 128]];
        apply(&mut f, ColorOrder::RGB, Some(&lut), 0, 31, PowerModel::Strip);
        assert_eq!(f[0][0], lut[128]);
    }

    #[test]
    fn blur_spreads_a_spike_and_conserves_ends() {
        // k = 64 is the 1-2-1 kernel: 255 → 128 with 64 either side
        let mut f = [[0u8; 3], [0; 3], [255; 3], [0; 3], [0; 3]];
        blur_frame(&mut f, 64, 1);
        assert_eq!(f.map(|p| p[0]), [0, 64, 128, 64, 0]);
        // a second pass widens it further (the ends clamp, so light that
        // reaches pixel 0 stays there rather than falling off the strip)
        blur_frame(&mut f, 64, 1);
        assert_eq!(f.map(|p| p[0]), [16, 64, 96, 64, 16]);
        // k = 0 and single-pixel frames are no-ops
        let mut g = [[7u8, 8, 9], [1, 2, 3]];
        blur_frame(&mut g, 0, 4);
        assert_eq!(g, [[7, 8, 9], [1, 2, 3]]);
        // a flat frame survives a full-strength blur unchanged
        let mut flat = [[100u8; 3]; 6];
        blur_frame(&mut flat, 128, 3);
        assert_eq!(flat, [[100u8; 3]; 6]);
    }

    #[test]
    fn glow_bleeds_without_dimming() {
        let mut f = [[0u8; 3], [0; 3], [255; 3], [0; 3], [0; 3]];
        glow_frame(&mut f, 128); // neighbours at half strength
        assert_eq!(f.map(|p| p[0]), [0, 127, 255, 127, 0]);
        // unlike blur, the source pixel keeps its full value
        let mut g = [[0u8; 3], [200; 3], [0; 3]];
        glow_frame(&mut g, 0);
        assert_eq!(g.map(|p| p[0]), [0, 200, 0]);
    }

    #[test]
    fn palette_remap_recolors_by_luma() {
        // luma of pure green = (255·183) >> 8 = 182
        assert_eq!(luma([0, 255, 0]), 182);
        assert_eq!(luma([255, 255, 255]), 255);
        assert_eq!(luma([0, 0, 0]), 0);
        // a table that turns every luma into "red at that level"
        let mut lut = [[0u8; 3]; 256];
        for (i, slot) in lut.iter_mut().enumerate() {
            *slot = [i as u8, 0, 0];
        }
        let mut f = [[0u8, 255, 0], [255, 255, 255]];
        palette_remap_frame(&mut f, &lut, 256);
        assert_eq!(f, [[182, 0, 0], [255, 0, 0]]);
        // half strength blends with the original
        let mut h = [[0u8, 255, 0]];
        palette_remap_frame(&mut h, &lut, 128);
        assert_eq!(h, [[91, 127, 0]]);
        // amount 0 is a no-op
        let mut z = [[0u8, 255, 0]];
        palette_remap_frame(&mut z, &lut, 0);
        assert_eq!(z, [[0, 255, 0]]);
    }

    #[test]
    fn brightness_curve_reshapes_the_dimmer() {
        // off (0 and 10) is identity, and so are the endpoints
        for b in 0..=31u8 {
            assert_eq!(curve_brightness(b, 0), b);
            assert_eq!(curve_brightness(b, 10), b);
        }
        assert_eq!(curve_brightness(0, 22), 0);
        assert_eq!(curve_brightness(31, 22), 31);
        // γ2.2 pushes the useful range up the slider: half-way is much dimmer
        let mid = curve_brightness(16, 22);
        assert!((6..=9).contains(&mid), "16/31 at γ2.2 = {mid}");
        // monotonic, and a lit strip never curves to off
        let mut prev = 0;
        for b in 1..=31u8 {
            let v = curve_brightness(b, 22);
            assert!(v >= 1, "brightness {b} curved to 0");
            assert!(v >= prev, "not monotonic at {b}: {prev} → {v}");
            prev = v;
        }
        // γ < 1 goes the other way
        assert!(curve_brightness(16, 5) > 16);
    }

    #[test]
    fn power_cap_scales_hot_frames() {
        // 100 full-white pixels at full brightness ≈ 100 × 60 mA = 6 A
        let mut f = [[255u8, 255, 255]; 100];
        assert!((estimate_ma(&f, 31, PowerModel::Strip) as i64 - 6000).abs() < 100);
        let scale = apply(&mut f, ColorOrder::RGB, None, 3000, 31, PowerModel::Strip);
        assert!(scale < 256);
        let after = estimate_ma(&f, 31, PowerModel::Strip);
        assert!(after <= 3050, "capped estimate = {after}");
        // dim frames pass untouched
        let mut dim = [[10u8, 0, 0]; 100];
        let s2 = apply(&mut dim, ColorOrder::RGB, None, 3000, 31, PowerModel::Strip);
        assert_eq!(s2, 256);
        assert_eq!(dim[0], [10, 0, 0]);
    }

    #[test]
    fn hub75_model_divides_by_scan_ratio() {
        // full-white 64x64 panel: strip math says 4096 × 60 mA ≈ 246 A;
        // 1/32-scan time multiplexing lands it near 7.7 A (≈2x a typical
        // panel's rated draw — conservative by design)
        let f = alloc::vec![[255u8, 255, 255]; 4096];
        let strip = estimate_ma(&f, 31, PowerModel::Strip);
        let panel = estimate_ma(&f, 31, PowerModel::Hub75 { scan: 32 });
        assert_eq!(panel, strip / 32);
        assert!((7_000..9_000).contains(&panel), "panel estimate = {panel}");
        // the cap engages against the panel model, not the strip one
        let mut hot = alloc::vec![[255u8, 255, 255]; 4096];
        let scale = apply(
            &mut hot,
            ColorOrder::RGB,
            None,
            4_000,
            31,
            PowerModel::Hub75 { scan: 32 },
        );
        assert!(scale < 256);
        let after = estimate_ma(&hot, 31, PowerModel::Hub75 { scan: 32 });
        assert!(after <= 4_050, "capped panel estimate = {after}");
    }
}
