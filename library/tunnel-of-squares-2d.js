// name: Tunnel of Squares 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Tunnel of Squares 2D"; original source never consulted.

// Concentric, slightly spiral-twisted square rings rushing outward from the
// center, log-spaced for a tunnel-perspective illusion. Hue sweeps radially
// and drifts slowly around the wheel.

// Fixed twist angle (~0.1 rad), precomputed as cos/sin once at startup.
var TWIST = 0.12
var twC = cos(TWIST)
var twS = sin(TWIST)

// Speed slider: ~10x range, never fully stops.
var flowRate = 7.75  // rad/s at default slider position
//# min=0 max=1 step=0.01 default=0.75
export function sliderSpeed(v) {
  flowRate = 1 + v * 9
}

// "Squarocity": integer rings per log-octave, 1..7.
var rings = 4
//# min=0 max=1 step=0.01 default=0.5
export function sliderSquarocity(v) {
  rings = floor(1 + v * 6.99)
}

var flowPhase = 0   // fast animation phase (kept wrapped so it never overflows)
var hueBase = 0     // slow global hue drift

export function beforeRender(delta) {
  flowPhase += delta / 1000 * flowRate
  if (flowPhase > PI2) flowPhase -= PI2   // wrap: sin() only cares mod 2*pi
  hueBase = time(0.1)                     // full hue drift ~6.5 s
}

export function render2D(index, x, y) {
  // center the unit square
  var px = x - 0.5
  var py = y - 0.5

  // Sign vector of the position, rotated by the fixed twist angle...
  var sx = px > 0 ? 1 : (px < 0 ? -1 : 0)
  var sy = py > 0 ? 1 : (py < 0 ? -1 : 0)
  var rx = sx * twC - sy * twS
  var ry = sx * twS + sy * twC

  // ...dotted with the position: a twisted diamond/square norm (~|x|+|y|).
  var m = px * rx + py * ry
  if (m < 0.002) m = 0.002   // floor away the log singularity at dead center

  // Log-spacing packs rings tight in the middle, exponentially wider outward.
  var phase = rings * log(m) + atan2(py, px) - flowPhase

  // |sin| cubed: crisp bright rings with deep dark gaps.
  var b = abs(sin(phase))
  b = b * b * b

  // Hue: radial sweep by the (un-logged) square metric plus the slow drift.
  hsv(hueBase + m, 1, b)
}
