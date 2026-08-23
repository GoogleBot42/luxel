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
