// name: Crosstown Traffic 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Crosstown Traffic 2D"; original source never consulted.

// Thin soft-edged colored bars — car headlights on a street grid — glide
// across a matrix, half horizontal and half vertical, each in its own
// lane at its own random speed and length. Hues fan evenly across the
// cars and the whole assignment slowly rotates through the rainbow.
// Cars slide fully off one edge and re-enter from the other.
//
// Map-driven (normalized 2D coordinates) instead of the original's
// sqrt(pixelCount) square-matrix assumption; vertical traffic reuses the
// horizontal-segment math by transposing the query point into
// temporaries per line (the original's in-place swap leak is not
// preserved — the spec calls it an accident).

var NUM_LINES = 24                 // ~2x a small matrix width
var LANES = NUM_LINES / 2          // lanes per orientation

// per-line state (randomized once at startup; motion is deterministic)
var halfLen = array(NUM_LINES)
var speedMul = array(NUM_LINES)
var lane = array(NUM_LINES)
var isVert = array(NUM_LINES)
// per-frame derived state
var hueOf = array(NUM_LINES)
var lo = array(NUM_LINES)          // segment endpoints along travel axis
var hi = array(NUM_LINES)

var i
for (i = 0; i < NUM_LINES; i++) {
  halfLen[i] = 0.25 + random(0.25)          // quarter..half of the span
  speedMul[i] = 0.5 + random(2)             // 0.5x..2.5x base speed
  isVert[i] = mod(i, 2)                     // alternate orientations
  lane[i] = (floor(i / 2) + 0.5) / LANES    // own lane, centered in slot
}

// --- controls ---
var lineWidth = 0.06
//# min=0 max=1 step=0.01 default=0.35
export function sliderLineWidth(v) { lineWidth = 0.015 + v * 0.13 }

var sweepInterval = 0.16
//# min=0 max=1 step=0.01 default=0.7
export function sliderLineSpeed(v) {
  // squared curve: fine control at the slow end, up = faster
  sweepInterval = 0.02 + 0.45 * (1.02 - v * v)
}

var colorInterval = 0.35
//# min=0 max=1 step=0.01 default=0.6
export function sliderColorSpeed(v) {
  colorInterval = 0.03 + 0.8 * (1.02 - v * v)
}

export function beforeRender(delta) {
  var colorClock = time(colorInterval)
  var j
  for (j = 0; j < NUM_LINES; j++) {
    // hues fanned evenly across lines, all rotating together
    hueOf[j] = mod(colorClock + j / NUM_LINES, 1)
    // per-line sawtooth stretched to sweep well past both edges (x4)
    // so cars exit fully before wrapping back in
    var saw = time(sweepInterval * speedMul[j])
    var center = 0.5 + (saw - 0.5) * 4
    lo[j] = center - halfLen[j]
    hi[j] = center + halfLen[j]
  }
}

export function render2D(index, x, y) {
  var j
  for (j = 0; j < NUM_LINES; j++) {
    // transpose trick: vertical traffic = horizontal math on swapped axes
    var px = x
    var py = y
    if (isVert[j]) { px = y; py = x }

    // true distance from point to the finite segment (rounded bar ends):
    // beyond either endpoint -> distance to that endpoint, else
    // perpendicular distance to the centerline
    var d
    if (px < lo[j])      d = dist(px, py, lo[j], lane[j])
    else if (px > hi[j]) d = dist(px, py, hi[j], lane[j])
    else                 d = abs(py - lane[j])

    if (d < lineWidth) {
      // first hit wins; brightness falls linearly to the width edge
      hsv(hueOf[j], 1, 1 - d / lineWidth)
      return
    }
  }
  rgb(0, 0, 0)   // no car here: black street
}
