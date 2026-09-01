// name: Sound & Music Spectrum Visualizer
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sound & Music Spectrum Visualizer"; original source
// never consulted.

// Classic spectrum analyzer laid along the strip: ~10 segments, low to high
// frequency, with auto-sensitivity (PI controller), rate-limited spectrum
// refresh, quantized/overdriven band values and fast per-frame smoothing.
// With no sensor board attached (all-zero input) it idles dark.

// -- sensor bindings (engine fills these; zeros without a board) --
export var frequencyData = array(32)

// -- layout --
var segments = 10
var bands = array(16)                 // 32 raw bands averaged in pairs
var pix = array(pixelCount)           // smoothed per-pixel brightness
var segOf = array(pixelCount)         // pixel -> segment index
var i
for (i = 0; i < pixelCount; i++) segOf[i] = floor(i * segments / pixelCount)

// EQ regions: bass / lower-mid / upper-mid / treble breakpoints, as
// fractions of the segment count. Exported for room tuning.
// (written as exact integer ratios: `segments * 0.8` is 7.99998 in 16.16 and
// floors to 7, which silently hands band 8 to the treble boost)
export var eqBreak1 = floor(segments / 5)
export var eqBreak2 = floor(segments / 2)
export var eqBreak3 = floor(segments * 4 / 5)
export var eqBass = 0.02              // bass cut hard
export var eqLowMid = 1               // lower mids about unity
export var eqHighMid = 1.5            // upper mids boosted moderately
export var eqTreble = 2               // treble about doubled

var NOISE_GATE = 0.055                // input-side squelch: below this a band reads silent
var GATE_SETTLE = 8                   // seconds the squelch takes to close down onto it
var gate = 0                          // wide open at power-on -> the strip rails, then settles

// -- auto-sensitivity PI controller state --
var targetLit = pixelCount * 0.2      // ~a fifth of the strip fully lit
var integral = 60                     // starts railed: the strip blazes for a
var gain = 1                          // few seconds, then settles to the room
var lastTotal = 0

// -- refresh timer --
var sinceRefresh = 999                // ms; force a refresh on first frame

// -- controls --
var rainbowOn = 1
var shiftOn = 0
var fixedHue = 2 / 3                  // blue default
var presets = array(5)
presets[0] = 0        // red
presets[1] = 1 / 3    // green
presets[2] = 2 / 3    // blue (default, middle of the slider)
presets[3] = 0.75     // violet-leaning blue
presets[4] = 0.9      // pink/magenta

//# min=0 max=1 step=0.01 default=1
export function sliderRainbow(v) {
  rainbowOn = v > 0.2
}

//# min=0 max=1 step=0.01 default=0
export function sliderColorShift(v) {
  shiftOn = v > 0.2
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderColor(v) {
  fixedHue = presets[floor(min(v, 0.999) * 5)]
}

var drift

export function beforeRender(delta) {
  var dt = delta / 1000

  // 1. PI controller on last frame's total lit-ness
  var err = targetLit - lastTotal
  integral = clamp(integral + err * dt * 0.05, 0, 500)
  gain = max(0.02, err * 0.002 + integral)

  // 1b. the squelch calibrates itself: wide open at power-on (so the strip
  // blazes for the first several seconds while the loop finds the room),
  // easing shut onto the noise floor over GATE_SETTLE
  gate = min(NOISE_GATE, gate + NOISE_GATE * dt / GATE_SETTLE)

  // 2. spectrum refresh, several times a second (not every frame)
  sinceRefresh += delta
  if (sinceRefresh >= 200) {
    sinceRefresh = 0
    var b
    for (b = 0; b < 16; b++) {
      // average adjacent raw bands
      var v = (frequencyData[b * 2] + frequencyData[b * 2 + 1]) / 2
      // fixed EQ by region
      if (b < eqBreak1) v *= eqBass
      else if (b < eqBreak2) v *= eqLowMid
      else if (b < eqBreak3) v *= eqHighMid
      else v *= eqTreble
      // Squelch on the RAW level, ahead of the gain: the board reports a hair
      // of room tone in every band forever, and gating downstream of the
      // sensitivity loop only lets the loop amplify that floor until it glows
      // (the controller then parks at a gain where half the strip is lit by
      // noise instead of by music). Gating first makes the lit set
      // gain-independent — only bands carrying real signal ever light, and
      // the loop is left with a single equilibrium.
      if (v < gate) v = 0
      // then the sensitivity gain and a half-step ladder that overshoots full
      // brightness, so loud hits slam and stay saturated while smoothing decays
      else v = min(ceil(v * gain * 2) / 2, 3)
      bands[b] = v
    }
  }

  // 3. per-pixel exponential smoothing toward the segment's band value,
  //    every frame; the sum feeds back into the controller
  var total = 0
  var p
  for (p = 0; p < pixelCount; p++) {
    pix[p] = (pix[p] * 4 + bands[segOf[p]]) / 5
    // feedback is the UNCLAMPED sum: the ladder's overshoot above full
    // brightness counts, so the controller stops pushing gain once the loud
    // bands are saturated instead of amplifying until the noise floor lights
    total += pix[p]
  }
  lastTotal = total

  // slow back-and-forth hue drift, period of tens of seconds
  drift = triangle(time(0.5))
}

export function render(index) {
  var v = clamp(pix[index], 0, 1)
  var h
  if (rainbowOn) {
    // triangle of position spanning a bit more than half the wheel,
    // scrolled gently back and forth by the drift
    h = triangle(index / pixelCount) * 0.6 + drift
  } else if (shiftOn) {
    // near-monochrome slow recolor with a slight positional ripple
    h = drift + index / pixelCount * 0.03
  } else {
    h = fixedHue
  }
  hsv(h, 1, v)
}
