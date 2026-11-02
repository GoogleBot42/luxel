// name: Portal
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/portal.md, novas.md, scrolls.md — reimagined, not a replica.
// Energy pulses well up and spread; their summed field runs through a banded
// gradient, so every pulse wears nested contour rings. On a panel the pulses are
// rings racing out of a glowing core: the picture depends on radius only, so a
// static radius canvas is painted through a palette rebuilt each frame from the
// radial field (one paintCanvas, no per-pixel code). On a strip they bloom at
// random spots into one capped field array.
// Above 4,096 px a grid renders at <= 4,096 cells (64x64 on 128x128), expanded
// bilinearly by the engine (setRenderSize): the same picture, minus a full-panel paintCanvas.

var POOL = 8
var born = array(POOL), amp = array(POOL), at = array(POOL), alive = array(POOL)
var rc = array(POOL), hw = array(POOL), env = array(POOL)

var interval = 1.6                // seconds between pulses
//# min=0.3 max=8 step=0.1 default=1.6
export function sliderPulseIntervalSeconds(v) { interval = clamp(v, 0.3, 8) }

var life = 4.5                    // seconds a pulse lives
//# min=1 max=12 step=0.5 default=4.5
export function sliderLifetimeSeconds(v) { life = clamp(v, 1, 12) }

var spread = 100                  // pulse width at the end of its life, % of the display
//# min=10 max=200 step=1 default=100
export function sliderSpreadPercent(v) { spread = clamp(v, 10, 200) }

// 0 fire portal | 1 embers | 2 bioluminescent | 3 void
var palette = 0
//# min=0 max=3 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 3) }

// Gradients over the summed field, [pos, r, g, b, ...]. Thin warm contour bands
// low, a violet core high; white-hot to crimson with no brown; sea-glow peaking at
// the top; cold contour lines into a blue-white core.
var G0 = [0, 0, 0, 0, 0.07, 1, 0, 0, 0.1, 0.18, 0.01, 0, 0.15, 1, 0.12, 0, 0.18, 0.18, 0.02, 0,
  0.23, 1, 0.3, 0, 0.26, 0.15, 0.03, 0, 0.31, 1, 0.5, 0.03, 0.37, 0.05, 0, 0.04, 0.5, 0.18, 0, 0.4,
  0.75, 0.6, 0.1, 1, 1, 1, 0.75, 1]
var G1 = [0, 0, 0, 0, 0.15, 0.22, 0, 0.03, 0.35, 0.8, 0.01, 0.1, 0.55, 1, 0.15, 0.02,
  0.75, 1, 0.5, 0.05, 0.9, 1, 0.85, 0.45, 1, 1, 1, 0.9]
var G2 = [0, 0, 0, 0, 0.15, 0, 0.03, 0.07, 0.4, 0, 0.25, 0.32, 0.65, 0, 0.75, 0.5,
  0.85, 0.3, 1, 0.75, 1, 0.8, 1, 0.95]
var G3 = [0, 0, 0, 0, 0.08, 0, 0.3, 1, 0.11, 0, 0, 0.06, 0.18, 0.4, 0, 1, 0.21, 0, 0, 0.06,
  0.28, 0, 0.7, 1, 0.31, 0, 0, 0.04, 0.45, 0, 0, 0.02, 0.7, 0.12, 0, 0.5, 1, 0.55, 0.95, 1]

// the chosen gradient as a 128-entry LUT, for the radial palette
var LN = 128
var LR = array(LN), LG = array(LN), LB = array(LN)
var lutFor = -1, grad = G0
function buildLut() {
  grad = palette == 0 ? G0 : palette == 1 ? G1 : palette == 2 ? G2 : G3
  var k = 4
  for (var i = 0; i < LN; i++) {
    var v = i / (LN - 1)
    while (k < grad.length - 4 && grad[k] < v) k += 4
    var p0 = grad[k - 4], t = clamp((v - p0) / max(grad[k] - p0, 0.001), 0, 1)
    LR[i] = mix(grad[k - 3], grad[k + 1], t)
    LG[i] = mix(grad[k - 2], grad[k + 2], t)
    LB[i] = mix(grad[k - 1], grad[k + 3], t)
  }
  lutFor = palette
}

// panel: radius canvas (0 at the centre, ~1 in the corners) + radial palette
var CAP = 64, NR = 48
var rad = array(CAP * CAP)
var RP = array(NR * 4)
for (var k = 0; k < NR; k++) RP[k * 4] = k / (NR - 1)
var CW = 0, CH = 0

function buildRadius(w, h) {
  CW = w; CH = h
  var ax = w / h, maxR = hypot(0.5 * ax, 0.5)
  for (var r = 0; r < h; r++) {
    var dy = (r - (h - 1) / 2) / max(h - 1, 1)
    for (var c = 0; c < w; c++) {
      var dx = (c - (w - 1) / 2) / max(w - 1, 1) * ax
      var rr = hypot(dx, dy) / maxR
      // a three-arm spiral warp: as a ring expands its lobes turn, so it swirls
      rr += 0.035 * min(rr * 4, 1) * sin(atan2(dy, dx) * 3 + rr * 14)
      rad[r * w + c] = clamp(rr, 0, 0.999)
    }
  }
}

// strip: one field array, capped
var FN = 1024
var F = array(FN)

var clock = 0, nextAt = 0, installed = -1

// start mid-flow: three pulses already under way, so frame one is never dark
for (var i = 0; i < 3; i++) {
  alive[i] = 1; born[i] = -i * interval; amp[i] = 0.8 + random(0.35)
  at[i] = 0.5 + (random(1) + random(1) - 1) * 0.42
}

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

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
  clock += dt
  if (clock > 3600) {             // keep the clock small
    clock -= 3600; nextAt -= 3600
    for (var i = 0; i < POOL; i++) born[i] -= 3600
  }
  if (palette != lutFor) buildLut()
  if (clock >= nextAt) {
    for (var i = 0; i < POOL; i++) {
      if (!alive[i]) {
        alive[i] = 1
        born[i] = clock
        amp[i] = 0.8 + random(0.35)
        at[i] = 0.5 + (random(1) + random(1) - 1) * 0.42
        break
      }
    }
    nextAt = clock + interval * (0.75 + random(0.5))
  }
  var s = spread / 200
  for (var i = 0; i < POOL; i++) {
    if (!alive[i]) continue
    var u = (clock - born[i]) / life
    if (u >= 1) { alive[i] = 0; continue }
    env[i] = amp[i] * (1 - u) * min(1, u * 12)
    rc[i] = u * 1.1               // panel: ring radius, out past the corners
    hw[i] = 0.05 + s * u          // strip half-width, display units
  }
}

function panel() {
  var w = gridWidth() > 0 ? min(gridWidth(), CAP) : 32
  var h = gridHeight() > 0 ? min(gridHeight(), CAP) : 32
  if (w != CW || h != CH) buildRadius(w, h)
  if (installed != 9) { setPalette(RP); installed = 9 }
  var core = 0.42 + 0.1 * wave(time(0.04))
  for (var k = 0; k < NR; k++) {
    var r = k / (NR - 1)
    var f = core * max(0, 1 - r * 5)
    for (var i = 0; i < POOL; i++) {
      if (!alive[i]) continue
      var d = abs(r - rc[i]), w = hw[i] * 0.45   // rings stay crisper than blooms
      if (d < w) f += env[i] * cos(d / w * PI / 2)
    }
    var li = floor(clamp(f, 0, 1) * (LN - 1) + 0.5)
    RP[k * 4 + 1] = LR[li]; RP[k * 4 + 2] = LG[li]; RP[k * 4 + 3] = LB[li]
  }
  paintCanvas(rad, CW, CH, 1, gridWidth() > CW || gridHeight() > CH ? 1 : 0)   // bilinear only when upscaling
}

function strip() {
  if (installed != palette) { setPalette(grad); installed = palette }
  var n = min(pixelCount, FN)
  feedback(F, 0)
  for (var i = 0; i < POOL; i++) {
    if (!alive[i]) continue
    var c = at[i] * n, h = hw[i] * n
    var lo = max(0, ceil(c - h)), hi = min(n - 1, floor(c + h))
    for (var p = lo; p <= hi; p++) F[p] += env[i] * cos((p - c) / h * PI / 2)
  }
  var k = n / pixelCount
  for (var p = 0; p < pixelCount; p++) {
    paint(min(F[floor(p * k)], 1))
    setPixel(p)
  }
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
