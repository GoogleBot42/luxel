// name: spotlights / rotation 3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "spotlights / rotation 3D"; original source never consulted.

// A white-cored double cone tumbles through the unit cube. The rotation axis
// wanders on three triangle-wave oscillators; hue is painted onto the world
// from the unrotated x coordinate so colors stay put while the beam sweeps.

var width = 3 / PISQ      // cone width divisor (slider-quantized)
var speedMul = 1          // oscillator speed multiplier (slider-quantized)

// rotation matrix, rebuilt once per frame
var m00 = 1, m01 = 0, m02 = 0
var m10 = 0, m11 = 1, m12 = 0
var m20 = 0, m21 = 0, m22 = 1

//# min=0 max=1 step=0.2 default=0.4
export function sliderScale(v) {
  // ~six discrete steps; step / ~10 (pi squared) is the width divisor
  width = (1 + floor(clamp(v, 0, 1) * 5.99)) / PISQ
}

//# min=0 max=1 step=0.5 default=0.5
export function sliderSpeed(v) {
  // three coarse steps: gentle sweep .. fast tumble
  speedMul = pow(2, floor(clamp(v, 0, 1) * 2.99) - 1)   // 0.5, 1, 2
}

export function beforeRender(delta) {
  // wandering rotation axis: three triangle waves at close-but-different
  // periods, rescaled to -1..1 (sines were "almost too smooth")
  var ax = triangle(time(0.035 / speedMul)) * 2 - 1
  var ay = triangle(time(0.041 / speedMul)) * 2 - 1
  var az = triangle(time(0.047 / speedMul)) * 2 - 1

  // rotation angle: plain sawtooth, somewhat shorter period, full turn/cycle
  var angle = time(0.025 / speedMul) * PI2

  var len = hypot3(ax, ay, az)
  if (len < 0.001) { ax = 0; ay = 1; az = 0; len = 1 }
  var ux = ax / len, uy = ay / len, uz = az / len

  // classic Rodrigues axis-angle rotation matrix
  var c = cos(angle), s = sin(angle), t = 1 - c
  m00 = t * ux * ux + c
  m01 = t * ux * uy - s * uz
  m02 = t * ux * uz + s * uy
  m10 = t * ux * uy + s * uz
  m11 = t * uy * uy + c
  m12 = t * uy * uz - s * ux
  m20 = t * ux * uz - s * uy
  m21 = t * uy * uz + s * ux
  m22 = t * uz * uz + c
}

export function render3D(index, x, y, z) {
  // center the volume
  x -= 0.5
  y -= 0.5
  z -= 0.5

  // rotate the point into cone space
  var rx = m00 * x + m01 * y + m02 * z
  var ry = m10 * x + m11 * y + m12 * z
  var rz = m20 * x + m21 * y + m22 * z

  // signed double-cone field: + inside, - outside; clamp keeps the
  // brightness curve from exploding
  var f = clamp(abs(ry) - hypot(rx / width, rz / width), -1, 1)

  // hue from the UNrotated centered x: painted onto the world (the small
  // negative side wraps to magenta-ish tones near center)
  hsv(x, clamp(1 - f, 0, 1), pow((1 + f) / 2, 4))
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)   // planar slice of the tumbling cone
}

export function render(index) {
  render3D(index, index / pixelCount * 2, 0, 0)   // strip stretched across x
}
