// name: 2D Spiral Twirls
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D Spiral Twirls"; original source never consulted.

// A rotating pinwheel whose arms periodically wind into a spiral, relax
// straight, then wind the opposite way. Each arm is a half-rainbow ramp
// with a dark comet seam; the palette drifts around the hue wheel.
// Sliders at zero deliberately freeze their motion (unlike the buggy
// zero-guard described of the original).

// All the rate sliders below carry //# bounds, so the UI hands the handler
// REAL units — they are used as-is, never rescaled from 0..1.

// Wind/unwind oscillations per second; 0 freezes the twist where it stands.
var twistRate = 0.04
//# min=0 max=1 step=0.01 default=0.04
export function sliderTwistSpeed(v) {
  twistRate = max(0, v)
}

// Pinwheel rotation in arm-periods per second: the pattern repeats every
// 1/arms of a turn, so at the top of the slider a 2-arm wheel spins twice
// per second. 0 freezes it.
var rotRate = 0.04
//# min=0 max=4 step=0.01 default=0.04
export function sliderRotationSpeed(v) {
  rotRate = max(0, v)
}

// Hue offset of the whole palette, in turns around the colour wheel.
var baseColor = 0
//# min=0 max=1 step=0.01 default=0
export function sliderInitialColor(v) {
  baseColor = v
}

// Palette drift in full colour cycles per second.
var colRate = 0.01
//# min=0 max=1 step=0.01 default=0.01
export function sliderColorSpeed(v) {
  colRate = max(0, v)
}

// Whole arms, in arms.
var arms = 2
//# min=1 max=8 step=1 default=2
export function sliderArms(v) {
  arms = clamp(floor(v), 1, 8)
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
