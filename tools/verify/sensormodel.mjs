// Synthetic PB sensor-board feed ("beat120") — shared by snap.mjs (the judge
// harness) and report.mjs (the visual report generator) so the two can never
// drift apart. See snap.mjs's header for how the feed is applied.
//
// A deterministic stand-in for the PB sensor expansion board. Everything is a
// pure function of (frameIndex, fps) — no Date, no RNG — so the two sides get
// byte-identical input and a rerun reproduces a run exactly.
//
// Band geometry mirrors the engine's own analyzer (crates/luxel-core/src/
// audio.rs): 32 log-spaced bands from 37 Hz to 10 kHz, each band a fixed ratio
// wider than the last. Levels are normalized 0..1 like parse_sensor_board's
// u16 fields; maxFrequency is in Hz; accelerometer follows netin.rs's
// "±0.5 = ±full-scale" convention.

export const SENSOR_MODEL = "beat120";

const BANDS = 32;
const BAND_LO_HZ = 37;
const BAND_HI_HZ = 10_000;
const BAND_RATIO = (BAND_HI_HZ / BAND_LO_HZ) ** (1 / BANDS);

/** Fractional band index whose centre frequency is `hz` (inverse of audio.rs's
 *  `LO·ratio^(b+0.5)` band centres). */
const binOfHz = (hz) => Math.log(hz / BAND_LO_HZ) / Math.log(BAND_RATIO) - 0.5;

const BEAT_HZ = 2; // 120 BPM
const BEAT_ATTACK = 0.04; // fraction of the beat spent rising: near-instant
const BEAT_DECAY = 0.18; // exponential decay constant, in beats
const ENERGY_BASE = 0.15;
const ENERGY_SWING = 0.55; // → energyAverage sweeps 0.15 … 0.70
const SPECTRUM_FLOOR = 0.02; // a hair of room tone between beats
const BASS_BIN = 1.5; // ≈ 48 Hz
const BASS_WIDTH = 1.8;
const BASS_AMP = 0.7;
const MELODY_ROOT_HZ = 330; // ≈ band 12, comfortably mid
const MELODY_SCALE = [0, 2, 4, 5, 7, 9, 11, 12]; // semitones, one step per beat
const MELODY_WIDTH = 1.0;
const MELODY_AMP = 0.9; // loudest peak, so maxFrequency tracking it is honest
const SHIMMER_BIN = 28; // ≈ 6.5 kHz
const SHIMMER_WIDTH = 3.0;
const SHIMMER_AMP = 0.12;
const SHIMMER_HZ = 7; // shimmer's own slow tremolo
const TILT_PERIOD_S = 8;
const TILT_AMPLITUDE = 0.2;
const TILT_Z = 0.25; // resting gravity term (±0.5 = ±full-scale)
const LIGHT_LEVEL = 0.5;

const clamp01 = (v) => (v < 0 ? 0 : v > 1 ? 1 : v);
/** Unit-height gaussian bump at `centre` bins, `width` bins of spread. */
const bump = (b, centre, width) => Math.exp(-(((b - centre) / width) ** 2));

/** One synthetic sensor frame for absolute frame `i` at `fps`. */
export function synthSensorFrame(i, fps) {
  const t = i / fps;
  const beat = t * BEAT_HZ;
  const phase = beat - Math.floor(beat); // 0 at each downbeat
  const env =
    phase < BEAT_ATTACK
      ? phase / BEAT_ATTACK
      : Math.exp(-(phase - BEAT_ATTACK) / BEAT_DECAY);

  const step = Math.floor(beat) % MELODY_SCALE.length;
  const melodyHz = MELODY_ROOT_HZ * 2 ** (MELODY_SCALE[step] / 12);
  const melodyBin = binOfHz(melodyHz);
  const shimmer = SHIMMER_AMP * (0.5 + 0.5 * Math.sin(2 * Math.PI * SHIMMER_HZ * t));

  const frequencyData = new Array(BANDS);
  for (let b = 0; b < BANDS; b++) {
    const spectrum =
      BASS_AMP * bump(b, BASS_BIN, BASS_WIDTH) +
      MELODY_AMP * bump(b, melodyBin, MELODY_WIDTH) +
      shimmer * bump(b, SHIMMER_BIN, SHIMMER_WIDTH);
    frequencyData[b] = clamp01(SPECTRUM_FLOOR + env * spectrum);
  }

  const tilt = (2 * Math.PI * t) / TILT_PERIOD_S;
  return {
    frequencyData,
    energyAverage: clamp01(ENERGY_BASE + ENERGY_SWING * env),
    maxFrequencyMagnitude: clamp01(SPECTRUM_FLOOR + env * MELODY_AMP),
    maxFrequency: melodyHz,
    light: LIGHT_LEVEL,
    accelerometer: [
      TILT_AMPLITUDE * Math.sin(tilt),
      TILT_AMPLITUDE * Math.cos(tilt),
      TILT_Z,
    ],
    analogInputs: [0, 0, 0, 0, 0],
  };
}
