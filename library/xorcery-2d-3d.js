// name: xorcery 2D/3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "xorcery 2D/3D"; original source never consulted.

// A stateless coordinate-driven field: XOR of centered, up-scaled fixed-point
// coordinates produces reflected, self-similar blocky cells; multi-rate clocks
// zoom, breathe and recolor it.

var rampMed, phaseMed, triSlow, sinSlow

var zoom = 4        // coordinate scale: cells across the field
var speed = 1       // clock-rate multiplier
var hueSpread = 0.2 // how much the field perturbs the hue
var breathe = 0.2   // modulus breathing depth

export function beforeRender(delta) {
  rampMed = time(0.06 / speed)      // ~3.9 s ramp, kept raw
  phaseMed = time(0.06 / speed) * PI2  // same period as a sine phase
  triSlow = triangle(time(0.3 / speed))   // ~5x slower, triangle shaped
  sinSlow = sin(time(0.1 / speed) * PI2)  // a bit slower than medium, as a sine
}

// Coordinate scale of the XOR field — roughly how many self-similar cells span
// the display. Bigger values give finer, busier blockwork.
//# min=1 max=16 step=0.5 default=4
export function sliderZoom(v) {
  zoom = clamp(v, 1, 16)
}

// Overall animation rate: 1 is the natural pace, 4 is four times as fast.
//# min=0.1 max=4 step=0.05 default=1
export function sliderSpeed(v) {
  speed = clamp(v, 0.1, 4)
}

// How far the field itself pushes the hue around the wheel, on top of the
// steady diagonal gradient. 0 leaves a clean rolling rainbow.
//# min=0 max=1 step=0.01 default=0.2
export function sliderColorSpread(v) {
  hueSpread = clamp(v, 0, 1)
}

// Depth of the breathing modulus that folds the field: 0 holds a fixed fold
// width, larger values pump the cell structure in and out.
//# min=0 max=0.45 step=0.01 default=0.2
export function sliderBreathe(v) {
  breathe = clamp(v, 0, 0.45)
}

export function render3D(index, x, y, z) {
  // 1. center, scale up, XOR the raw fixed-point bits, bring back down
  var cx = (x - 0.5) * zoom
  var cy = (y - 0.5) * zoom
  var cz = (z - 0.5) * zoom
  var v = (cx ^ cy ^ cz) / (zoom * 3)

  // 2. slow zoom / counter-zoom
  v = v * (triSlow * 3 + sinSlow * 0.8)

  // 3. breathing modulus, full-wave shaping, medium sine
  var m = 0.5 + breathe * triangle(rampMed)
  var raw = wave(v % m) + sin(phaseMed)

  // 4. high-contrast brightness: wrap, square, triangle, cube
  var b = frac(abs(raw) + abs(m) + rampMed)
  b = b * b
  b = triangle(b)
  b = b * b * b

  // 5. narrow field perturbation + diagonal gradient + steady rotation
  var h = triangle(raw) * hueSpread + (x + y + z) / 3 + rampMed

  hsv(h, 1, b)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}

export function render(index) {
  render3D(index, index / pixelCount, 0, 0)
}
