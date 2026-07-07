// name: 2D sinc(theta)/theta
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D sinc(theta)/theta"; original source never consulted.

// Three ripple-ring systems -- pure red, green, blue -- expand outward from
// slowly wandering centers and blend additively where they cross. The
// "sinc" character comes from dividing the ring oscillation by the distance
// to the center: a hot saturated core with a 1/r decay envelope. The three
// channels share one mechanism, differing only in fixed phase offsets.

var edge = sqrt(pixelCount)       // proxy for the panel's edge length

// speed: inverted, right = faster; period from ~2 min down to a few seconds
var speedInterval = 0.6
//# min=0 max=1 step=0.01 default=0.7
export function sliderSpeed(v) { speedInterval = mix(2, 0.05, v) }

// spatial density of the rings + rate of phase advance together
var scaleF = 1
//# min=0 max=1 step=0.01 default=0.25
export function sliderScale(v) { scaleF = 0.25 + v * 3 }

// center wandering rate along one axis (right = slower)
var sizeB = 4
//# min=0 max=1 step=0.01 default=0.25
export function sliderSizeB(v) { sizeB = 1 + v * edge }

// center wandering rate along the other axis (dormant in the original;
// wired here the same way size B drives its axis)
var sizeC = 3
//# min=0 max=1 step=0.01 default=0.15
export function sliderSizeC(v) { sizeC = 1 + v * edge }

// integer ring-contrast exponent: 1 (soft, washed) .. 6 (thin, crisp)
var gammaExp = 2
//# min=0 max=1 step=0.2 default=0.2
export function sliderGamma(v) { gammaExp = 1 + floor(v * 5.001) }

// per-channel fixed phase offsets (quarter to half a turn apart)
var offR = 0
var offG = PI2 * 0.25
var offB = PI2 * 0.45

var ringPhase = 0, wanderX = 0, wanderY = 0

export function beforeRender(delta) {
  // main outward ring motion
  ringPhase = time(speedInterval) * PI2 * scaleF
  // center wandering: two faster phases derived from the main period
  wanderX = time(speedInterval / sizeB) * PI2 * scaleF
  wanderY = time(speedInterval / sizeC) * PI2 * scaleF
}

// one channel's ripple field at (x, y)
function ripple(x, y, off) {
  // wandering center: cosines span about twice the panel width, so the
  // center is frequently off-screen and waves sweep in from outside
  var cx = 0.5 + cos(wanderX + off)
  var cy = 0.5 + cos(wanderY + off * 2)
  var d = hypot(x - cx, y - cy)
  // ring oscillation divided by distance = the sinc trick (div-by-0 -> 0)
  var v = 1 - cos(d * PI2 * 3 * scaleF - ringPhase + off) / d
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
