// name: fast pulse 3d
// Clean-room reimplementation from a prose functional description of the
// community pattern "fast pulse 3d"; original source never consulted.

// Narrow, hard-edged pulses whip through the display on a sinusoidal
// sweep — fast through the middle, lingering at the extremes. Each pulse
// has a white-hot core with a saturated fringe whose hue cycles through
// the whole rainbow over a few seconds. In 3D the pulses are glowing
// planes whose orientation slowly tumbles (three mismatched sine
// oscillators act as the plane-normal components); 2D gets a flat slice
// of the same field; 1D gets racing dots.

var t = 0               // master phase: hue + motion driver (~3.3 s)
var ox = 0              // slowly tumbling axis weights
var oy = 0
var oz = 0

export function beforeRender(delta) {
  t = time(0.05)                       // ~3.3 s master period
  ox = sin(time(0.05) * PI2)           // matches the master period
  oy = sin(time(0.025) * PI2)          // ~half of it
  oz = sin(time(0.034) * PI2)          // ~two-thirds of it
}

export function render(index) {
  // Sinusoidal moving offset (scaled ~2x) + position, folded by a
  // triangle wave, then sharpened to the 5th power: thin pulses, dark gaps.
  var v = triangle(2 * sin(t * PI2) + index / pixelCount)
  v = v * v * v * v * v
  // Binary saturation: white-hot core in the top ~tenth of the range.
  hsv(t, v < 0.9, v)
}

export function render3D(index, x, y, z) {
  // Position term = dot product with the tumbling direction vector;
  // moving offset scaled ~3x. Core threshold a bit more generous.
  var v = triangle(3 * sin(t * PI2) + x * ox + y * oy + z * oz)
  v = v * v * v * v * v
  hsv(t, v < 0.8, v)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}
