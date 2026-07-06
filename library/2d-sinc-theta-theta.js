// name: 2D sinc(theta)/theta
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D sinc(theta)/theta"; original source never consulted.

// Three concentric ripple systems (pure R, G, B) expand from slowly
// wandering centers and blend additively where they cross. The "sinc"
// character comes from dividing the ring oscillation by the radius: a hot
// saturated core with a 1/r decay envelope. The three channels share the
// machinery, differing only in fixed phase offsets.

var edge = sqrt(pixelCount)   // proxy for panel edge length

var speedV = 0.2
//# min=0 max=1 step=0.01 default=0.2
export function sliderSpeed(v) { speedV = v }   // right = faster

var scaleF = 1
//# min=0 max=1 step=0.01 default=0.2
export function sliderScale(v) { scaleF = 0.25 + v * 3.75 }

var divB = 5
//# min=0 max=1 step=0.01 default=0.5
export function sliderSizeB(v) { divB = 1 + (1 - v) * edge * 0.5 }  // right = slower

// Size C was inert in the original; here it drives the other axis the
// same way Size B drives the first (the spec's suggested fix).
var divC = 3
//# min=0 max=1 step=0.01 default=0.75
export function sliderSizeC(v) { divC = 1 + (1 - v) * edge * 0.5 }

var gammaI = 3
//# min=0 max=1 step=0.01 default=0.4
export function sliderGamma(v) { gammaI = floor(1 + v * 5.99) }  // integer 1..6

var ringPhase, cxPhase, cyPhase

export function beforeRender(delta) {
  // main period: a few seconds (right) up to ~2 minutes (left)
  var interval = 0.04 + (1 - speedV) * 1.9
  ringPhase = time(interval) * PI2 * scaleF
  // center wander: main period divided by the size factors
  cxPhase = time(interval / divB) * PI2 * scaleF
  cyPhase = time(interval / divC) * PI2 * scaleF
}

function channel(x, y, off) {
  // wandering center: cosines span ~2x the panel, so centers roam off-screen
  var cx = cos(cxPhase + off)
  var cy = cos(cyPhase + off * 1.5)
  var d = hypot(x - cx, y - cy)
  // ring field: 1 - cos(...)/d -- huge near the center, 1/r decay outward
  var v = 1 - cos(d * PI2 * 2 * scaleF - ringPhase + off) / d
  // keep pow() in range; negatives stay well-defined with an integer exponent
  v = clamp(v, -1, 5)
  return pow(v, gammaI)   // contrast: thins rings, deepens gaps
}

export function render2D(index, x, y) {
  // quarter- to half-turn offsets keep the three systems related but distinct
  rgb(channel(x, y, 0),
      channel(x, y, PI2 * 0.25),
      channel(x, y, PI2 * 0.5))
}
