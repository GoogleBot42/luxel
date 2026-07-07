// name: Rainbow v2
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rainbow v2"; original source never consulted.
//
// The classic endlessly scrolling rainbow with a full set of tweak
// sliders: spread, speed, direction, saturation, brightness. Speed is
// mapped so higher = faster (the original scaled the period, which the
// description suggested inverting for a port).

const BASE_REV_MS = 5000   // several seconds per hue revolution at mid speed

var phase = 0
var spread = -1            // slider-top = 0 spread; lower fans out (negative
                           // spread reverses the spatial gradient direction)
var speedMul = 1
var dir = 1
var sat = 1
var bri = 1

//# min=0 max=1 step=0.01 default=0
export function sliderColorSpread(v) {
  // top end = whole strip one hue; bottom = one full rainbow across the
  // strip, gradient running in the reverse direction
  spread = v - 1
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  // 0 = ~1/4 speed, 0.5 = base, 1 = ~4x
  speedMul = pow(4, 2 * v - 1)
}

//# min=0 max=1 step=1 default=0
export function sliderDirection(v) {
  // slider used as a toggle: below halfway one way, above the other
  dir = v < 0.5 ? 1 : -1
}

//# min=0 max=1 step=0.01 default=1
export function sliderSaturation(v) {
  sat = v
}

//# min=0 max=1 step=0.01 default=1
export function sliderBrightness(v) {
  bri = v
}

export function beforeRender(delta) {
  phase = frac(phase + dir * delta * speedMul / BASE_REV_MS + 1)
}

export function render(index) {
  hsv(phase + (index / pixelCount) * spread, sat, bri)
}
