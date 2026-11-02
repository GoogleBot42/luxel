// name: Pool Caustics
// tags: both, nature
// Curated original (hand-written showcase of the Luxel language/builtins).
// Sunlight through a rippling pool: a web of sharp bright lines dancing over
// tiles with faint grout. Two simplex layers drift in different directions;
// the light is |a·b|, which is zero along BOTH layers' zero lines, so the
// lines cross into a net and swell into knots where they meet. One noise row
// per canvas row, one palette (sharpness lives in its stops), one paintCanvas.
// On a strip it is a sunlit shallow stream, glints flowing down its length.

var CAP = 64                   // canvas cap per side (128 px panels run at 2x)
var vArr = array(4096)
var rowA = array(1024), rowB = array(1024)
var gcol = array(64)           // 0.5 on a grout column, else 0
var pal = array(48)
var z = 0, oa = 0, ob = 0

var cellPx = 20
// Typical width of one caustic cell, in pixels.
//# min=6 max=60 step=1 default=20
export function sliderCellSizePx(v) { cellPx = clamp(v, 6, 60) }

var speed = 8
// Drift of the light pattern, in pixels per second.
//# min=0 max=40 step=1 default=8
export function sliderSpeedPxPerS(v) { speed = clamp(v, 0, 40) }

var sharp = 4
//# min=2 max=8 step=1 default=4
export function sliderSharpness(v) { sharp = clamp(floor(v), 2, 8); build() }

var grout = 35
// How much darker the grout lines between floor tiles are, in percent.
//# min=0 max=100 step=1 default=35
export function sliderGroutPct(v) { grout = clamp(v, 0, 100); build() }

var look = 0
// 0 pool · 1 lagoon · 2 lava
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { look = clamp(floor(v), 0, 2); build() }

// A mapless strip stays a strip (this export suppresses the default grid).
export function render(index) { hsv(0, 0, 0) }

function stop(k, pos, r, g, b) {
  pal[k * 4] = pos
  pal[k * 4 + 1] = r
  pal[k * 4 + 2] = g
  pal[k * 4 + 3] = b
}

// Palette position is |a·b|: 0 on a light line, ~0.1+ on open floor. The
// lower half is the lit tile, the upper half the same ramp on grout.
function build() {
  var w = 4 / sharp, gk = 1 - grout / 100
  var p0, p1, p2, p3
  for (var half = 0; half < 2; half++) {
    var o = half * 0.5, k = half ? gk : 1, s = half * 6
    if (look == 0) {
      stop(s, o, k, k, k)
      stop(s + 1, o + 0.012 * w, 0.45 * k, 0.95 * k, k)
      stop(s + 2, o + 0.035 * w, 0.08 * k, 0.6 * k, 0.78 * k)
      stop(s + 3, o + 0.09 * w, 0.03 * k, 0.36 * k, 0.52 * k)
      stop(s + 4, o + 0.3, 0.02 * k, 0.27 * k, 0.43 * k)
      stop(s + 5, o + 0.499, 0.02 * k, 0.24 * k, 0.4 * k)
    } else if (look == 1) {
      stop(s, o, k, k, 0.8 * k)
      stop(s + 1, o + 0.012 * w, 0.75 * k, 0.95 * k, 0.55 * k)
      stop(s + 2, o + 0.035 * w, 0.2 * k, 0.7 * k, 0.5 * k)
      stop(s + 3, o + 0.09 * w, 0.06 * k, 0.42 * k, 0.38 * k)
      stop(s + 4, o + 0.3, 0.05 * k, 0.32 * k, 0.32 * k)
      stop(s + 5, o + 0.499, 0.04 * k, 0.28 * k, 0.3 * k)
    } else {
      stop(s, o, k, k, 0.6 * k)
      stop(s + 1, o + 0.012 * w, k, 0.7 * k, 0.12 * k)
      stop(s + 2, o + 0.035 * w, 0.85 * k, 0.25 * k, 0.02 * k)
      stop(s + 3, o + 0.09 * w, 0.42 * k, 0.06 * k, 0.02 * k)
      stop(s + 4, o + 0.3, 0.3 * k, 0.04 * k, 0.03 * k)
      stop(s + 5, o + 0.499, 0.26 * k, 0.03 * k, 0.04 * k)
    }
  }
  setPalette(pal)
}
build()

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  var u = speed * dt / cellPx              // cells travelled this frame
  z = mod(z + u * 0.45 + dt * 0.04, 256)
  oa = mod(oa + u, 256)
  ob = mod(ob + u * 0.8, 256)
}

function strip() {
  var n = min(pixelCount, 1024), k = pixelCount / n, s = k / cellPx
  fillNoise3D(rowA, n, 1, s, 1, -oa, 0.3, z, 3)
  fillNoise3D(rowB, n, 1, s * 1.37, 1, -ob * 1.6, 5.1, z * 1.2, 11)
  for (var c = 0; c < n; c++) {
    paint(min(abs(rowA[c] * rowB[c]) * 0.5, 0.49))
    fillRange(floor(c * k), floor((c + 1) * k))
  }
}

export function renderBulk() {
  if (!has2DMap()) { strip(); return }
  var gw = gridWidth(), gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  var k = 1
  while (ceil(gw / k) > CAP || ceil(gw / k) * ceil(gh / k) > 4096) k++
  var cw = ceil(gw / k), ch = ceil(gh / k), s = k / cellPx
  var tile = max(round(8 / k), 3)                // 8 px floor tiles
  for (var c = 0; c < cw; c++) gcol[c] = c % tile == tile - 1 ? 0.5 : 0
  var i = 0
  for (var r = 0; r < ch; r++) {
    fillNoise3D(rowA, cw, 1, s, 1, oa, r * s, z, 3)
    fillNoise3D(rowB, cw, 1, s * 1.21, 1, 9.3, r * s * 1.21 - ob, z * 1.3, 11)
    if (r % tile == tile - 1) {
      for (c = 0; c < cw; c++) vArr[i++] = min(abs(rowA[c] * rowB[c]), 0.49) + 0.5
    } else {
      for (c = 0; c < cw; c++) vArr[i++] = min(abs(rowA[c] * rowB[c]), 0.49) + gcol[c]
    }
  }
  paintCanvas(vArr, cw, ch, 1, k > 1 ? 1 : 0)   // bilinear upscale
}
