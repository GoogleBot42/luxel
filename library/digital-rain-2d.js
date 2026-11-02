// name: Digital Rain
// Clean-room reimplementation from the prose specs docs/pattern-specs/matrix-green-waterfall-2d.md, matrix-rain.md — reimagined, not a replica.
// Falling code: one stream per pixel column on a panel, a handful of drops on
// a strip. Near-white heads leave tails that decay through fade() — the frame
// is the buffer — and tail cells re-light now and then like changing glyphs.
// Rain respawns each drop after a random pause; Cascade runs endless trains
// with a hash-picked spacing per column. A few ops per column per frame.

var MAXC = 128
var pos = array(MAXC), rate = array(MAXC), dHue = array(MAXC)

// 0 rain (drops with pauses) · 1 cascade (repeating trains per column)
var mode = 0
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1) }

// Rain hue in degrees; 120 is the classic green.
var hueDeg = 120
//# min=0 max=359 step=1 default=120
export function sliderHue(v) { hueDeg = mod(v, 360) }

// How far each stream's hue may stray from Hue, in degrees.
var spreadDeg = 0
//# min=0 max=90 step=1 default=0
export function sliderHueSpread(v) { spreadDeg = clamp(v, 0, 90) }

// Share of columns that are raining at any moment, percent.
var density = 70
//# min=10 max=100 step=1 default=70
export function sliderDensity(v) { density = clamp(v, 10, 100) }

// Fall speed in rows (strip: pixels) per second; each stream varies ±50 %.
var speed = 16
//# min=2 max=60 step=1 default=16
export function sliderSpeed(v) { speed = clamp(v, 2, 60) }

// Tail length in rows (strip: pixels), to 3 % brightness.
var trail = 14
//# min=3 max=60 step=1 default=14
export function sliderTrail(v) { trail = clamp(v, 3, 60) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var ncol = 0, len = 1, gw = 0, vw = 32, dt = 0, fadeK = 1, strip = 0

function respawn(c, first) {
  rate[c] = 0.5 + random(1)
  dHue[c] = (random(1) - 0.5) * spreadDeg / 360
  var gap = (strip ? min(len, 48) : len) * ((100 - density) / density * 1.6 + 0.05)
  pos[c] = first ? random(len + (strip ? 0 : gap)) - (strip ? 0 : gap) : -random(gap) - 1
}

function layout() {
  strip = !has2DMap()
  gw = gridWidth()
  var w = gw, h = gridHeight()
  if (strip) { w = clamp(round(pixelCount / 20), 2, MAXC); h = pixelCount }
  else if (gw == 0) { w = vw; h = vw }
  w = min(w, MAXC)
  if (w == ncol && h == len) return
  ncol = w
  len = h
  for (var c = 0; c < ncol; c++) respawn(c, 1)
}

// one cell of column c, row r
function dot(c, r) {
  if (strip) setPixel(r)
  else if (gw > 0) setPixel(gridIndex(c, r))
  else fillRect(c / vw, r / vw, (c + 0.9) / vw, (r + 0.9) / vw)
}

// paint a head that moved from row a to row b (b > a), plus a glyph flicker
function stroke(c, a, b, h) {
  var hr = floor(b)
  if (hr < 0) return
  hsv(h, 1, 1)
  for (var r = max(floor(a), 0); r < hr && r < len; r++) dot(c, r)
  if (hr < len) { hsv(h, 0.15, 1); dot(c, hr) }
  if (random(1) < dt * 7) {
    var d = 1 + floor(random(trail))
    if (hr - d >= 0 && hr - d < len) {
      hsv(h, 1, min(1, pow(0.03, d / trail) * (0.5 + random(1))))
      dot(c, hr - d)
    }
  }
}

export function beforeRender(delta) {
  dt = min(delta, 200) / 1000
  fadeK = pow(0.03, dt * speed / trail)
}

export function renderBulk() {
  layout()
  fade(fadeK)
  var hue = hueDeg / 360
  for (var c = 0; c < ncol; c++) {
    var step = speed * rate[c] * dt
    var p = pos[c] + step
    if (mode == 0) {
      stroke(c, pos[c], p, hue + dHue[c])
      if (p - trail > len) { respawn(c, 0); p = pos[c] }
    } else {
      // stateless trains: heads every `cyc` rows, spacing hashed per column
      var cyc = trail + 4 + len * (0.2 + hash(c * 3.7 + 1)) * 70 / density
      p = mod(p, cyc)
      var hc = hue + (hash(c * 5.3) - 0.5) * spreadDeg / 360
      for (var y = p; y < len; y += cyc) stroke(c, y - step, y, hc)
    }
    pos[c] = p
  }
}
