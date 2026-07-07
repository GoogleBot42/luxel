// name: spotlights / rotation 3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "spotlights / rotation 3D"; original source never consulted.

// A white-hot double cone tumbles through the unit cube on a wandering axis.
// Hue is painted onto the world by unrotated x; the cone surface is vivid,
// its core desaturates to white, and brightness falls off steeply outside.

var width = 0.304    // cone width divisor (slider-quantized, ~step/pi^2)
var spd = 1          // speed multiplier (quantized 1..3)

// rotation matrix, rebuilt once per frame
var m00 = 1, m01 = 0, m02 = 0
var m10 = 0, m11 = 1, m12 = 0
var m20 = 0, m21 = 0, m22 = 1

//# min=0 max=1 step=0.01 default=0.5
export function sliderScale(v) {
  // ~6 discrete widths; divide the step by roughly pi squared
  width = (1 + floor(v * 5.99)) / 9.87
}

//# min=0 max=1 step=0.5 default=0
export function sliderSpeed(v) {
  spd = 1 + floor(v * 2.99)        // 3 coarse steps
}

export function beforeRender(delta) {
  // rotation axis from three triangle waves at close-but-different periods
  // (triangle instead of sine on purpose: not "too smooth")
  var ax = triangle(time(0.037 / spd)) * 2 - 1
  var ay = triangle(time(0.043 / spd)) * 2 - 1
  var az = triangle(time(0.049 / spd)) * 2 - 1
  var angle = time(0.025 / spd) * PI2   // sawtooth: full turn per cycle

  var len = sqrt(ax * ax + ay * ay + az * az)
  if (len < 0.001) { ax = 0; ay = 1; az = 0; len = 1 }
  var ux = ax / len, uy = ay / len, uz = az / len

  // axis-angle (Rodrigues) rotation matrix
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
  x -= 0.5; y -= 0.5; z -= 0.5     // center the volume
  var rx = m00 * x + m01 * y + m02 * z
  var ry = m10 * x + m11 * y + m12 * z
  var rz = m20 * x + m21 * y + m22 * z

  // signed double-cone field: + inside, - outside; clamp or the curve explodes
  var f = clamp(abs(ry) - sqrt((rx / width) * (rx / width) + (rz / width) * (rz / width)), -1, 1)

  var b = (1 + f) * 0.5            // ((1+f)/2)^4: steep, soft-edged falloff
  b = b * b
  // hue from the UNrotated x so color stays fixed to the installation
  hsv(x, saturate(1 - f), b * b)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)         // planar slice of the tumbling cone
}

export function render(index) {
  render3D(index, index / pixelCount * 2, 0, 0)
}
