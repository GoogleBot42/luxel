// name: sound - spectro kalidastrip
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - spectro kalidastrip"; original source never
// consulted.

// Music-reactive spectrum with a sliding kaleidoscope fold. Per-band
// transients (live energy above a rolling average) flare rainbow regions
// that decay as short trails; a PI auto-gain controller keeps roughly a
// fifth of the strip lit regardless of room volume. All-zero audio input
// simply renders dark.

export var frequencyData = array(32)   // 32-band spectrum, low bands first
export var energyAverage
export var maxFrequency
export var maxFrequencyMagnitude

var bands = arrayLength(frequencyData)

// --- tuning ---------------------------------------------------------------
// The four values marked (control) below are driven by the sliders further
// down; their top-level values are the untouched defaults.
var TARGET_FILL = 0.2      // (control) aim: ~1/5 of the strip lit
var KP = 0.4               // proportional gain
var KI = 0.06              // integral gain (a bit smaller than KP)
var INTEGRAL_MAX = 300
var AVG_WINDOW = 1500      // ms — rolling-average time window
var AVG_EPS = 0.0002       // rolling averages never reach zero
var SCROLL_INTERVAL = 0.035 // (control) fold slides over ~2.3 s
var ATTACK_GAIN = 4        // (control) live energy amplification
var DECAY = 0.75           // (control) trail persistence per frame
var FEEDBACK_CAP = 1       // per-pixel clamp feeding the controller

// --- controls -------------------------------------------------------------
// How much of the strip the auto-gain controller tries to keep lit; the
// loudness the room happens to be at is compensated for either way.
//# min=5 max=80 step=5 default=20
export function sliderTargetFill(v) { TARGET_FILL = clamp(v, 1, 95) / 100 }

// Share of a spark's brightness carried into the next frame: 0 = no trail,
// high values smear transients into long comet tails.
//# min=0 max=95 step=5 default=75
export function sliderTrailPersistence(v) { DECAY = clamp(v, 0, 95) / 100 }

// Seconds for the kaleidoscope fold to slide once through the spectrum.
//# min=0.5 max=30 step=0.1 default=2.3
export function sliderFoldSeconds(v) { SCROLL_INTERVAL = max(0.25, v) / 65.536 }

// How hard a band's live energy is driven against its rolling average
// before the transient is taken; higher = snappier, punchier flares.
//# min=1 max=12 step=0.5 default=4
export function sliderAttackGain(v) { ATTACK_GAIN = max(0.25, v) }

// --- state ----------------------------------------------------------------
var avg = array(bands)       // per-band rolling average (sensitivity-scaled)
var pix = array(pixelCount)  // per-pixel persistence buffer (trails)
var integral = 40            // PI integral term, starts moderately positive
var sensitivity = 1
var accum = 0                // brightness fed back from last frame's render
var scroll = 0

export function beforeRender(delta) {
  // 1. Auto-gain: PI controller on last frame's lit fraction.
  var err = TARGET_FILL - accum / pixelCount
  integral = clamp(integral + err, 0, INTEGRAL_MAX)
  sensitivity = max(0, KP * err + KI * integral)
  accum = 0

  // 2. Slide the kaleidoscope fold back and forth.
  scroll = time(SCROLL_INTERVAL)

  // 3. Update each band's rolling average toward the scaled live energy.
  var k = min(1, delta / AVG_WINDOW)
  for (var b = 0; b < bands; b++) {
    avg[b] = max(AVG_EPS, avg[b] + (frequencyData[b] * sensitivity - avg[b]) * k)
  }
}

export function render(index) {
  var pos = index / pixelCount

  // Nested triangle fold: mirrors the spectrum around moving fold points.
  var f = triangle(triangle(pos * 2) + scroll) * (bands - 1)

  // Linear interpolation between adjacent bands, live and averaged.
  var b0 = floor(f)
  var b1 = min(bands - 1, b0 + 1)
  var ft = f - b0
  var live = frequencyData[b0] + (frequencyData[b1] - frequencyData[b0]) * ft
  var norm = avg[b0] + (avg[b1] - avg[b0]) * ft

  // Transient above the recent norm, emphasized where the band is
  // generally active, squared for contrast.
  var v = (live * ATTACK_GAIN * sensitivity - norm) * (0.4 + norm * 8)
  v = max(0, v)
  v = v * v

  // Fast attack, slow decay: blend into the persistence buffer.
  v = pix[index] = pix[index] * DECAY + v

  // Feedback for the auto-gain controller.
  accum += min(v, FEEDBACK_CAP)

  // Rainbow keyed to frequency, plus a mild gradient along the strip;
  // overdriven peaks desaturate toward white.
  var h = f / (bands - 1) + pos * 0.25
  hsv(h, clamp(2 - v, 0, 1), min(v, 1))
}
