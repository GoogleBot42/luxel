// name: Sunset
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sunset"; original source never consulted.

// Slow two-axis interference plasma tuned for indirect ambient lighting:
// soft blobs of warm blended color breathe over many seconds. At any
// instant only a narrow wedge of the wheel is visible (so bounced light
// stays colored instead of averaging white) and that wedge itself drifts
// around the whole rainbow over tens of seconds. Pure 1D renderer that
// maps the strip onto a serpentine matrix with a hardcoded width, per
// the original — "wrong" widths shear the plasma in interesting ways.

var WIDTH = 12       // hardcoded matrix width (columns)
var ZIGZAG = 1       // serpentine wiring: every other row reversed
var MASTER = 1       // overall brightness scale
var CONTRAST = 2     // hue-range divisor: small = monochrome, big = white-ish
var ROWS = ceil(pixelCount / WIDTH)

var ph1, ph2, zoom, hueDrift, brtPhase, hueRoll

export function beforeRender(delta) {
  // slow oscillators with unequal periods so they beat against each other
  ph1 = wave(time(.17)) * PI2
  ph2 = wave(time(.23)) * PI2
  // spatial zoom breathes between ~2 and ~6 waves per matrix
  zoom = (2 + 4 * wave(time(.55))) * PI2
  hueDrift = wave(time(.37))
  brtPhase = wave(time(.13))
  hueRoll = time(.4)   // steady ramp: whole palette rotates through the wheel
}

export function render(index) {
  var row = floor(index / WIDTH)
  var col = index % WIDTH
  if (ZIGZAG && (row % 2)) col = WIDTH - 1 - col
  var u = col / WIDTH
  var v = row / ROWS

  // classic interference plasma field, roughly 0..1
  var f = (2 + sin(u * zoom + ph1) + cos(v * zoom + ph2)) / 4

  // cubed wave crushes midtones: mostly dim with soft bright islands
  var b = wave(f + brtPhase)
  b = b * b * b

  // hue: folded field (half range) + slow drift + brightness (two-tone
  // blobs: bright cores shift hue vs their fringes), compressed by the
  // contrast divisor, minus the slow full-wheel rotation
  var h = (triangle(f) / 2 + hueDrift + b) / CONTRAST - hueRoll

  hsv(h, 1, b * MASTER)
}
