// name: Color Pick Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "Color Pick Fade"; original source never consulted.

// Four evenly spaced soft pulses of one user-chosen color drift along the
// strip against black. A triangle wave raised to the fourth power narrows
// each pulse into a soft bump with long dark gaps; peak brightness is
// capped at half. The speed slider scales the period (higher = slower).

const REPEATS = 4

var phase = 0
var hue = 0.06        // warm orange default
var sat = 1
var period = 10       // seconds per drift cycle

//# min=0 max=1 step=0.01 default=0.06
export function sliderHue(v) {
  hue = v
}

//# min=0 max=1 step=0.01 default=1
export function sliderSaturation(v) {
  sat = v
}

// scales the period, not the rate: ~0.3 s at 0 up to ~60 s at 1
//# min=0 max=1 step=0.01 default=0.16
export function sliderSpeed(v) {
  period = 0.3 + v * 60
}

export function beforeRender(delta) {
  phase = frac(phase + delta / 1000 / period)
}

export function render(index) {
  var t = frac(index / pixelCount * REPEATS + phase)
  var b = triangle(t)
  b = b * b
  b = b * b            // fourth power: narrow soft pulses
  hsv(hue, sat, b * 0.5)
}
