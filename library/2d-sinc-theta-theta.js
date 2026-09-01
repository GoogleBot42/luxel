// name: 2D sinc(theta)/theta
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D sinc(theta)/theta"; original source never consulted.

// Three ripple-ring systems -- pure red, green, blue -- expand outward from
// slowly wandering centers and blend additively where they cross. The
// "sinc" character comes from dividing the ring oscillation by the distance
// to the center: a hot saturated core with a 1/r decay envelope. The three
// channels share one mechanism, differing only in fixed phase offsets.

// speed: inverted, right = faster; period from ~2 min down to a few seconds
var speedInterval = 0.635
//# min=0 max=1 step=0.01 default=0.7
export function sliderSpeed(v) { speedInterval = mix(2, 0.05, v) }

// Spatial density of the rings, in ring crests per panel width. The rate of
// phase advance is derived from it (unity at the default density), so denser
// rings also move busier -- the two are one control, as in the original.
var ringsPerUnit = 1.3
var phaseGain = 1
//# min=0.8 max=3 step=0.05 default=1.3
export function sliderScale(v) {
  ringsPerUnit = clamp(v, 0.8, 3)
  phaseGain = ringsPerUnit / 1.3
}

// Centre-wander period along one axis, as a MULTIPLE of the main ring period:
// right = slower drift (0.05 = frantic churn, 4 = the lobes almost hold still).
var sizeB = 0.2
//# min=0.05 max=4 step=0.05 default=0.2
export function sliderSizeB(v) { sizeB = clamp(v, 0.05, 4) }

// The same, for the other axis (dormant in the original; wired here the way
// size B drives its axis, which is the obvious repair).
var sizeC = 0.3
//# min=0.05 max=4 step=0.05 default=0.3
export function sliderSizeC(v) { sizeC = clamp(v, 0.05, 4) }

// integer ring-contrast exponent: 1 (soft, washed) .. 6 (thin, crisp)
var gammaExp = 3
//# min=1 max=6 step=1 default=3
export function sliderGamma(v) { gammaExp = clamp(floor(v + 0.5), 1, 6) }

// per-channel fixed phase offsets (quarter to half a turn apart)
var offR = 0
var offG = PI2 * 0.25
var offB = PI2 * 0.45

var ringPhase = 0, wanderX = 0, wanderY = 0

export function beforeRender(delta) {
  // main outward ring motion
  ringPhase = time(speedInterval) * PI2 * phaseGain
  // center wandering: two phases derived from the main period
  wanderX = time(speedInterval * sizeB) * PI2 * phaseGain
  wanderY = time(speedInterval * sizeC) * PI2 * phaseGain
}

// one channel's ripple field at (x, y)
function ripple(x, y, off) {
  // wandering center: cosines span about twice the panel width, so the
  // center is frequently off-screen and waves sweep in from outside
  var cx = 0.5 + cos(wanderX + off)
  var cy = 0.5 + cos(wanderY + off * 2)
  var d = hypot(x - cx, y - cy)
  // ring oscillation divided by distance = the sinc trick (div-by-0 -> 0)
  var v = 1 - cos(d * PI2 * ringsPerUnit - ringPhase + off) / d
  // keep pow() well away from fixed-point wraparound near the hot core
  v = clamp(v, -1, 3)
  return pow(v, gammaExp)
}

export function render2D(index, x, y) {
  rgb(
    ripple(x, y, offR),
    ripple(x, y, offG),
    ripple(x, y, offB)
  )
}
