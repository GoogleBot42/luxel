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
var TARGET_FILL = 0.2      // aim: ~1/5 of the strip lit
var KP = 0.4               // proportional gain
var KI = 0.06              // integral gain (a bit smaller than KP)
var INTEGRAL_MAX = 300
var AVG_WINDOW = 1500      // ms — rolling-average time window
var AVG_EPS = 0.0002       // rolling averages never reach zero
var SCROLL_INTERVAL = 0.035 // time() interval: fold slides over ~2.3 s
var ATTACK_GAIN = 4        // live energy amplification before subtracting avg
var DECAY = 0.75           // trail persistence per frame
var FEEDBACK_CAP = 1       // per-pixel clamp feeding the controller

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
