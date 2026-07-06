// name: Matrix 2 tone pulse
// Clean-room reimplementation from a prose functional description of the
// community pattern "Matrix 2 tone pulse"; original source never consulted.

// A two-wave interference plasma locked to exactly two hues: a primary
// tone and its complement, always separated by black seams (regions fade
// down through black before coming back up in the other color — the two
// tones never blend). A slow "zoom" oscillation breathes the spatial
// scale. Index-based on purpose: the pseudo-column is index modulo a
// FRACTIONAL, animated scale factor — an aliasing artifact that is the
// whole character of the churning cell texture. Do not "fix" it.

var WIDTH = 8          // nominal matrix width (index math only)
var ROW_DIV = 3        // pseudo-row divisor (deliberately != WIDTH)
var BAND = 0.1         // hue band width around each tone

var primaryHue = 0.22  // yellow-green; complement lands blue-violet
var zigzag = 0         // serpentine wiring compensation

export function hsvPickerPrimaryTone(h, s, v) { primaryHue = h }
export function toggleZigzag(v) { zigzag = v }

var t1, t2, zoom

export function beforeRender(delta) {
  t1 = time(0.048) * PI2                 // ~3.1 s phase clock
  t2 = time(0.071) * PI2                 // ~4.7 s phase clock (never syncs)
  zoom = 3 + 2 * sin(time(0.19) * PI2)   // 1..5, ~12.5 s breathing
}

export function render(index) {
  var i = index
  if (zigzag) {
    var row = floor(index / WIDTH)
    if (mod(row, 2) == 1) i = row * WIDTH + WIDTH - 1 - mod(index, WIDTH)
  }

  var pr = i / ROW_DIV        // fractional pseudo-row
  var pc = mod(i, zoom)       // index mod a fractional ANIMATED number

  // classic two-wave interference field, wrapped to just below 1
  var v = (1 + sin(pc * zoom / WIDTH + t1) + cos(pr * zoom / WIDTH + t2)) / 2
  v = mod(v, 0.996)

  // doubled-frequency triangle: hits zero exactly where the hue mapping
  // switches halves, hiding every tone transition in black
  var seam = triangle(v * 2)

  // fold the scalar into two narrow bands half a wheel apart
  var h
  if (v < 0.5) h = primaryHue - BAND / 2 + v * 2 * BAND
  else         h = primaryHue + 0.5 - BAND / 2 + (v - 0.5) * 2 * BAND

  hsv(h, 1, v * v * v * seam)   // cubed: dark field, bright blooms
}
