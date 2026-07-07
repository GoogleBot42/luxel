// name: Golden Tix
// Clean-room reimplementation from a prose functional description of the
// community pattern "Golden Tix"; original source never consulted.

// A minimal 1D live-coding sandbox in the tixy.land style: edit the one-line
// formula below. It receives (t, i, p) — scaled time, raw pixel index, and
// normalized position — and returns a signed value in roughly -1..+1.
// Positive values light in one picker-chosen hue, negative in another;
// brightness is the squared magnitude on black.

// ---- the formula: edit me! -------------------------------------------------
function formula(t, i, p) {
  return sin(i * t)   // default: accelerating moiré stripes
}
// ----------------------------------------------------------------------------

var posHue = 0        // positive values: red (top of the hue wheel)
var negHue = 0.5      // negative values: cyan-ish (middle of the wheel)
var elapsed = 0       // seconds since start / last reset
var divisor = 5       // time divisor: speed * 100, clamped positive
var tScaled = 0

export function hsvPickerPositiveColor(h, s, v) { posHue = h }
export function hsvPickerNegativeColor(h, s, v) { negHue = h }

//# min=0 max=1 step=0.01 default=0.05
export function sliderSpeedShift(v) {
  divisor = max(v * 100, 0.5)   // higher = slower; clamp away the /0 wart
}

//# min=0 max=1 step=0.01 default=0
export function sliderSlideRightToReset(v) {
  if (v >= 0.99) elapsed = 0    // one-shot: re-arms as soon as slider leaves max
}

export function beforeRender(delta) {
  elapsed += delta / 1000
  tScaled = elapsed / divisor
}

export function render(index) {
  var v = formula(tScaled, index, index / pixelCount)
  if (v >= 0) {
    hsv(posHue, 1, v * v)
  } else {
    hsv(negHue, 1, v * v)
  }
}
