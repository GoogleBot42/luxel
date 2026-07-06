// name: 2D Spiral Twirls
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D Spiral Twirls"; original source never consulted.

// A rotating pinwheel whose arms periodically wind into a spiral, relax
// straight, then wind the opposite way. Each arm is a half-rainbow ramp
// with a dark comet seam; the palette drifts around the hue wheel.
// Sliders at zero deliberately freeze their motion (unlike the buggy
// zero-guard described of the original).

var twistRate = 0.04       // cycles per second
//# min=0 max=1 step=0.01 default=0.2
export function sliderTwistSpeed(v) {
  // inverse-period feel: fully right ~1 s per oscillation; zero = frozen
  twistRate = v * v
}

var rotRate = 0.04
//# min=0 max=1 step=0.01 default=0.2
export function sliderRotationSpeed(v) {
  rotRate = v * v
}

var baseColor = 0
//# min=0 max=1 step=0.01 default=0
export function sliderInitialColor(v) {
  baseColor = v
}

var colRate = 0.01
//# min=0 max=1 step=0.01 default=0.1
export function sliderColorSpeed(v) {
  colRate = v * v
}

var arms = 2
//# min=0 max=1 step=0.5 default=0.5
export function sliderArms(v) {
  arms = 1 + floor(v * 2.999)   // snapped integer, 1..3
}

var twistPhase = 0
var rotPhase = 0
var colPhase = 0
var twist = 0

export function beforeRender(delta) {
  var dt = delta / 1000
  // rate of zero freezes that motion in place
  twistPhase = mod(twistPhase + dt * twistRate, 1)
  rotPhase = mod(rotPhase + dt * rotRate, 1)
  colPhase = mod(colPhase + dt * colRate, 1)
  // sinusoidal twist remapped to -1..+1; the sign flip alternates handedness
  twist = wave(twistPhase) * 2 - 1
}

export function render2D(index, x, y) {
  // recenter and scale so the display half-width is one
  var px = (x - 0.5) * 2
  var py = (y - 0.5) * 2
  var r = hypot(px, py)
  var a = atan2(py, px) / PI2 + 0.5   // polar angle normalized to 0..1

  // radius-proportional angular offset bends straight arms into spirals
  a += r * twist * 0.5

  // arm-local fraction: serves as hue ramp and brightness profile
  var f = frac(a * arms - rotPhase + 8)

  // comet profile: dark seam, cubed slow rise, linear bright half,
  // hard bright-to-dark edge at the wrap; linear radial falloff on top
  var shape = f < 0.5 ? f * f * f : f
  var v = max(0, 1.05 - r) * shape

  // each arm spans half the rainbow; whole palette drifts over time
  hsv((f + baseColor) * 0.5 + colPhase, 1, v)
}
