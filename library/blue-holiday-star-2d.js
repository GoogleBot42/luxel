// name: Blue Holiday Star 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Blue Holiday Star 2D"; original source never consulted.

// An eight-pointed star (two overlapped four-pointed stars, one axis-aligned
// and one turned an eighth of a turn) glows at the center: icy cyan-blue with
// a near-white core and long saturated rays fading into black. Perlin-driven
// twinkle swells the first star's size and the second star's pointiness, and
// the whole figure rotates imperceptibly slowly.
//
// The star shape comes from a Minkowski distance: with exponent p < 1 the
// iso-contours of (|x|^p + |y|^p)^(1/p) are concave four-pointed stars.

var SEED = 4.42
var HUE = 0.52                 // icy cyan-blue

// eighth-turn rotation, cached at startup
var C8 = cos(PI / 4)
var S8 = sin(PI / 4)

var speedDiv = 1.7             // timebase divisor (from the speed slider)
//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  speedDiv = 0.25 + (1 - v) * 3   // inverse, floored so it never stops
}

var acc = 0                    // time accumulator
var rc = 1, rs = 0             // slow-rotation sin/cos this frame
var tw1 = 0, tw2 = 0           // twinkle amounts (modest / larger)

export function beforeRender(delta) {
  acc += delta / 1000 / speedDiv
  if (acc > 3600) acc -= 3600  // wrap ~hourly to avoid precision loss

  // very slow spin, wrapping every half turn
  var a = mod(acc * 0.012, PI)
  rc = cos(a)
  rs = sin(a)

  // one smooth 1D noise stream -> two scaled twinkle copies
  var n = perlin(acc * 0.9, 3.7, 11.2, SEED)
  tw1 = n * 0.05
  tw2 = n * 0.12
}

// p-th root of (|x|^p + |y|^p); floored to dodge division-by-zero at center
function minkowski(x, y, p) {
  return max(pow(pow(abs(x), p) + pow(abs(y), p), 1 / p), 0.001)
}

export function render2D(index, x, y) {
  // recenter, apply the slow rotation
  var px = x - 0.5
  var py = y - 0.5
  var ax = px * rc - py * rs
  var ay = px * rs + py * rc

  // star 1, axis-aligned rays: noise modulates its size
  var b1 = pow(clamp((0.11 + tw1) / minkowski(ax, ay, 0.5), 0, 1), 5)

  // star 2, diagonal rays: noise modulates the exponent (pointiness)
  var qx = ax * C8 - ay * S8
  var qy = ax * S8 + ay * C8
  var p2 = clamp(0.33 - tw2, 0.08, 1)
  var b2 = pow(clamp(0.11 / minkowski(qx, qy, p2), 0, 1), 4)

  var v = (b1 + b2) / 2
  // bright core desaturates toward white; dim ray tips stay deep blue
  var s = clamp(1.15 - v, 0, 1)
  hsv(HUE, s, v)
}
