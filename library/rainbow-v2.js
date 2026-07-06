// name: Rainbow v2
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rainbow v2"; original source never consulted.

// The classic endlessly scrolling rainbow, with sliders for everything:
// how much of the rainbow is fanned across the strip, scroll speed and
// direction, saturation, and overall brightness. At zero spread the whole
// strip pulses as one uniform hue cycling around the wheel.

var spread = 1       // 0 = uniform strip, 1 = one full rainbow across it
var speed = 0.75     // higher = faster (spec's period quirk inverted)
var dirSign = 1      // +1 / -1 scroll direction
var sat = 1
var bright = 1
var phase = 0

//# min=0 max=1 step=0.01 default=1
export function sliderColorSpread(v) { spread = v }

//# min=0 max=1 step=0.01 default=0.75
export function sliderSpeed(v) { speed = v }

//# min=0 max=1 step=1 default=0
export function sliderDirection(v) { dirSign = v < 0.5 ? 1 : -1 }

//# min=0 max=1 step=0.01 default=1
export function sliderSaturation(v) { sat = v }

//# min=0 max=1 step=0.01 default=1
export function sliderBrightness(v) { bright = v }

export function beforeRender(delta) {
  // Sawtooth phase; period runs ~20 s (slow) down to ~1 s (fast).
  phase = time(mix(0.3, 0.015, speed)) * dirSign
}

export function render(index) {
  // hue wraps naturally around the color wheel
  hsv(phase + index / pixelCount * spread, sat, bright)
}
