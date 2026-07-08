//! On-device audio analysis: PCM samples → the PB sensor-board surface
//! (32 log-spaced bands 37 Hz–10 kHz, energyAverage, maxFrequency).
//! `no_std` + fixed-point throughout, so the same code runs on the ESP32
//! (fed by an I2S/PDM microphone) and in host tests. The FFT is a plain
//! 512-point radix-2 with per-stage scaling — plenty for 32 bands at
//! LED frame rates.

use alloc::vec::Vec;

use crate::engine::SensorFrame;
use crate::fixed::Fx;

pub const FFT_N: usize = 512;
const BANDS: usize = 32;
const LO_HZ: u32 = 37;
const HI_HZ: u32 = 10_000;

/// Q15 sine table covering one full turn in FFT_N steps (twiddles index it).
fn sin_q15_table() -> Vec<i32> {
    (0..FFT_N)
        .map(|i| {
            let turns = Fx::from_raw(((i as i64 * 65536) / FFT_N as i64) as i32);
            (crate::fmath::sin_turns(turns).raw() >> 1).clamp(-32767, 32767)
        })
        .collect()
}

pub struct AudioAnalyzer {
    sample_rate: u32,
    sin: Vec<i32>,     // Q15 sine, FFT_N entries (cos = sin phase-shifted)
    window: Vec<i32>,  // Q15 Hann
    buf: Vec<i16>,     // accumulating input samples
    band_edges: Vec<(usize, usize)>, // FFT-bin [start, end) per band
    re: Vec<i32>,
    im: Vec<i32>,
}

impl AudioAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        let sin = sin_q15_table();
        // Hann: 0.5·(1 − cos(2πn/N)) in Q15, via the same table
        let cos_at = |i: usize| sin[(i + FFT_N / 4) % FFT_N];
        let window: Vec<i32> = (0..FFT_N).map(|n| 16384 - (cos_at(n) >> 1)).collect();
        // log-spaced band edges over the positive-frequency bins
        let hz_per_bin = sample_rate as f32 / FFT_N as f32;
        let mut band_edges = Vec::with_capacity(BANDS);
        for b in 0..BANDS {
            let f0 = LO_HZ as f32 * libm_powf(HI_HZ as f32 / LO_HZ as f32, b as f32 / BANDS as f32);
            let f1 =
                LO_HZ as f32 * libm_powf(HI_HZ as f32 / LO_HZ as f32, (b + 1) as f32 / BANDS as f32);
            let s = ((f0 / hz_per_bin) as usize).max(1);
            let e = ((f1 / hz_per_bin) as usize).max(s + 1).min(FFT_N / 2);
            band_edges.push((s.min(FFT_N / 2 - 1), e));
        }
        Self {
            sample_rate,
            sin,
            window,
            buf: Vec::with_capacity(FFT_N),
            band_edges,
            re: alloc::vec![0; FFT_N],
            im: alloc::vec![0; FFT_N],
        }
    }

    /// Feed PCM; returns a sensor frame each time FFT_N samples accumulate.
    pub fn push_samples(&mut self, samples: &[i16]) -> Option<SensorFrame> {
        let mut out = None;
        for &s in samples {
            self.buf.push(s);
            if self.buf.len() == FFT_N {
                out = Some(self.analyze());
                self.buf.clear();
            }
        }
        out
    }

    fn analyze(&mut self) -> SensorFrame {
        // window (Q15·i16 → keep ~16 bits) + bit-reversal permutation load
        let bits = FFT_N.trailing_zeros();
        for i in 0..FFT_N {
            let j = (i as u32).reverse_bits() >> (32 - bits);
            let w = (self.buf[i] as i32 * self.window[i]) >> 15;
            self.re[j as usize] = w;
            self.im[j as usize] = 0;
        }
        // radix-2 DIT with >>1 per stage (overall /N — no overflow)
        let mut half = 1;
        while half < FFT_N {
            let step = FFT_N / (2 * half);
            for start in (0..FFT_N).step_by(2 * half) {
                for k in 0..half {
                    let tw = k * step;
                    let (c, s) = (
                        self.sin[(tw + FFT_N / 4) % FFT_N], // cos
                        -self.sin[tw],                      // −sin (forward)
                    );
                    let (a, b) = (start + k, start + k + half);
                    // i64 products: |re|·|c| pairs can graze i32::MAX at
                    // full-scale input
                    let tr =
                        ((self.re[b] as i64 * c as i64 - self.im[b] as i64 * s as i64) >> 15) as i32;
                    let ti =
                        ((self.re[b] as i64 * s as i64 + self.im[b] as i64 * c as i64) >> 15) as i32;
                    let (ar, ai) = (self.re[a] >> 1, self.im[a] >> 1);
                    self.re[a] = ar + (tr >> 1);
                    self.im[a] = ai + (ti >> 1);
                    self.re[b] = ar - (tr >> 1);
                    self.im[b] = ai - (ti >> 1);
                }
            }
            half *= 2;
        }
        // magnitudes + banding. A full-scale sine ends up at ~N/4-scaled
        // amplitude after the per-stage shifts; NORM makes it ≈ 1.0.
        let mag = |k: usize| -> i64 {
            let (re, im) = (self.re[k] as i64, self.im[k] as i64);
            isqrt((re * re + im * im) as u64) as i64
        };
        const NORM: i64 = 8192; // measured with the full-scale-sine test
        let mut frame = SensorFrame::default();
        let mut energy: i64 = 0;
        for (b, &(s, e)) in self.band_edges.iter().enumerate() {
            let mut sum: i64 = 0;
            for k in s..e {
                sum += mag(k);
            }
            let avg = sum / (e - s) as i64;
            let v = ((avg << 16) / NORM).min(65536) as i32;
            frame.frequency_data[b] = Fx::from_raw(v);
            energy += v as i64;
        }
        frame.energy_average = Fx::from_raw((energy / BANDS as i64) as i32);
        // loudest raw bin in range → Hz
        let lo = self.band_edges[0].0;
        let hi = self.band_edges[BANDS - 1].1;
        let (mut max_k, mut max_v) = (lo, 0i64);
        for k in lo..hi {
            let m = mag(k);
            if m > max_v {
                max_v = m;
                max_k = k;
            }
        }
        frame.max_frequency =
            Fx::from_int(((max_k as u64 * self.sample_rate as u64) / FFT_N as u64) as i32);
        frame.max_frequency_magnitude = Fx::from_raw((((max_v << 16) / NORM).min(65536)) as i32);
        frame
    }
}

/// Integer square root (u64 → u32-ish), simple Newton iteration.
fn isqrt(v: u64) -> u64 {
    if v == 0 {
        return 0;
    }
    let mut x = 1u64 << ((64 - v.leading_zeros() + 1) / 2);
    loop {
        let nx = (x + v / x) / 2;
        if nx >= x {
            return x;
        }
        x = nx;
    }
}

/// Tiny powf for the (compile-time-ish) band-edge setup only — exp/ln via
/// the fixed-point fmath would lose precision here; a float approximation
/// with a few Newton terms is fine for band edges.
fn libm_powf(base: f32, exp: f32) -> f32 {
    // base^exp = e^(exp·ln base); ln/exp via f64 in std tests, and via a
    // small series in no_std. Band edges only need ~0.1% accuracy.
    exp2f(exp * log2f(base))
}

fn log2f(x: f32) -> f32 {
    let bits = x.to_bits();
    let e = ((bits >> 23) & 0xff) as i32 - 127;
    let m = f32::from_bits((bits & 0x007f_ffff) | 0x3f80_0000); // 1..2
    // minimax-ish log2(m) for m in [1,2)
    let t = m - 1.0;
    e as f32 + t * (1.4426951 + t * (-0.7181452 + t * (0.4451067 + t * -0.1421344)))
}

fn exp2f(x: f32) -> f32 {
    let i = if x < 0.0 { (x - 0.9999) as i32 } else { x as i32 };
    let f = x - i as f32; // 0..1
    let p = 1.0 + f * (0.6931472 + f * (0.2401597 + f * (0.0558263 + f * 0.0089893)));
    f32::from_bits((((i + 127) as u32) << 23)) * p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: f64, n: usize, amp: f64) -> Vec<i16> {
        (0..n)
            .map(|i| {
                let t = i as f64 / rate;
                ((t * freq * core::f64::consts::TAU).sin() * amp * 32767.0) as i16
            })
            .collect()
    }

    #[test]
    fn silence_is_zero() {
        let mut a = AudioAnalyzer::new(20_000);
        let f = a.push_samples(&[0i16; FFT_N]).expect("frame");
        assert_eq!(f.energy_average, Fx::ZERO);
        assert!(f.frequency_data.iter().all(|v| *v == Fx::ZERO));
    }

    #[test]
    fn full_scale_sine_hits_its_band() {
        let mut a = AudioAnalyzer::new(20_000);
        let f = a.push_samples(&sine(440.0, 20_000.0, FFT_N, 1.0)).expect("frame");
        // peak frequency within one bin (~39 Hz at 20 kHz / 512)
        let hz = f.max_frequency.to_f64();
        assert!((hz - 440.0).abs() < 45.0, "maxFrequency = {hz}");
        assert!(
            f.max_frequency_magnitude.to_f64() > 0.5,
            "magnitude = {}",
            f.max_frequency_magnitude.to_f64()
        );
        // the band containing 440 Hz is the strongest
        let strongest = f
            .frequency_data
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| v.raw())
            .unwrap()
            .0;
        // band edges: 37·(10000/37)^(b/32) — 440 Hz lands in band ~14
        assert!((13..=15).contains(&strongest), "strongest band = {strongest}");
        assert!(f.energy_average > Fx::ZERO);
    }

    #[test]
    fn two_tones_two_bands() {
        let mut a = AudioAnalyzer::new(20_000);
        let low = sine(100.0, 20_000.0, FFT_N, 0.5);
        let high = sine(4000.0, 20_000.0, FFT_N, 0.5);
        let mixed: Vec<i16> = low
            .iter()
            .zip(&high)
            .map(|(a, b)| (*a as i32 + *b as i32).clamp(-32768, 32767) as i16)
            .collect();
        let f = a.push_samples(&mixed).expect("frame");
        // both regions light up well above the quiet bands
        let band_of = |hz: f64| ((hz / 37.0).ln() / (10_000f64 / 37.0).ln() * 32.0) as usize;
        let quiet = f.frequency_data[band_of(1000.0)].to_f64();
        assert!(f.frequency_data[band_of(100.0)].to_f64() > quiet + 0.05);
        assert!(f.frequency_data[band_of(4000.0)].to_f64() > quiet + 0.05);
    }
}
