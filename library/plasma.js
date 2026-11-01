// name: Plasma
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/sinpulse-3d.md, matrix-2d-honeycomb.md, matrix-2-tone-pulse.md, sunset.md — reimagined, not a replica.
// The classic sine plasma — column, row and diagonal waves drifting against each
// other while the blob scale breathes — in four looks: rainbow blobs, neon contour
// lines, a two-tone complementary pair split by black seams, and a soft ambient
// wedge of hue that slowly rotates round the wheel. On a panel the three wave terms
// are per-column/row/diagonal tables summed into a canvas (one fillCanvas); 3D maps
// and strips evaluate the same field per pixel.

var style = 0
var blobs = 3
var drift = 3
var breath = 10
var spread = 1
var tone = 0.22

// 0 rainbow · 1 neon contours · 2 two-tone · 3 ambient wedge
//# min=0 max=3 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 3) }
// Blob cycles across the panel at the fine end of the breath (the coarse end is a third).
//# min=1 max=6 step=0.5 default=3
export function sliderBlobs(v) { blobs = clamp(v, 1, 6) }
// Seconds for the faster drift phase to come round.
//# min=0.5 max=30 step=0.1 default=3
export function sliderDriftS(v) { drift = clamp(v, 0.5, 30) }
//# min=1 max=60 step=0.5 default=10
export function sliderBreathS(v) { breath = clamp(v, 1, 60) }
// Hue range the field spans; the ambient wedge shows a quarter of it, two-tone ignores it.
//# min=30 max=360 step=5 default=360
export function sliderHueSpreadDeg(v) { spread = clamp(v, 30, 360) / 360 }
// The two-tone style's first colour (its partner is the complement); V is ignored.
export function hsvPickerTone(h, s, v) { tone = h }

var CAP = 2304
var hA = array(CAP), vA = array(CAP)
var colS = array(48), rowC = array(48), diag = array(96)
var p1 = 0, p2 = 0, p3 = 0, bp = 0, hueT = 0, shim = 0, Z = 6
var cw = 32, ch = 32, gH = 0, gV = 0

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  p1 = mod(p1 + dt * PI2 / drift, PI2)
  p2 = mod(p2 + dt * PI2 / (drift * 2.15), PI2)
  p3 = mod(p3 - dt * PI2 / (drift * 3.4), PI2)
  bp = mod(bp + dt / breath, 1)
  hueT = mod(hueT + dt / (style == 3 ? 60 : 24), 1)
  shim = mod(shim + dt / (drift * 1.6), 1)
  Z = PI2 * blobs * (0.33 + 0.67 * (0.5 - 0.5 * cos(bp * PI2)))
}

// field sum (-3..3) -> brush colour in gH (hue) / gV (value)
function colour(f) {
  var e = clamp((f + 2.2) / 4.4, 0, 1)
  if (style == 0) {
    gH = hueT + e * spread
    gV = 0.05 + 0.95 * e * e * (3 - 2 * e)
  } else if (style == 1) {
    var q = triangle(e * 2.5 + shim)
    q = q * q
    gH = hueT + triangle(e) * spread
    gV = 0.04 + 0.96 * q * q
  } else if (style == 2) {
    var t = 1 - triangle(e * 2)
    gH = tone + (e < 0.5 ? 0 : 0.5) + (e - 0.5) * 0.08
    gV = 1 - t * t * t
  } else {
    gH = hueT + e * spread * 0.25
    gV = 0.3 + 0.7 * e * e
  }
}

export function renderBulk() {
  var i, c, r, x, y
  var gw = gridWidth(), gh = gridHeight()
  if (gw > 0 && has2DMap()) {
    var k = 1
    while (gw / k > 48 || gh / k > 48) k *= 2
    cw = ceil(gw / k); ch = ceil(gh / k)
    var sc = 1 / max(cw, ch)
    for (c = 0; c < cw; c++) colS[c] = sin((c + 0.5) / cw * Z + p1)
    for (r = 0; r < ch; r++) rowC[r] = cos((r + 0.5) / ch * Z + p2)
    for (i = 0; i < cw + ch - 1; i++) diag[i] = sin((i + 1) * sc * Z * 0.7 + p3)
    i = 0
    for (r = 0; r < ch; r++) {
      var rc = rowC[r]
      for (c = 0; c < cw; c++) {
        colour(colS[c] + rc + diag[c + r])
        hA[i] = gH; vA[i] = gV; i++
      }
    }
    fillCanvas(hA, 1, vA, cw, ch)
    setBlur(k > 1 ? 0.5 : 0, k / 2)
    return
  }
  setBlur(0)
  var is3 = has3DMap(), is2 = has2DMap()
  for (i = 0; i < pixelCount; i++) {
    if (is3 || is2) {
      x = pixelCoord(i, 0); y = pixelCoord(i, 1)
      colour(sin(x * Z + p1) + cos(y * Z + p2) + sin((x + y + pixelCoord(i, 2)) * Z * 0.7 + p3))
    } else {   // strip: three waves of different lengths along the run
      x = i / pixelCount
      colour(sin(x * Z + p1) + cos(x * Z * 0.62 + p2) + sin(x * Z * 1.37 + p3))
    }
    hsv(gH, 1, gV)
    setPixel(i)
  }
}

// Mapless strips stay strips: this export suppresses the default grid.
export function render(index) { hsv(0, 0, 0) }
