// name: matrix 2D pulse edit
// Clean-room reimplementation from a prose functional description of the
// community pattern "matrix 2D pulse edit"; original source never consulted.

// A dim "twilight" plasma driven entirely by the linear pixel index. The
// character depends on two deliberate quirks preserved from the original:
// the row divisor (3) disagrees with the declared matrix width (8), and the
// column is the index modulo a *fractional, time-varying* zoom factor —
// which makes the cell width visibly breathe between fine stripes and
// broad blobs.

var matrixWidth = 8
var serpentine = 0 // internal constant: mirror columns on alternate rows

var fastPhase = 0
var slowPhase = 0
var zoom = 1

export function beforeRender(delta) {
  fastPhase = time(0.06) * PI2 // full circle every ~4 s
  slowPhase = time(0.12) * PI2 // full circle every ~8 s
  zoom = 2.5 + 1.5 * sin(time(0.2) * PI2) // 1..4, breathing over ~13 s
}

export function render(index) {
  var row = floor(index / 3) // deliberately NOT the matrix width
  var col = mod(index, zoom) // fractional modulo: the breathing glitch

  if (serpentine && mod(floor(index / matrixWidth), 2) == 1) {
    col = matrixWidth - 1 - col
  }

  // Two-term plasma with a small positive bias; spills a bit past 0..1.
  var h = 0.1 +
    (sin(col * zoom / matrixWidth + fastPhase) +
     cos(row * zoom / matrixWidth + slowPhase)) / 2

  // Brightness from the raw pre-fold hue: signed cube, scaled well down.
  // Negative values render black, so only positive crests glow.
  var v = h * h * h / 3
  if (v < 0) v = 0

  // Fold the top of the hue range back to keep one sector of the wheel out.
  var hue = h
  if (hue > 0.75) hue = mod(hue, 0.62)

  hsv(hue, 1, v)
}
