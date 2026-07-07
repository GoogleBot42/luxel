// name: Carrie's Holiday Star 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Carrie's Holiday Star 2D"; original source never consulted.

// An eight-pointed star: four long axis rays + four shorter diagonal rays,
// built from a generalized (Minkowski) p-norm with p < 1 so iso-distance
// contours are concave four-pointed stars. Smooth noise makes it twinkle,
// the whole figure rotates slowly, and the base hue drifts over minutes.

var period = 2.25          // master time period (seconds per accumulator unit)
//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  // higher = faster; inverse map onto the period with a floor so it never stops
  period = 0.5 + 3.5 * (1 - v)
}

var tAcc = 0               // master time accumulator
var baseHue = 0.55         // starts in cyan/blue territory
var rotC = 1               // frame rotation, precomputed per frame
var rotS = 0
var numA = 0.16            // axis-star ray reach (fixed + twinkle)
var numD = 0.1             // diagonal-star ray reach
var p2 = 0.55              // diagonal star p-norm exponent (breathes with noise)
var inv2 = 1.818           // 1 / p2, recomputed alongside it

// eighth-of-a-turn rotation for the diagonal star (sin/cos of PI/4)
var C8 = 0.7071
var S8 = 0.7071

export function beforeRender(delta) {
  // advance by elapsed time / period, wrapped mod ~an hour to keep precision
  tAcc = mod(tAcc + delta / 1000 / period, 3600)

  // hue drift: tiny nudge scaled by elapsed time (full wheel in ~4 minutes)
  baseHue = mod(baseHue + delta * 0.000004, 1)

  // slow global rotation: on the order of a minute per revolution at default
  var ang = tAcc * 0.3
  rotC = cos(ang)
  rotS = sin(ang)

  // one smooth noise sample drives the twinkle for the whole frame:
  // it modulates the axis star's reach and the diagonal star's pointiness
  var tw = simplex2(tAcc * 1.5, 4.31)
  numA = 0.16 + 0.08 * tw
  numD = 0.1 + 0.05 * tw
  p2 = 0.55 + 0.12 * tw
  inv2 = 1 / p2
}

export function render2D(index, x, y) {
  // center the origin, then rotate the whole frame
  var cx = x - 0.5
  var cy = y - 0.5
  var px = cx * rotC - cy * rotS
  var py = cx * rotS + cy * rotC

  // axis-aligned star: generalized (Minkowski) distance with p = 0.55 —
  // the p-th root of the sum of p-th powers gives concave star contours.
  // Reach over distance, clamped, then sharpened hard so rays are crisp.
  var ax = abs(px)
  var ay = abs(py)
  var d1 = pow(pow(ax, 0.55) + pow(ay, 0.55), 1.818)
  var r1 = clamp(numA / d1, 0, 1)
  var v1 = r1 * r1 * r1 * r1 * r1

  // diagonal star: rotate an eighth turn, smaller reach, and the p-norm
  // exponent itself breathes with the noise so the rays change shape;
  // sharpened a touch less than the axis star
  var dx = abs(px * C8 - py * S8)
  var dy = abs(px * S8 + py * C8)
  var d2 = pow(pow(dx, p2) + pow(dy, p2), inv2)
  var r2 = clamp(numD / d2, 0, 1)
  var v2 = r2 * r2 * r2 * r2

  var v = (v1 + v2) / 2

  // center fix: at the exact origin the distance is zero and the division
  // yields zero; detect coordinates at the origin and force near-full bright
  if (cx * cx + cy * cy < 0.0005) v = 0.95

  // bright core desaturates toward white; dim ray tips stay fully saturated
  hsv(baseHue, 1.7 - v, v)
}
