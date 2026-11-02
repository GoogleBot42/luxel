// name: Interference
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/2d-sinc-theta-theta.md — reimagined, not a replica.
// Point sources orbit the panel and their expanding ripples interfere. RGB sinc:
// each source rings in its own evenly spaced primary under a 1/r sinc envelope,
// and the rings mix additively into secondaries and white-hot crossings. Palette
// rings: the summed field through one drifting OKLCH ramp (a 32-stop setPalette
// rebuilt per frame, so the per-pixel cost is one paint). Per pixel: one hypot
// and one wave per source; a strip shows a line cut through the middle.
// Above 4,096 px a grid renders at <= 4,096 cells (64x64 on 128x128), expanded
// bilinearly by the engine (setRenderSize): a quarter of the work on a 128x128 chain.

var style = 0
var nSrc = 3
var rings = 6
var orbit = 30

// 0 RGB sinc · 1 palette rings
//# min=0 max=1 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 1) }
//# min=2 max=5 step=1 default=3
export function sliderSources(v) { nSrc = clamp(floor(v), 2, 5) }
// Ring crests across the panel width.
//# min=2 max=12 step=0.5 default=6
export function sliderRingsPerPanel(v) { rings = clamp(v, 2, 12) }
// Seconds for the slowest source to lap its orbit; ripples expand faster as it shortens.
//# min=5 max=120 step=1 default=30
export function sliderOrbitS(v) { orbit = clamp(v, 5, 120) }

var sx = array(5), sy = array(5), ang = array(5)
var cr = array(5), cg = array(5), cb = array(5)
var rgbTmp = array(3)
var pal = array(128)
var rp = 0, hue = 0.55, inv = 1 / 3
var x0, y0, r0, g0, b0, p0, x1, y1, r1, g1, b1, p1, x2, y2, r2, g2, b2, p2
var x3, y3, r3, g3, b3, p3, x4, y4, r4, g4, b4, p4

function okRGB(L, C, h, o) {   // OKLCH -> gamma sRGB, Ottosson's OKLab matrices
  var a = C * cos(h * PI2), b = C * sin(h * PI2)
  var l = L + 0.3963 * a + 0.2158 * b, m = L - 0.1056 * a - 0.0639 * b, s = L - 0.0895 * a - 1.2915 * b
  l = l * l * l; m = m * m * m; s = s * s * s
  o[0] = pow(clamp(4.0767 * l - 3.3077 * m + 0.2310 * s, 0, 1), 0.4167)
  o[1] = pow(clamp(-1.2684 * l + 2.6098 * m - 0.3413 * s, 0, 1), 0.4167)
  o[2] = pow(clamp(-0.0042 * l - 0.7034 * m + 1.7076 * s, 0, 1), 0.4167)
}

setPalette(pal)

var rs = 0                          // 1 once a big grid renders at <= 4,096 cells
function bigGrid() {                // above 4,096 px: render <= 4,096 cells, engine expands bilinearly
  if (rs || pixelCount <= 4096 || !has2DMap() || gridWidth() < 1 || gridHeight() < 2) return
  var gw = gridWidth(), gh = gridHeight(), w = min(gw, floor(64 * sqrt(gw / gh) + 0.001))
  setRenderSize(w, min(gh, floor(4096 / w)), 1)   // gridWidth/gridHeight/pixelCount follow at once
  rs = 1
}

export function beforeRender(delta) {
  bigGrid()
  var dt = min(delta, 100) / 1000
  rp = mod(rp + dt * 18 / orbit, 1)
  hue = mod(hue + dt / 90, 1)
  for (var i = 0; i < nSrc; i++) {
    var dir = i % 2 ? -1 : 1
    ang[i] = mod(ang[i] + dir * dt * (1 + 0.37 * i) / orbit, 1)
    var r = 0.24 + 0.09 * i
    sx[i] = 0.5 + r * cos(ang[i] * PI2 + i * 1.3)
    sy[i] = 0.5 + r * 0.85 * sin(ang[i] * PI2 * (1 + i % 2) + i * 2.1)
    hsv2rgb(i / nSrc, 1, 1, rgbTmp)
    cr[i] = rgbTmp[0]; cg[i] = rgbTmp[1]; cb[i] = rgbTmp[2]
  }
  // unpacked to scalars: the per-pixel path reads no arrays
  x0 = sx[0]; y0 = sy[0]; r0 = cr[0]; g0 = cg[0]; b0 = cb[0]; p0 = rp
  x1 = sx[1]; y1 = sy[1]; r1 = cr[1]; g1 = cg[1]; b1 = cb[1]; p1 = rp - 0.31
  x2 = sx[2]; y2 = sy[2]; r2 = cr[2]; g2 = cg[2]; b2 = cb[2]; p2 = rp - 0.62
  x3 = sx[3]; y3 = sy[3]; r3 = cr[3]; g3 = cg[3]; b3 = cb[3]; p3 = rp - 0.93
  x4 = sx[4]; y4 = sy[4]; r4 = cr[4]; g4 = cg[4]; b4 = cb[4]; p4 = rp - 1.24
  inv = 1 / nSrc
  if (style == 1) {
    for (var k = 0; k < 32; k++) {
      var p = k / 31
      okRGB(0.06 + 0.86 * p, 0.09 + 0.08 * p * (1 - p) * 4, hue + 0.22 * p, rgbTmp)
      pal[k * 4] = p; pal[k * 4 + 1] = rgbTmp[0]; pal[k * 4 + 2] = rgbTmp[1]; pal[k * 4 + 3] = rgbTmp[2]
    }
  }
}

// sinc ring of one source: crest^2 under a 1/r envelope, guarded at the core
function ring(d, p) { var w = wave(d * rings - p); return w * w / (d * 3 + 0.18) }

export function render2D(index, x, y) {
  var d, w, r, g, b = 0
  if (style == 0) {   // sinc rings: crest^2 under a guarded 1/r; sources take R, G, B in turn
    d = hypot(x - x0, y - y0); w = wave(d * rings - p0); r = w * w / (d * 3 + 0.18)
    d = hypot(x - x1, y - y1); w = wave(d * rings - p1); g = w * w / (d * 3 + 0.18)
    if (nSrc > 2) { d = hypot(x - x2, y - y2); w = wave(d * rings - p2); b = w * w / (d * 3 + 0.18) }
    if (nSrc > 3) { d = hypot(x - x3, y - y3); w = wave(d * rings - p3); r += w * w / (d * 3 + 0.18) }
    if (nSrc > 4) { d = hypot(x - x4, y - y4); w = wave(d * rings - p4); g += w * w / (d * 3 + 0.18) }
    rgb(r, g, b)
  } else {
    r = wave(hypot(x - x0, y - y0) * rings - rp) + wave(hypot(x - x1, y - y1) * rings - rp)
    if (nSrc > 2) r += wave(hypot(x - x2, y - y2) * rings - rp)
    if (nSrc > 3) r += wave(hypot(x - x3, y - y3) * rings - rp)
    if (nSrc > 4) r += wave(hypot(x - x4, y - y4) * rings - rp)
    r = r * inv
    paint(r * r * (3 - 2 * r), 1)
  }
}

export function render(index) { render2D(index, index / pixelCount, 0.5) }
