// name: Kaleidoscope
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/zoom-kaleidoscope.md — reimagined, not a replica (stained-glass source: from a prose description of "Perlin Kaleidoscope 2D", no source consulted).
// A live source folded into N mirrored wedges that spin. Source 0: domain-warped
// noise and three orbiting blobs quantised into stained-glass cells with dark
// leading; source 1: an XOR lattice of square and diamond rings that zooms in and
// out through ridged colour. The source is built once per frame on a small polar
// wedge canvas; each panel cell is one fold + one bilinear read through a packed
// (angle, radius) table, and colour is a per-frame palette painted with paintCanvas.
// Above 4,096 px a grid renders at <= 4,096 cells (64x64 on 128x128), expanded
// bilinearly by the engine (setRenderSize): the same picture, minus a full-panel paintCanvas.

var CAP = 64, NA = 16, NR = 32    // panel canvas cap; wedge canvas angles x radii
var RMAX = 1.5, FAN = 0.95        // radius span (half-width = 1); radians of source per wedge

// 0 stained glass (noise + blobs), 1 zooming XOR lattice
var source = 0
//# min=0 max=1 step=1 default=0
export function sliderSource(v) { source = clamp(floor(v), 0, 1); setPalette(source ? palZ : palG) }

var segs = 6
//# min=3 max=12 step=1 default=6
export function sliderSegments(v) { segs = clamp(floor(v), 3, 12) }

// whole-figure rotation, degrees per second (negative spins the other way)
var spinDeg = 20
//# min=-180 max=180 step=1 default=20
export function sliderSpinDegreesPerSecond(v) { spinDeg = clamp(v, -180, 180) }

// One zoom in-and-out of the source, in seconds; 0 holds the zoom still.
var zoomSec = 16
//# min=0 max=60 step=1 default=16
export function sliderZoomSeconds(v) { zoomSec = clamp(v, 0, 60) }

// how fast the source evolves under the mirrors, x realtime
var texSpeed = 1
//# min=0 max=3 step=0.1 default=1
export function sliderTextureSpeed(v) { texSpeed = clamp(v, 0, 3) }

// hue rotation of the whole palette, degrees per second
var hueDeg = 25
//# min=-180 max=180 step=1 default=25
export function sliderColorDriftDegreesPerSecond(v) { hueDeg = clamp(v, -180, 180) }

// colour cells in the stained glass
var bands = 4
//# min=2 max=5 step=1 default=4
export function sliderBands(v) { bands = clamp(floor(v), 2, 5) }

var contrast = 50
//# min=0 max=100 step=1 default=50
export function sliderContrastPercent(v) { contrast = clamp(v, 0, 100) }

// 0 vivid HSV, 1 OKLCH jewel tones (even lightness across hues)
var palMode = 0
//# min=0 max=1 step=1 default=0
export function sliderPalette(v) { palMode = clamp(floor(v), 0, 1) }

var tab = array(CAP * CAP), cv = array(CAP * CAP)
var pol = array(NA * NR), wx = array(NA * NR), wy = array(NA * NR)
var NG = 20, NZ = 40
var palG = array(NG * 4), palZ = array(NZ * 4)
setPalette(palG)
var c3 = array(3)
var CW = 0, CH = 0
var spin = 0, tex = 0, hueB = 0, zph = 0.5, zs = 1, ring = 0, dA = 0, dB = 0
var q = array(6), bx = array(3), by = array(3)
q[1] = 1.9; q[2] = 3.4; q[3] = 5.1; q[4] = 0.7; q[5] = 2.8

// the wedge's source coordinates, once: the fold spans a fixed fan of the source
for (var j = 0; j < NR; j++) for (var i = 0; i < NA; i++) {
  var wa = ((i + 0.5) / NA - 0.5) * FAN, rr = (j + 0.5) / NR * RMAX * 0.5
  wx[j * NA + i] = cos(wa) * rr
  wy[j * NA + i] = sin(wa) * rr
}

function buildTable() {
  var half = min(CW, CH) * 0.5
  for (var r = 0; r < CH; r++) {
    var dy = (r - CH * 0.5 + 0.5) / half
    for (var c = 0; c < CW; c++) {
      var dx = (c - CW * 0.5 + 0.5) / half
      tab[r * CW + c] = floor(mod(atan2(dy, dx) * 0.15915494, 1) * 4096) + min(hypot(dx, dy) / RMAX, 0.999)
    }
  }
}

// OKLCH (hue in turns) -> linear RGB in c3, clipped
function oklchRGB(L, C, h) {
  var a = C * cos(h * PI2), b = C * sin(h * PI2)
  var l = L + 0.3963 * a + 0.2158 * b, m = L - 0.1056 * a - 0.0639 * b, s = L - 0.0895 * a - 1.2915 * b
  l = l * l * l; m = m * m * m; s = s * s * s
  c3[0] = saturate(4.0767 * l - 3.3077 * m + 0.2310 * s)
  c3[1] = saturate(-1.2684 * l + 2.6098 * m - 0.3413 * s)
  c3[2] = saturate(-0.0042 * l - 0.7034 * m + 1.7076 * s)
}

function colour(h, v) {
  if (palMode) oklchRGB(0.2 + 0.62 * v, 0.14 * min(1, v * 3), h)
  else hsv2rgb(h, 1, v, c3)
}

function put(p, n, pos, h, v) {
  colour(h, v)
  p[n * 4] = pos; p[n * 4 + 1] = c3[0]; p[n * 4 + 2] = c3[1]; p[n * 4 + 3] = c3[2]
}

function contr(v) {
  var c = contrast * 0.01
  return smoothstep(c * c * 0.5, 1 - c * c * 0.45, v)
}

// stained glass: band colours over the field value, a dark lead line at each boundary
function glassPalette() {
  var q0 = mod(hueB * 1.5, 1) * bands, LW = 0.035, n = 0
  put(palG, n++, 0, floor(q0) * 0.29 + hueB, contr(0.15))
  for (var m = floor(q0) + 1; m < q0 + bands; m++) {
    var b = clamp((m - q0) / bands, 0.002, 0.998), lo = max(b - LW, 0.001), hi = min(b + LW, 0.999)
    put(palG, n++, lo, (m - 1) * 0.29 + hueB, contr(0.15 + 0.85 * lo))
    put(palG, n++, b, 0, 0)
    put(palG, n++, hi, m * 0.29 + hueB, contr(0.15 + 0.85 * hi))
  }
  while (n < NG) put(palG, n++, 1, floor(q0 + bands) * 0.29 + hueB, contr(0.9))
}

// zoom lattice: hue swings with the field, brightness is ridged noise of it
function zoomPalette() {
  for (var n = 0; n < NZ; n++) {
    var f = n / (NZ - 1)
    var v = contr(perlinRidge(f * 2, dA, dB, 2, 0.8, 1, 3) * 0.85)
    put(palZ, n, f, triangle(f * 0.5 + hueB), v * sqrt(v))
  }
}

var rs = 0                          // 1 once a big grid renders at <= 4,096 cells
function bigGrid() {                // above 4,096 px: render <= 4,096 cells, engine expands bilinearly
  if (rs || pixelCount <= 4096 || !has2DMap() || gridWidth() < 1 || gridHeight() < 2) return
  var gw = gridWidth(), gh = gridHeight(), w = min(gw, floor(64 * sqrt(gw / gh) + 0.001))
  setRenderSize(w, min(gh, floor(4096 / w)), 1)   // gridWidth/gridHeight/pixelCount follow at once
  rs = 1
}

export function beforeRender(delta) {
  bigGrid()
  var dt = min(delta, 100) * 0.001, st = dt * texSpeed
  var gw = gridWidth(), gh = gridHeight()
  var w = gw > 0 ? min(gw, CAP) : 32, h = gh > 0 ? min(gh, CAP) : 32
  if (w != CW || h != CH) { CW = w; CH = h; buildTable() }

  spin = mod(spin + dt * spinDeg / 360, 1)
  hueB = mod(hueB + dt * hueDeg / 360, 1)
  tex = mod(tex + st * 0.18, 512)
  if (zoomSec > 0) zph = mod(zph + dt / zoomSec, 1)
  ring = mod(ring + st * 0.15, 64)
  dA = mod(dA + st * 0.38, 256); dB = mod(dB + st * 0.6, 256)

  if (source == 0) {
    zs = pow(2, 1.2 * triangle(zph) - 0.6)
    // six incommensurate orbit phases, so the three blobs never fall into step
    for (var i = 0; i < 6; i++) q[i] = mod(q[i] + st * (2.1 + i * 0.83), PI2)
    for (i = 0; i < 3; i++) {
      var rr = 0.16 + 0.4 * (0.5 + 0.5 * sin(q[i * 2])), aa = 0.44 * sin(q[i * 2 + 1])
      bx[i] = cos(aa) * rr; by[i] = sin(aa) * rr
    }
    for (var j = 0; j < NR; j++) {
      var rad = (j + 0.5) / NR * RMAX * 0.5, base = 0.52 - rad * 0.6
      for (var c = 0; c < NA; c++) {
        i = j * NA + c
        var x = wx[i], y = wy[i], sx = x * zs, sy = y * zs
        var n1 = simplex3(sx * 2.4 + tex * 0.7, sy * 2.4, tex * 0.45, 17)
        var n2 = simplex3(sx * 3.8 + n1 * 0.5, sy * 3.8 - n1 * 0.5, tex * 0.8 + 60, 23)
        var e = x - bx[0], f = y - by[0], g = 1 / (1 + 32 * (e * e + f * f)), glo = g * g
        e = x - bx[1]; f = y - by[1]; g = 1 / (1 + 40 * (e * e + f * f)); glo += g * g
        e = x - bx[2]; f = y - by[2]; g = 1 / (1 + 26 * (e * e + f * f)); glo += g * g
        // a little of the angle, so every wedge is faceted, not just banded
        pol[i] = saturate(base + n1 * 0.5 + n2 * 0.25 + glo * 0.3 + c * 0.007)
      }
    }
    glassPalette()
  } else {
    zs = 1.4 * pow(2, 3 * triangle(zph) - 1.5)
    for (i = 0; i < NA * NR; i++) {
      x = abs(wx[i] * zs); y = abs(wy[i] * zs)
      pol[i] = triangle((max(x, y) - ring) ^ (x + y + 0.07 - ring))
    }
    zoomPalette()
  }
}

export function renderBulk() {
  var A = segs / 4096, S = spin * segs, n = CW * CH
  for (var i = 0; i < n; i++) {
    var p = tab[i], ai = floor(p)
    var u = ai * A + S
    cv[i] = canvasGet(pol, NA, abs(u - floor(u) - 0.5) * 2, p - ai)
  }
  paintCanvas(cv, CW, CH, 1, gridWidth() > CW || gridHeight() > CH ? 1 : 0)   // bilinear only when upscaling
}
