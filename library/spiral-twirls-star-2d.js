// name: spiral twirls star 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "spiral twirls star 2D"; original source never
// consulted.

// A six-pointed star (hexagram outline SDF) filled with rotating rainbow
// spiral arms; black outside. The arms' twist winds up, unwinds and
// rewinds the other way in a slow breathing oscillation while the whole
// star mask spins at an adjustable bidirectional speed. Phases accumulate
// from the frame delta, so changing a speed slider never makes the motion
// jump (the intended behavior of the original's phase-matching timer),
// and a slider at zero really does freeze its motion (the original's
// zero-guard was buggy; the intended behavior is implemented here).

var starSize = 0.55
var edge = 0.02
var twistSpeed = 0.5
var rotSpeed = 0.5
var baseHue = 0
var colorSpeed = 0.3
var arms = 2
var starRotCtl = 0.5

export function sliderStarSize(v) {
  //# min=0 max=1 step=0.01 default=0.5
  starSize = 0.1 + v * 0.9
}
export function sliderLineWidth(v) {
  //# min=0 max=1 step=0.01 default=0.25
  edge = v * v * 0.3          // squared: most of the travel is subtle
}
export function sliderTwistSpeed(v) {
  //# min=0 max=1 step=0.01 default=0.5
  twistSpeed = v              // 0 = frozen twist
}
export function sliderRotationSpeed(v) {
  //# min=0 max=1 step=0.01 default=0.5
  rotSpeed = v                // 0 = arms stop sweeping
}
export function sliderInitialColor(v) {
  //# min=0 max=1 step=0.01 default=0
  baseHue = v
}
export function sliderColorSpeed(v) {
  //# min=0 max=1 step=0.01 default=0.3
  colorSpeed = v              // 0 = color frozen at InitialColor
}
export function sliderArms(v) {
  //# min=0 max=1 step=0.5 default=0.5
  arms = 1 + floor(v * 2.999) // snaps to 1, 2 or 3
}
export function sliderStarRotation(v) {
  //# min=0 max=1 step=0.01 default=0.5
  starRotCtl = v              // center = still; ends = fast, opposite spins
}

var twistPhase = 0
var rotPhase = 0
var huePhase = 0
var starAngle = 0
var cosA = 1
var sinA = 0
var twist = 0

export function beforeRender(delta) {
  var dt = delta / 1000
  // Accumulated phases: slider speed scales the rate; zero stops it dead
  twistPhase = frac(twistPhase + dt * twistSpeed / 20)  // ~20 s breathing
  rotPhase = frac(rotPhase + dt * rotSpeed / 3)         // ~3 s per rev
  huePhase = frac(huePhase + dt * colorSpeed / 15)      // ~15 s hue drift

  // Star spin: bidirectional, roughly logarithmic response around center
  var sr = starRotCtl - 0.5
  var spin = sign(sr) * (pow(2, abs(sr) * 8) - 1) * 0.04
  starAngle = frac(starAngle + dt * spin)
  cosA = cos(starAngle * PI2)
  sinA = sin(starAngle * PI2)

  // Twist swings -1..1; scaled to a few turns of shear at full radius
  twist = sin(twistPhase * PI2) * 3
}

// Signed distance to a hexagram (Star of David outline) of size r,
// centered at the origin: fold the plane across the star's mirror
// symmetries with hexagonal-lattice direction vectors, then measure
// distance to a single edge segment.
function sdHexagram(px, py, r) {
  var kx = -0.5
  var ky = 0.8660254
  px = abs(px)
  py = abs(py)
  var d = 2 * min(dot(kx, ky, px, py), 0)
  px -= d * kx
  py -= d * ky
  d = 2 * min(dot(ky, kx, px, py), 0)
  px -= d * ky
  py -= d * kx
  px -= clamp(px, r * 0.5773503, r * 1.7320508)
  py -= r
  return hypot(px, py) * sign(py)
}

export function render2D(index, x, y) {
  // Recenter to -1..1 and spin the whole scene by the star angle
  var px = (x - 0.5) * 2
  var py = (y - 0.5) * 2
  var rx = px * cosA - py * sinA
  var ry = px * sinA + py * cosA

  // Star mask, fattened outward by the edge threshold
  if (sdHexagram(rx, ry, starSize) > edge) {
    rgb(0, 0, 0)
    return
  }

  // Polar coordinates; shear the angle with radius for the spiral wind
  var r = hypot(rx, ry)
  var ang = atan2(ry, rx) / PI2 + 0.5 + r * twist * 0.5

  // Arm coordinate: fractional position within one arm's sweep
  var arm = ang * arms - rotPhase
  arm -= floor(arm)

  // Lower half cubed = soft dark wedge; upper half linear; the wrap
  // discontinuity gives each arm its crisp leading edge
  var shape = arm < 0.5 ? arm * arm * arm * 4 : arm
  var v = max(0, 1.1 - r) * shape

  // About half the wheel on screen at once, drifting through all hues
  hsv(arm * 0.5 + baseHue + huePhase, 1, v)
}

// 3D: fixed isometric projection onto the 2D scene
export function render3D(index, x, y, z) {
  var u = 0.5 + ((x - 0.5) - (y - 0.5)) * 0.6
  var v = 0.5 + ((x - 0.5) + (y - 0.5)) * 0.3 - (z - 0.5) * 0.6
  render2D(index, u, v)
}

// 1D fallback: the horizontal slice through mid-height
export function render(index) {
  render2D(index, index / max(1, pixelCount - 1), 0.5)
}
