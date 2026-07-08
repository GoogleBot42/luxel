// name: sound - rays Frequency-BPM Reactive 1
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - rays Frequency-BPM Reactive 1"; original source
// never consulted.

// "Rays" of color are born at one end of the strip and stream toward the other,
// leaving a scrolling history of the music. Each ray's hue encodes the dominant
// pitch when it was born; its brightness encodes how loud that pitch was (double-
// squared, so vivid rays on black). The novel twist: ray travel speed is
// proportional to the detected musical tempo (BPM), so faster songs stream
// faster. With no sensor board attached the pattern synthesizes gently varying
// rainbow rays. Two subsystems share the frame loop: the ray renderer (circular
// history buffers + PI auto-gain) and the beat/tempo detector.

// --- Sensor bindings (engine stubs them with zeros when no board is present) ---
const NB = 32
export var frequencyData = array(NB)
export var maxFrequency = 0            // dominant frequency (Hz)
export var maxFrequencyMagnitude = 0   // its magnitude
export var light = 0                   // presence probe (see silence detect)

const HUE_FULLSCALE = 4000   // mid-kHz full-scale for pitch -> one hue trip

// --- Ray renderer state ---
var hues = array(pixelCount)   // per-pixel hue history (circular)
var vals = array(pixelCount)   // per-pixel brightness history (circular)
var head = 0                   // fractional write-head position
var lastVal = 0                // last brightness written (AGC feedback)
var integral = 0               // PI integral accumulator
var speed = 0.02               // pixels per ms (fallback default)

// --- Tempo detector state ---
var slowBass = 0, fastBass = 0
var recentMax = 0.001
var deriv = array(16)          // ring buffer of normalized fast-avg derivative
var derivLen = 4               // effective window (from beat-sensitivity slider)
var derivHead = 0
var derivMean = 0
var debounce = 0               // ms countdown
var intervals = array(8)       // last several inter-beat intervals (ms)
var intHead = 0, intCount = 0
var sinceBeat = 0              // ms since last beat
var prevFast = 0
var bpm = 0

// --- UI controls ---
var beatSens = 0.4
var bpmFactor = 0.5
export function sliderBeatSensitivity(v) { //# min=0 max=1 step=0.01 default=0.4
  beatSens = v
  // Window length is a quadratic function of the slider (fixed vs. the original,
  // which sized it once before the default was assigned and never resized).
  derivLen = 2 + floor(v * v * 13)   // 2..15
}
export function showNumberBeatSensitivity() { return beatSens }
export function sliderBpmSpeedFactor(v) { //# min=0 max=1 step=0.01 default=0.5
  bpmFactor = 0.1 + v * 1.9
}
export function showNumberBpmSpeedFactor() { return bpmFactor }

var simClock = 0

export function beforeRender(delta) {
  // Silence / presence probe: the engine stubs sensors to zero, so if there is
  // no dominant magnitude and the spectrum is empty, synthesize sound.
  var specSum = arraySum(frequencyData)
  var silent = (maxFrequencyMagnitude <= 0) && (specSum <= 0)

  var domFreq = maxFrequency
  var domMag = maxFrequencyMagnitude

  if (silent) {
    // C. Simulation fallback: slow pitch sweep + irregular beat-like swells.
    simClock += delta / 1000
    domFreq = 200 + wave(simClock * 0.02) * 3200 * (0.85 + random(0.3))
    var swell = sin(simClock * 2.0) * sin(simClock * 3.0) * sin(simClock * 5.0)
    domMag = log(1 + max(0, swell) * 8 + 0.2) * (0.85 + random(0.3))
  }

  // --- B. Beat / tempo detection from the low ("bass") bands, skipping the very
  // lowest bin. In silence bass is ~0, so the fallback speed carries the rays. ---
  var bass = 0
  var i = 1
  while (i < 6) { bass += frequencyData[i]; i += 1 }
  slowBass += 0.001 * (bass - slowBass)
  fastBass += 0.1 * (bass - fastBass)
  if (bass > slowBass && bass > 0.02) recentMax -= recentMax * 0.02
  if (fastBass > recentMax) recentMax = fastBass
  if (recentMax < 0.001) recentMax = 0.001

  var d = (fastBass - prevFast) / recentMax
  prevFast = fastBass
  deriv[derivHead] = d
  derivHead = (derivHead + 1) % 16
  // Running mean over the effective window.
  var s = 0
  var k = 0
  while (k < derivLen) {
    var idx = ((derivHead - 1 - k) % 16 + 16) % 16
    s += deriv[idx]
    k += 1
  }
  derivMean = s / derivLen

  debounce -= delta
  sinceBeat += delta
  if (derivMean > 0.5 && debounce <= 0) {
    // Confirmed beat.
    if (intCount > 0 || sinceBeat < 4000) {
      intervals[intHead] = sinceBeat
      intHead = (intHead + 1) % 8
      if (intCount < 8) intCount += 1
    }
    sinceBeat = 0
    // Mean interval -> BPM.
    var sumI = 0
    var j = 0
    while (j < intCount) { sumI += intervals[j]; j += 1 }
    var meanI = intCount > 0 ? sumI / intCount : 0
    if (meanI > 0) bpm = 60000 / meanI
    // Debounce = ~1/5 of a quarter note at the current tempo estimate.
    var quarterMs = bpm > 0 ? 60000 / bpm : 500
    debounce = quarterMs * 0.2
    // Ray speed proportional to estimated BPM * user factor.
    if (bpm > 0) speed = (bpm / 6000) * bpmFactor
  }

  // --- A. Ray renderer: advance head, auto-gain, write ---
  head += delta * speed
  head = ((head % pixelCount) + pixelCount) % pixelCount

  // PI auto-gain chasing "recent written brightness ~ mid-scale".
  var err = 0.5 - lastVal
  integral = clamp(integral + err * 0.01, 0, 50)
  var gain = 0.5 + integral

  var raw = domMag * gain
  var bright = clamp(raw * raw, 0, 1)      // squared at write
  lastVal = bright
  var h = frac(domFreq / HUE_FULLSCALE)

  var wi = floor(head)
  hues[wi] = h
  vals[wi] = bright
}

export function render(index) {
  // Reverse index so motion flows in the desired direction, offset by the head.
  var rev = pixelCount - 1 - index
  var src = floor(((rev + head) % pixelCount + pixelCount) % pixelCount)
  var v = vals[src]
  hsv(hues[src], 1, clamp(v * v, 0, 1))    // squared again as gamma
}
