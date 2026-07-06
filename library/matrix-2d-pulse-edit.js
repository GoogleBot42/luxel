// name: matrix 2D pulse edit
// Clean-room reimplementation from a prose functional description of the
// community pattern "matrix 2D pulse edit"; original source never consulted.

// A dim twilight plasma driven entirely by the linear pixel index. Its
// character depends on two deliberate "bugs" preserved from the original:
// the row divisor (3) disagrees with the declared matrix width (8), and the
// column value is the index modulo a *fractional, time-varying* zoom — that
// is what makes the cell width visibly breathe and glitch. Hue and
// brightness share one raw plasma field: the field is cubed (sign kept) for
// brightness so only positive crests glow out of black, then the hue is
// fold-limited to avoid the top sector of the wheel.

var WIDTH = 8                        // declared matrix width (row math ignores it)
var SERPENTINE = false               // mirror columns on alternate rows if wired zigzag
var HUE_BIAS = 0.1

var t1 = 0
var t2 = 0
var zoom = 1

export function beforeRender(delta) {
  t1 = time(0.05) * PI2              // fast drift, ~3.3 s per circle
  t2 = time(0.1) * PI2               // slower drift, ~6.5 s per circle
  zoom = 1 + 3 * wave(time(0.2))     // breathes 1..4 over ~13 s
}

export function render(index) {
  var row = floor(index / 3)         // deliberately NOT the matrix width
  var col = index % zoom             // fractional modulo: the breathing glitch
  if (SERPENTINE && mod(floor(index / WIDTH), 2) == 1) {
    col = WIDTH - 1 - col
  }

  // Two-term plasma; spans a bit beyond 0..1 in both directions.
  var h = HUE_BIAS + (sin(col * zoom / WIDTH + t1) + cos(row * zoom / WIDTH + t2)) / 2

  // Brightness from the same raw field: cube (sign-preserving), scale way
  // down. Negative half clamps to black, so only the crests glow.
  var v = h * h * h / 3
  if (v < 0) v = 0

  // Fold hues above ~3/4 back through modulo — keeps one sector of the
  // wheel unused and causes the occasional abrupt hue jump at the fold.
  if (h > 0.75) h = h % 0.64

  hsv(h, 1, v)
}
