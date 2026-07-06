// name: All Lasers Fire
// Clean-room reimplementation from a prose functional description of the
// community pattern "All Lasers Fire"; original source never consulted.

// Volleys of laser beams and sparks firing up from the bottom corner,
// morphing each cycle between orderly grids/fans and chaotic spray. The
// scalar field is rendered three times — once per RGB channel — with tiny
// spatial and phase offsets, faking chromatic aberration: white-hot cores
// with rainbow fringes. The tangent term is the chaos/order oscillator:
// its poles shatter the tiling, and a per-cycle decaying chaos factor
// lets every cycle relax back to order before re-arming.

export var speed = 0.5
export var blastScale = 1

var interval = 0.02

export function sliderSpeed(v) {
  //# min=0 max=1 step=0.01 default=0.5
  speed = v
  // inverted and cubed: the right end is dramatically faster; a tiny
  // floor keeps it from ever fully stopping
  interval = 0.004 + pow(1 - v, 3) * 0.1
}

export function sliderBlastScale(v) {
  //# min=0 max=1 step=0.01 default=0.1
  blastScale = 0.1 + v * 9.9   // fine detail up to ~10x, linear
}

var t = 0
var angle = 0
var chaos = 0
var env = array(3)

export function beforeRender(delta) {
  t = time(interval)
  angle = t * PI2
  chaos = (1 - t) * 0.3   // re-arms each cycle, decays to pure order
  // per-channel envelopes, each phase nudged slightly later
  env[0] = blastScale + wave(t)
  env[1] = blastScale + wave(t + 0.03)
  env[2] = blastScale + wave(t + 0.06)
}

var out = array(3)

function firePoint(px, py) {
  var x = px
  var y = py
  var d = hypot(x, y)
  for (var c = 0; c < 3; c++) {
    // tangent poles sweep from one giant beam through orderly grids into
    // explosive chaos; distance inside the argument makes chaos rings
    // propagate outward from the origin
    var s = d * env[c] * tan(d * chaos - angle - c * 0.02)
    var fx = frac(x / s)
    var fy = frac(y / s)
    // point-light falloff toward a spot just past the tile center,
    // blazing hotter near the origin, cubed for hard contrast
    var v = 0.06 / hypot(fx - 0.6, fy - 0.6) / d
    v = clamp(v, 0, 4)
    out[c] = v * v * v
    // nudge the sampling point down-left so the next channel sees an
    // almost-but-not-quite identical field
    x -= 0.004
    y -= 0.004
    d -= 0.00566
  }
  rgb(out[0], out[1], out[2])
}

export function render2D(index, x, y) {
  // flip vertical so the beams radiate up from the bottom corner
  // (change to taste for your mounting orientation)
  firePoint(x, 1 - y)
}

export function render(index) {
  // 1D: a line along the field's bottom edge; longer strips span a
  // proportionally wider slice
  firePoint(index / (3 * sqrt(pixelCount)), 0.05)
}
