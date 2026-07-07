// name: spiral twirls star 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "spiral twirls star 2D"; original source never consulted.
// (Hexagram SDF is the standard fold-and-measure construction.)

// slider-set parameters with sensible defaults
var starSize = 0.5
var edgeWidth = 0.3
var twistSpeed = 0.4
var rotSpeed = 0.4
var baseColor = 0
var colorSpeed = 0.3
var armCount = 2
var starSpin = 0.15      // signed rad/s for the star mask

//# min=0 max=1 step=0.01 default=0.5
export function sliderStarSize(v) { starSize = v }
//# min=0 max=1 step=0.01 default=0.3
export function sliderLineWidth(v) { edgeWidth = v }
//# min=0 max=1 step=0.01 default=0.4
export function sliderTwistSpeed(v) { twistSpeed = v }
//# min=0 max=1 step=0.01 default=0.4
export function sliderRotationSpeed(v) { rotSpeed = v }
//# min=0 max=1 step=0.01 default=0
export function sliderInitialColor(v) { baseColor = v }
//# min=0 max=1 step=0.01 default=0.3
export function sliderColorSpeed(v) { colorSpeed = v }
//# min=0 max=1 step=0.01 default=0.5
export function sliderArms(v) { armCount = floor(v * 2.999) + 1 }
//# min=0 max=1 step=0.01 default=0.55
export function sliderStarRotation(v) {
  // bidirectional, roughly logarithmic: center = stationary
  var s = v - 0.5
  var mag = (pow(24, abs(s) * 2) - 1) * 0.06
  starSpin = s < 0 ? -mag : mag
}

// free-running phase accumulators (slider zero = truly stopped)
var twistPh = 0.25
var rotPh = 0
var colPh = 0
var starAngle = 0
var twist = 0

export function beforeRender(delta) {
  var dt = delta / 1000
  twistPh += dt * twistSpeed * 0.03    // wind/unwind, tens of seconds
  rotPh += dt * rotSpeed * 0.3         // arm sweep, seconds per rev
  colPh += dt * colorSpeed * 0.02      // slow hue drift
  twistPh -= floor(twistPh)
  rotPh -= floor(rotPh)
  colPh -= floor(colPh)
  twist = sin(twistPh * PI2) * 3       // swings -3..+3 turns of shear

  starAngle += dt * starSpin           // phase accumulates: speed changes never jump

  resetTransform()
  translate(-0.5, -0.5)                // origin to the middle of the map
  scale(2, 2)                          // coords span roughly -1..1
  rotate(starAngle)                    // spin the whole scene (star mask)
}

// signed distance to a hexagram (six-pointed star) of size r, centered at 0
function starSD(px, py, r) {
  var kx = -0.5
  var ky = 0.8660254
  var kz = 0.5773503
  var kw = 1.7320508
  px = abs(px)
  py = abs(py)
  var d = min(kx * px + ky * py, 0)
  px -= 2 * d * kx
  py -= 2 * d * ky
  d = min(ky * px + kx * py, 0)
  px -= 2 * d * ky
  py -= 2 * d * kx
  px -= clamp(px, r * kz, r * kw)
  py -= r
  var len = hypot(px, py)
  return py < 0 ? -len : len
}

function scene(x, y) {
  var starR = 0.05 + starSize * 0.55
  var edge = edgeWidth * edgeWidth * 0.3   // squared: most travel is subtle
  if (starSD(x, y, starR) > edge) {
    rgb(0, 0, 0)                           // outside the star: black
    return
  }

  var r = hypot(x, y)
  var a = atan2(y, x) / PI2                // normalized angle
  a += r * twist / 2                       // radial shear = spiral wind

  var arm = a * armCount - rotPh + 8
  arm -= floor(arm)

  // lower half cubed (soft dark wedge), upper half linear; the wrap
  // discontinuity makes the crisp leading edge
  var shape = arm < 0.5 ? arm * arm * arm : arm
  var v = max(0, (1.05 - r) * shape)

  hsv(arm * 0.5 + baseColor + colPh, 1, v)
}

export function render2D(index, x, y) {
  scene(x, y)
}

// 3D: fixed isometric projection onto the 2D scene
export function render3D(index, x, y, z) {
  var px = (x - z) * 0.7
  var py = (x + z) * 0.35 + y * 0.6 - 0.65
  scene(px, py)
}

// 1D fallback: the horizontal slice through mid-height
export function render(index) {
  var px = (index / pixelCount - 0.5) * 2
  var c = cos(starAngle)
  var s = sin(starAngle)
  scene(px * c, px * s)
}
