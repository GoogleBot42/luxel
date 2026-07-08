// name: Light Organ - 2.0
// Clean-room reimplementation from a prose functional description of the
// community pattern "Light Organ - 2.0"; original source never consulted.

// A music "light organ" for a 1D strip. Four solid colored bars tile across the
// strip; each bar's brightness pulses with the loudness of one spectral region
// (bass / low / mid / high) after equal-loudness weighting and per-band adaptive
// gain (AGC). On every detected beat the bars change width and reshuffle their
// hues. Two layouts alternate: equal-width bars and drive-proportional bars.
// When the music stops (a song gap is detected) the display shuts off and a very
// dim, slowly drifting rainbow shimmer plays as an idle screen until sound
// returns. Most of the complexity is adaptive gain, not rendering.

// --- Sensor bindings (engine stubs them with zeros when no board is present) ---
const NB = 32
export var frequencyData = array(NB)   // 32-band magnitudes
export var energyAverage = 0           // overall sound energy
export var maxFrequency = 0            // loudest bin's frequency (unused here)
export var maxFrequencyMagnitude = 0   // magnitude of the loudest bin (unused)

// --- Band layout over the 32 bins: bass / low / mid / high ---
const BANDS = 4
var bandLo = array(BANDS)   // first bin (inclusive)
var bandHi = array(BANDS)   // last bin (exclusive)

// Equal-loudness weight table: attenuate bass and extreme treble, emphasize the
// vocal midrange, with a secondary bump in the low-treble presence region.
var wt = array(NB)

// Per-band AGC state.
var held = array(BANDS)     // fast peak-hold envelope
var thr = array(BANDS)      // moving-average "zero brightness" threshold
var slowPk = array(BANDS)   // slow all-time-recent peak
var alpha = array(BANDS)    // adaptive threshold smoothing factor
var drive = array(BANDS)    // normalized, squared display magnitude (0..1)
var hue = array(BANDS)      // per-band hue

// Global state.
var shortE = 0, longE = 0.002   // short/long energy EMAs (longE has a floor)
var gap = 1                     // idle (song-gap) mode active?
var beatCount = 0
var beatMs = 0                  // ms since last beat
var prevBass = 0                // previous frame's bass drive (beat detection)
var barWidth = 3                // random bar width for layout A
var whitePos = 0                // marching white-dot position
var huePhase = 0                // slowly cycling hue phase
var layout = 0                  // 0 = equal bars, 1 = proportional bars
var envLoud = 0                 // overall loudness envelope (white overlay gate)

// Update timers (ms accumulators).
var thrTimer = 0, alphaTimer = 0, pkTimer = 0

// --- One-time setup ---
function setup() {
  bandLo[0] = 0;  bandHi[0] = 3
  bandLo[1] = 3;  bandHi[1] = 10
  bandLo[2] = 10; bandHi[2] = 20
  bandLo[3] = 20; bandHi[3] = NB
  var i = 0
  while (i < NB) {
    // A smooth Fletcher-Munson-ish shape peaking in the midrange (~bin 12),
    // with a presence bump near bin 22, scaled up to a workable range.
    var mid = 1 - abs(i - 12) / 20
    if (mid < 0.15) mid = 0.15
    var presence = 1 - abs(i - 22) / 8
    if (presence < 0) presence = 0
    wt[i] = (mid + presence * 0.4) * 1000
    i += 1
  }
  var b = 0
  while (b < BANDS) { slowPk[b] = 0.001; alpha[b] = 0.05; b += 1 }
}
setup()

function bandMax(b) {
  var m = 0
  var i = bandLo[b]
  while (i < bandHi[b]) {
    var v = frequencyData[i] * wt[i]
    if (v > m) m = v
    i += 1
  }
  return m
}
function bandMean(b) {
  var s = 0
  var i = bandLo[b]
  while (i < bandHi[b]) { s += frequencyData[i] * wt[i]; i += 1 }
  var n = bandHi[b] - bandLo[b]
  return n > 0 ? s / n : 0
}

export function beforeRender(delta) {
  // 1. Energy excluding the bass bins, scaled up so divisions stay healthy.
  var e = 0
  var i = 3
  while (i < NB) { e += frequencyData[i]; i += 1 }
  e = e * 10

  // 2-4. Per-band data point and fast peak-hold / exponential decay.
  var decay = exp(-delta / 200)   // ~200 ms time constant
  var b = 0
  while (b < BANDS) {
    var dp = bandMax(b)
    held[b] = held[b] * decay
    if (dp > held[b]) held[b] = dp
    b += 1
  }

  // 5. Adaptive threshold smoothing factor, recomputed periodically from each
  // band's dynamic range: proportional to log(slowPeak / threshold), clamped.
  alphaTimer += delta
  if (alphaTimer > 40) {
    alphaTimer = 0
    b = 0
    while (b < BANDS) {
      var denom = thr[b] > 0.001 ? thr[b] : 0.001
      var dyn = log(slowPk[b] / denom + 1)
      alpha[b] = clamp(dyn * 0.03, 0.01, 0.3)
      b += 1
    }
  }

  // 6. Per-band moving-average thresholds on a fast timer.
  thrTimer += delta
  if (thrTimer > 30) {
    thrTimer = 0
    b = 0
    while (b < BANDS) {
      thr[b] += alpha[b] * (bandMean(b) - thr[b])
      if (thr[b] < 0) thr[b] = 0
      b += 1
    }
  }

  // 7. Slow all-time-recent peak: bleeds down on a fast timer, snaps up instantly.
  pkTimer += delta
  if (pkTimer > 30) {
    pkTimer = 0
    b = 0
    while (b < BANDS) {
      slowPk[b] *= 0.9995
      if (slowPk[b] < 0.001) slowPk[b] = 0.001
      b += 1
    }
  }
  b = 0
  while (b < BANDS) {
    if (held[b] > slowPk[b]) slowPk[b] = held[b]
    b += 1
  }

  // 8. Normalized, squared drive per band.
  b = 0
  while (b < BANDS) {
    var span = slowPk[b] - thr[b]
    var d = span > 0.001 ? (held[b] - thr[b]) / span : 0
    d = clamp(d, 0, 1)
    drive[b] = d * d
    b += 1
  }

  // 9. Song-gap detection via two energy EMAs.
  shortE += (gap ? 0.02 : 0.05) * (e - shortE)          // slower when idle
  if (e > longE) longE += 0.01 * (e - longE)            // rise faster
  else longE += 0.001 * (e - longE)                     // fall slower
  if (longE < 0.002) longE = 0.002                      // squelch floor
  if (!gap && shortE < 0.5 * longE) {
    gap = 1
    beatCount += 1
    layout = beatCount & 1
  } else if (gap && shortE > longE) {
    gap = 0
    longE *= 0.5                                        // let it re-adapt
  }

  // Overall loudness envelope for the white overlay gate.
  envLoud += 0.2 * (drive[0] + drive[1] + drive[2] + drive[3] - envLoud * 4) * 0.25

  // 10. Beat detection on the squared bass drive.
  beatMs += delta
  huePhase = time(20 / 65.536)   // slow hue cycle
  var beat = 0
  if (!gap && beatMs > 60 && drive[0] > prevBass * 3 + 0.05) beat = 1
  if (beatMs > 3000) beat = 1    // force a beat if none seen for a few seconds
  if (beat) {
    beatMs = 0
    beatCount += 1
    barWidth = 2 + floor(random(4))
    whitePos -= 1
    // Reshuffle hues: bass near red, others from the cycling phase plus fixed
    // offsets, high often blue (boosted at render in layout B).
    hue[0] = random(1) < 0.5 ? 0.0 : 0.04
    hue[1] = frac(huePhase + 0.25 + (random(1) < 0.3 ? random(1) : 0))
    hue[2] = frac(huePhase + 0.5)
    hue[3] = random(1) < 0.6 ? 0.66 : frac(huePhase + 0.75)
    if ((beatCount % 48) == 0) layout = 1 - layout
  }
  prevBass = drive[0]
}

export function render(index) {
  if (gap) {
    // Idle: very dim, slow rainbow shimmer — tiny drifting glints on black.
    var p = index / pixelCount
    var freq = 3 + wave(time(30 / 65.536)) * 4
    var h = frac(p * freq + time(45 / 65.536)) * 0.5
    var tw = triangle(p * 6 + time(12 / 65.536))
    var v = tw
    v = v * v; v = v * v; v = v * v          // ^8
    hsv(h, 1, v * 0.02)
    return
  }

  var band, bright
  if (layout == 0) {
    // Layout A: four equal-width bars, tiled by modulo.
    var group = barWidth * BANDS
    band = floor((index % group) / barWidth)
    bright = drive[band]
  } else {
    // Layout B: bar widths proportional to each band's squared drive.
    var w0 = 1 + floor(drive[0] * 7)
    var w1 = floor(drive[1] * 7)
    var w2 = 1 + floor(drive[2] * 7)
    var w3 = 1 + floor(drive[3] * 7)
    var total = w0 + w1 + w2 + w3
    if (total < 1) total = 1
    var pos = index % total
    if (pos < w0) band = 0
    else if (pos < w0 + w1) band = 1
    else if (pos < w0 + w1 + w2) band = 2
    else band = 3
    bright = drive[band]
    if (band == 3) bright = clamp(bright * 4, 0, 1)   // blue eye-sensitivity boost
  }

  var h = hue[band]
  // Sparse white-dot overlay riding the beat grid when overall loudness is high.
  if ((index % 8) == (((whitePos % 8) + 8) % 8) && envLoud > 0.3) {
    hsv(0, 0.2, 0.5)
  } else {
    hsv(h, 1, bright)
  }
}
