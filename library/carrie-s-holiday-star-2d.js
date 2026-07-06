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
var twinkle = 0            // per-frame noise value in ~[-1, 1]
var rotC = 1
var rotS = 0

// eighth-of-a-turn rotation for the diagonal star, precomputed once
const C8 = cos(PI / 4)
const S8 = sin(PI / 4)

// generalized (Minkowski) distance: p-th root of sum of p-th powers
function mink(ax, ay, p) {
  return pow(pow(ax, p) + pow(ay, p), 1 / p)
}

export function beforeRender(delta) {
  // advance by elapsed time / period, wrapped mod ~an hour to keep precision
  tAcc = mod(tAcc + delta / 1000 / period, 3600)

  // hue drift: tiny nudge scaled by elapsed time (full wheel in ~4 minutes)
  baseHue = mod(baseHue + delta * 0.000004, 1)

  // slow global rotation: on the order of a minute per revolution at default
  var ang = tAcc * 0.3
  rotC = cos(ang)
  rotS = sin(ang)

  // one smooth noise sample drives the twinkle for the whole frame
  twinkle = simplex2(tAcc * 1.5, 4.31)
}

export function render2D(index, x, y) {
  // center the origin, then rotate the whole frame
  var cx = x - 0.5
  var cy = y - 0.5
  var px = cx * rotC - cy * rotS
  var py = cx * rotS + cy * rotC

  // axis-aligned star: fixed-plus-twinkle reach over the star distance,
  // sharpened hard so the rays are crisp
  var d1 = mink(abs(px), abs(py), 0.55)
  var v1 = pow(clamp((0.13 + 0.07 * twinkle) / d1, 0, 1), 5)

  // diagonal star: rotate an eighth turn, smaller reach, and the p-norm
  // exponent itself breathes with the noise so the rays change shape
  var dx = px * C8 - py * S8
  var dy = px * S8 + py * C8
  var p2 = 0.55 + 0.14 * twinkle
  var d2 = mink(abs(dx), abs(dy), p2)
  var v2 = pow(clamp((0.09 + 0.05 * twinkle) / d2, 0, 1), 4)

  var v = (v1 + v2) / 2

  // center fix: at the exact origin the distance is zero and the division
  // misbehaves; detect coordinates at the origin and force near-full bright
  if (cx * cx + cy * cy < 0.0005) v = 0.95

  // bright core desaturates toward white; dim ray tips stay fully saturated
  hsv(baseHue, 1.7 - v, v)
}
