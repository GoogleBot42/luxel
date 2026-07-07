// name: xorcery 2D/3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "xorcery 2D/3D"; original source never consulted.

// A stateless coordinate-driven field: XOR of centered, up-scaled fixed-point
// coordinates produces reflected, self-similar blocky cells; multi-rate clocks
// zoom, breathe and recolor it.

var rampMed, phaseMed, triSlow, sinSlow

export function beforeRender(delta) {
  rampMed = time(0.06)              // ~3.9 s ramp, kept raw
  phaseMed = time(0.06) * PI2       // same period as a sine phase
  triSlow = triangle(time(0.3))     // ~5x slower, triangle shaped
  sinSlow = sin(time(0.1) * PI2)    // a bit slower than medium, as a sine
}

export function render3D(index, x, y, z) {
  // 1. center, scale up, XOR the raw fixed-point bits, bring back down
  var cx = (x - 0.5) * 4
  var cy = (y - 0.5) * 4
  var cz = (z - 0.5) * 4
  var v = (cx ^ cy ^ cz) / 12

  // 2. slow zoom / counter-zoom
  v = v * (triSlow * 3 + sinSlow * 0.8)

  // 3. breathing modulus, full-wave shaping, medium sine
  var m = 0.5 + 0.2 * triangle(rampMed)
  var raw = wave(v % m) + sin(phaseMed)

  // 4. high-contrast brightness: wrap, square, triangle, cube
  var b = frac(abs(raw) + abs(m) + rampMed)
  b = b * b
  b = triangle(b)
  b = b * b * b

  // 5. narrow field perturbation + diagonal gradient + steady rotation
  var h = triangle(raw) * 0.2 + (x + y + z) / 3 + rampMed

  hsv(h, 1, b)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}

export function render(index) {
  render3D(index, index / pixelCount, 0, 0)
}
