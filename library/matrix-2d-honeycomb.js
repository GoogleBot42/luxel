// name: matrix 2D honeycomb
// Clean-room reimplementation from a prose functional description of the
// community pattern "matrix 2D honeycomb"; original source never consulted.

// A full-rainbow plasma of soft cellular blobs drifting over a matrix.
// The original decoded a hardcoded 8-wide serpentine layout from the pixel
// index; here it is a true 2D renderer fed by the pixel mapper instead.
// No state — everything derives from slow, mutually incommensurate clocks.

var phi1 = 0        // phase of the x plasma term (radians)
var phi2 = 0        // phase of the y plasma term (radians)
var spatialFreq = 4 // how many blob cells fit across the panel (~2..7)
var hueOffset = 0   // slow palette rotation
var shimmer = 0     // fast phase for the per-cell brightness pulse

export function beforeRender(delta) {
  // Two slow triangle-smoothed oscillators, ~1 min periods that differ
  // slightly so the pattern beats and never exactly repeats.
  phi1 = triangle(time(0.83)) * PI2   // ~54 s
  phi2 = triangle(time(0.97)) * PI2   // ~64 s

  // Blob scale breathes between ~2 and ~7 over tens of seconds.
  spatialFreq = 2 + 5 * triangle(time(0.37))   // ~24 s

  // Slow hue rotation, tens of seconds around the whole wheel.
  hueOffset = time(0.55)                       // ~36 s

  // Fast sawtooth for the brightness shimmer, a few seconds per pulse.
  shimmer = time(0.045)                        // ~3 s
}

export function render2D(index, x, y) {
  // Smooth plasma field, roughly -0.5 .. 1.5
  var field = (1 + sin(x * spatialFreq + phi1) + cos(y * spatialFreq + phi2)) / 2

  // Brightness: shimmer phase added to the field, triangle-folded, then
  // cubed to crush mid values — crisp bright cells on near-black valleys.
  var v = triangle(field + shimmer)
  v = v * v * v

  // Hue: fold the field with a triangle so it sweeps up and back down
  // (no hard wrap seam), spread over half the wheel, plus the slow rotation.
  var h = triangle(frac(field)) / 2 + hueOffset

  hsv(h, 1, v)
}

// 1D fallback: treat the strip as a line across the plasma.
export function render(index) {
  render2D(index, index / pixelCount, 0)
}
