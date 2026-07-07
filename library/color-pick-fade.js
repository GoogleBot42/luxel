// name: Color Pick Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "Color Pick Fade"; original source never consulted.

// Four evenly spaced soft pulses of one user-chosen color drift along the
// strip against black. Gentle mood lighting: each pulse is a triangle wave
// raised to the fourth power (narrow bump, long dark gaps) and peak
// brightness is capped at half of maximum.

var PULSES = 4

var hue = 0.05  // warm orange default
var sat = 1
var speedV = 0.15
var t1 = 0

//# min=0 max=1 step=0.01 default=0.05
export function sliderHue(v) { hue = v }

//# min=0 max=1 step=0.01 default=1
export function sliderSaturation(v) { sat = v }

// scales the period, not the rate: higher = slower
// (sub-second frenzy up to roughly a minute per cycle)
//# min=0 max=1 step=0.01 default=0.15
export function sliderSpeed(v) { speedV = v }

export function beforeRender(delta) {
  t1 = time(0.01 + speedV * 0.9)
}

export function render(index) {
  var p = frac(index / pixelCount * PULSES + t1)
  var b = triangle(p)
  b = b * b
  hsv(hue, sat, b * b * 0.5)
}
