// name: sound - spectrokalidamandala
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - spectrokalidamandala"; original source never consulted.

// Concentric spectrum rings pulse from the center of a 2D map. Each ring maps
// to one of 32 frequency bands; a band spiking above its own moving average
// flashes its rings, which fade over a few frames. Hue = band position plus a
// mirrored angular term (the mandala). The whole image slowly breathes between
// ~0.5x and ~3.5x zoom, rings drift radially on a ~10 s cycle, and a PI gain
// controller holds average brightness near a target fill. With no sensor board
// (ambient-light sentinel untouched), a simulated dance loop feeds the bands.

export var frequencyData = array(32)   // 32-band spectrum, low bands first
export var energyAverage
export var maxFrequency
export var maxFrequencyMagnitude
export var light = -1                  // sentinel: stays -1 when no board

const TARGET_FILL = 0.33     // desired average pixel brightness
const AVG_WINDOW = 1500      // band moving-average window, ms
const FADE = 0.7             // persistence kept per frame
const ATTACK = 0.7           // novelty attack factor
const DRIFT_T = 0.15         // ring drift, ~10 s cycle
const BREATHE_T = 0.9        // zoom breathing, ~59 s cycle
const INTEGRAL_MAX = 300

var bandAvg = array(32)
var persist = array(pixelCount)
var integral = 0
var sensitivity = 1
var feedback = 0             // brightness sum from last render pass
var drift = 0
var zoom = 1
var simAccum = 0             // delta accumulator for 40 Hz simulated sound
var simT = 0                 // simulated-song position, measures (~2 s each)
var wanderT = 0              // slow clock for the lead-synth random walk

export function showNumberZoom() { return zoom }
export function showNumberSensitivity() { return sensitivity }
export function showNumberIntegral() { return integral }

// Simulated four-on-the-floor loop, called at ~40 Hz when no board is present
function simulateSound() {
  simT += 0.0125                     // 40 updates/s over a 2 s measure
  if (simT >= 1) simT -= 1
  wanderT += 0.0125 / 32             // wanders over a ~64 s supercycle
  if (wanderT >= 1) wanderT -= 1

  var i
  for (i = 0; i < 32; i++) frequencyData[i] = frequencyData[i] * 0.55

  // kick: 4 per measure, sharply concave attack, splayed across low third
  var kick = pow(1 - frac(simT * 4), 4)
  for (i = 0; i < 10; i++) frequencyData[i] += kick * (1 - i / 12) * 0.8

  // clap: offbeats in the low-mids, randomized band and strength
  var off = pow(1 - frac(simT * 2 + 0.5), 3)
  if (off > 0.05) {
    frequencyData[10 + floor(random(4))] += off * (0.4 + random(0.4))
  }

  // hi-hat: beats 2 and 4 in a couple of upper-mid bands
  frequencyData[22] += off * 0.5
  frequencyData[24] += off * 0.3

  // lead synth: a smooth walk from incommensurate unit-period waves
  // (integer multiples of a wrapping clock keep the walk continuous at wrap)
  var w = (triangle(wanderT * 3) + wave(wanderT * 5) + triangle(wanderT * 7)) / 3
  var lead = 14 + floor(w * 16)
  var pulse = pow(1 - frac(simT * 8), 2) * 0.6
  frequencyData[lead] += pulse
  if (random(1) < 0.4) frequencyData[min(31, lead + 3)] += pulse * 0.7
}

export function beforeRender(delta) {
  // 1. PI gain control against last frame's average brightness
  var err = TARGET_FILL - feedback / pixelCount
  integral = clamp(integral + err * delta * 0.001, 0, INTEGRAL_MAX)
  sensitivity = max(0.02, err * 2 + integral)
  feedback = 0

  // 2. radial ring drift
  drift = time(DRIFT_T)

  // 3. simulated sound when no sensor board ever wrote the light sentinel
  if (light == -1) {
    simAccum += delta
    while (simAccum >= 25) {
      simAccum -= 25
      simulateSound()
    }
  }

  // 4. per-band exponential moving averages (with tiny floor)
  var w = min(1, delta / AVG_WINDOW)
  var i
  for (i = 0; i < 32; i++) {
    var e = min(20, frequencyData[i] * sensitivity)
    bandAvg[i] = clamp(bandAvg[i] + (e - bandAvg[i]) * w, 0.001, 10)
  }

  // 5. breathing zoom around the recentered map
  zoom = 2 + 1.5 * sin(time(BREATHE_T) * PI2)   // ~0.5 .. ~3.5
  resetTransform()
  translate(-0.5, -0.5)
  scale(zoom, zoom)
}

export function render2D(index, x, y) {
  // ring index: fold distance, subtract drift, fold again -> mirrored copies
  var r = hypot(x, y)
  var fi = triangle(triangle(r) - drift) * 31
  var i0 = min(30, floor(fi))
  var f = fi - i0

  // interpolated current energy and moving average at the fractional band
  var cur = min(20, mix(frequencyData[i0], frequencyData[i0 + 1], f) * sensitivity)
  var avg = mix(bandAvg[i0], bandAvg[i0 + 1], f)

  // novelty: change over the running average, weighted by typical energy,
  // clipped at zero and squared for gamma-like emphasis
  var v = (cur - avg) * (10 + 10 * avg)
  if (v < 0) v = 0
  v = min(v, 3)
  v = v * v

  // persistence: decay then attack
  persist[index] = persist[index] * FADE + v * ATTACK
  var b = persist[index]

  // hue: band fraction + mirrored angular term (up to half the wheel)
  var hue = fi / 32 + 0.5 * triangle(atan2(y, x) / PI2)

  // whiteout past full brightness
  var s = b <= 1 ? 1 : max(0, 1 - (b - 1))

  feedback += clamp(b, 0, 1)
  hsv(hue, s, b)
}
