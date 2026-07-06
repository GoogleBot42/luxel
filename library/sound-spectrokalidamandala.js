// name: sound - spectrokalidamandala
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - spectrokalidamandala"; original source never
// consulted.

// Concentric spectrum rings pulse around the center of a 2D map. Each
// ring maps to one of 32 audio bands; a band spiking above its own recent
// moving average flashes its rings, which then fade through a per-pixel
// persistence buffer. Hue = frequency position + a mirrored angular term
// (the mandala). Rings drift radially on a ~10 s cycle and the whole
// image breathes between ~0.5x and ~3.5x zoom over ~a minute. A PI
// controller servos sensitivity so average brightness sits near a target
// fill at any volume. If no sensor board is present (the ambient-light
// sentinel never changes), a synthesized four-on-the-floor loop feeds the
// spectrum instead.

// --- sensor bindings (engine overwrites these when a board is present) ---
export var frequencyData = array(32)
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0
export var accelerometer = array(3)
export var light = -1          // negative sentinel: "no board detected"

// --- tuning constants ---
var BANDS = 32
var TARGET_FILL = 0.33         // desired average pixel brightness
var AVG_WINDOW = 1.5           // seconds, band moving-average horizon
var DRIFT_T = 0.15             // time() interval -> ~10 s radial drift
var FADE = 0.72                // persistence kept per frame
var ATTACK = 0.7               // novelty blended in per frame
var KP = 4                     // proportional gain
var KI = 3                     // integral gain per second
var I_MAX = 40                 // integral ceiling
var SENS_FLOOR = 0.02          // never fully dead

// --- state ---
var averages = array(BANDS)
var pix = array(pixelCount)    // per-pixel brightness persistence
var integral = 0
var sensitivity = 1
var briSum = 0                 // brightness fed back from last render pass
var drift = 0
var zoom = 1
var simAccum = 0

export function showNumberZoom() { return zoom }
export function showNumberSensitivity() { return sensitivity }
export function showNumberIntegral() { return integral }

// Smooth pseudo-random walk in 0..1 from incommensurate waves of
// relatively prime-ish periods; continuous when time() wraps.
function wander() {
  return frac((triangle(time(0.013)) + wave(time(0.007)) + triangle(time(0.031))) / 2)
}

// Synthesized dance loop, called at ~40 Hz when no board is attached.
function simStep() {
  var i
  // Fast per-band decay so hits read as transients.
  for (i = 0; i < BANDS; i++) frequencyData[i] *= 0.55

  var m = time(0.03)             // ~2 s per measure
  // Kick: four to the floor, sharply concave attack, low ~third of bands.
  var kickPhase = frac(m * 4)
  var kick = (1 - kickPhase) * (1 - kickPhase) * (1 - kickPhase)
  for (i = 0; i < 11; i++) frequencyData[i] += kick * (1 - i / 12) * 0.5

  // Clap: offbeats, low-mid bands, a little randomness in where and how much.
  var clapPhase = frac(m * 4 + 0.5)
  if (clapPhase < 0.12) {
    frequencyData[8 + floor(random(6))] += 0.25 + random(0.35)
  }

  // Hi-hats: beats two and four, a couple of upper-mid bands.
  var hatPhase = frac(m * 2 + 0.5)
  if (hatPhase < 0.08) {
    frequencyData[20] += 0.5
    frequencyData[22] += 0.3
  }

  // Lead synth: one wandering band, sometimes a harmony a few steps up.
  var lead = 12 + floor(wander() * 16)
  frequencyData[lead] += 0.45
  if (random(1) < 0.4) frequencyData[min(BANDS - 1, lead + 4)] += 0.3
}

export function beforeRender(delta) {
  var dt = delta / 1000
  var i

  // 1. PI gain control from last frame's brightness feedback.
  var avgBri = pixelCount > 0 ? briSum / pixelCount : 0
  var err = TARGET_FILL - avgBri
  integral = clamp(integral + err * KI * dt, 0, I_MAX)
  sensitivity = max(SENS_FLOOR, KP * err + integral)
  briSum = 0

  // 2. Radial ring drift, ~10 s cycle.
  drift = time(DRIFT_T)

  // 3. Simulated sound at a fixed ~40 Hz when no board ever spoke.
  if (light < 0) {
    simAccum = min(simAccum + delta, 200)
    while (simAccum >= 25) {
      simAccum -= 25
      simStep()
    }
  }

  // 4. Exponential moving average per band, ~1.5 s window.
  var w = min(1, dt / AVG_WINDOW)
  for (i = 0; i < BANDS; i++) {
    averages[i] = max(0.0001,
      averages[i] + (frequencyData[i] * sensitivity - averages[i]) * w)
  }

  // 5. Breathing zoom over the platform's long default cycle (~65 s).
  zoom = 2 + 1.5 * sin(time(1) * PI2)   // ~0.5 .. ~3.5
  resetTransform()
  translate(-0.5, -0.5)                 // origin to map center
  scale(zoom, zoom)
}

// Linear interpolation into a 32-band array at a fractional index.
function sampleBands(a, f) {
  var i0 = floor(f)
  var i1 = min(BANDS - 1, i0 + 1)
  return mix(a[i0], a[i1], f - i0)
}

export function render2D(index, x, y) {
  // Ring index: distance folded, drift subtracted, folded again ->
  // mirrored repeating spectrum copies (the kaleidoscope).
  var r = hypot(x, y)
  var bandFrac = triangle(triangle(r) - drift)
  var fIdx = bandFrac * (BANDS - 1)

  var cur = sampleBands(frequencyData, fIdx)
  var avg = sampleBands(averages, fIdx)

  // Novelty: change over the running average, weighted so habitually
  // strong bands flash harder; clip negatives, square for gamma emphasis.
  var val = (cur * sensitivity - avg) * (10 + 10 * avg)
  val = clamp(val, 0, 3)
  val = val * val

  // Persistence: decay-plus-attack is the displayed brightness.
  pix[index] = pix[index] * FADE + val * ATTACK
  var v = pix[index]

  // Mandala hue: frequency position + mirrored angular shift.
  var hue = bandFrac + triangle(atan2(y, x) / PI2) * 0.5

  // Whiteout past unity; feed clamped brightness to the controller.
  var bri = clamp(v, 0, 1)
  var sat = clamp(2 - v, 0, 1)
  briSum += bri
  hsv(hue, sat, bri)
}
