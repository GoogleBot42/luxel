// name: Matrix Green Waterfall 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Matrix Green Waterfall 2D"; original source never
// consulted.

// Minimal "digital rain": each column's repeating falling ramp is a
// floored modulo of (y minus a scrolling phase) by a per-column cycle
// length derived from a smooth wave of x. Squaring the ramp darkens the
// tail and sharpens the head; a saturation dip above ~4/5 brightness
// bleaches just the streak heads toward white. No per-drop state at all.

var speed = 20     // integer fall-rate multiplier, 0..50
var fallPhase = 0
var colFreq = 8

//# min=0 max=50 step=1 default=20
export function sliderSpeed(v) {
  speed = floor(v * 50)
}

export function beforeRender(delta) {
  // phase sweeps many cycles per clock period; the modulo folds it
  fallPhase = (time(0.25) + 0.015) * speed
  // base column frequency assumes a roughly square matrix; the slow drift
  // keeps the columns from locking into a fixed pattern
  colFreq = sqrt(pixelCount) / 2 + time(0.9) * 0.35
}

export function render2D(index, x, y) {
  var cycle = wave(x * colFreq)          // per-column repeat interval
  var v = mod(y - fallPhase, cycle)      // floored mod: always positive
  v = v * v                              // gamma: dark tail, hot head
  var s = 1
  if (v > 0.8) s = 1 - 0.085             // whiten the streak head
  hsv(0.33, s, v)
}
