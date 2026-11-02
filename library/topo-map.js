// name: Topo Map
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/perlin-simplex-noise-2d.md, perlin-simplex-noise-1d.md — reimagined, not a replica.
// Also folds docs/pattern-specs/patternflow/contour-blobs.md (Patternflow preset, CC BY-SA 4.0 — prose-described idea only, no code).
// A living hiking map: two octaves of simplex terrain slowly reshape while
// hypsometric tints (sea, shallows, beach, green, tan, snow) and crisp contour
// lines follow them; or glowing neon contours on black; or islands on a black sea.
// The whole look is ONE palette over height, rebuilt only when a control moves:
// the frame is two native fillNoise3D canvases, an arrayMaxAbs normalise and one
// paintCanvas — no per-pixel code. On a strip it is a slice through the same land.
// Above 4,096 px a grid renders at <= 4,096 cells (64x64 on 128x128), expanded
// bilinearly by the engine (setRenderSize): the same picture, minus a full-panel paintCanvas.

var CAP = 64
var A = array(CAP * CAP), B = array(CAP * CAP)
var NS = 120                        // palette stops
var pal = array(NS * 4), keys = array(NS)

var style = 0
//# min=0 max=2 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 2); dirty = 1 }
var contours = 8
//# min=3 max=20 step=1 default=8
export function sliderContours(v) { contours = clamp(floor(v), 3, 20); dirty = 1 }
var flowS = 60                      // seconds for the terrain to reshape
//# min=5 max=300 step=1 default=60
export function sliderFlowS(v) { flowS = clamp(v, 5, 300) }
var hills = 1.5                       // hills across the panel (or strip)
//# min=0.5 max=6 step=0.5 default=1.5
export function sliderScaleHillsAcross(v) { hills = clamp(v, 0.5, 6) }
var seaPct = 0                      // coastline height, % of the relief
//# min=-50 max=50 step=1 default=0
export function sliderSeaLevelPct(v) { seaPct = clamp(v, -50, 50); dirty = 1 }

export function render(index) { hsv(0, 0, 0) }   // keeps a strip mapless

// --- the palette: colour as a function of signed height hs in -1..1 ---
var c3 = array(3)
var SEA = 0, DLT = 0.1, LW = 0.02
function ramp(hs) {                 // base tint, written to c3
  var L = SEA
  if (style == 1) { c3[0] = 0; c3[1] = 0; c3[2] = 0; return }
  if (hs < L) {
    if (style == 2) { c3[0] = 0; c3[1] = 0; c3[2] = 0; return }
    var d = (L - hs) / (L + 1)      // 0 at the coast, 1 in the deepest trench
    if (d < 0.25) mixColors(0.2, 0.7, 0.8, 0.04, 0.3, 0.75, d / 0.25, c3)
    else mixColors(0.04, 0.3, 0.75, 0.02, 0.08, 0.35, (d - 0.25) / 0.75, c3)
    return
  }
  var u = (hs - L) / (1 - L)        // 0 at the coast, 1 on the highest peak
  if (style == 2) {                 // each band: dim at its foot, brighter up to the next line
    var b = frac((hs - L) / DLT)
    hsv2rgb(0.3 + u * 0.32, 0.9, 0.1 + 0.4 * b * b, c3)
    return
  }
  if (u < 0.05) mixColors(0.9, 0.78, 0.4, 0.25, 0.7, 0.12, u / 0.05, c3)
  else if (u < 0.5) mixColors(0.25, 0.7, 0.12, 0.1, 0.42, 0.08, (u - 0.05) / 0.45, c3)
  else if (u < 0.75) mixColors(0.1, 0.42, 0.08, 0.55, 0.42, 0.15, (u - 0.5) / 0.25, c3)
  else if (u < 0.9) mixColors(0.55, 0.42, 0.15, 0.5, 0.35, 0.3, (u - 0.75) / 0.15, c3)
  else mixColors(0.5, 0.35, 0.3, 1, 1, 1, (u - 0.9) / 0.1, c3)
}
function lineCol(hs) {              // contour colour at hs, written to c3
  var L = SEA
  if (style == 1) { hsv2rgb(0.62 - (hs + 1) * 0.45, 0.85, 1, c3) }
  else if (style == 2) { if (hs < L) return; hsv2rgb(0.3 + (hs - L) / (1 - L) * 0.32, 0.45, 1, c3) }
  else if (hs < L) { c3[0] = c3[0] * 0.6 + 0.12; c3[1] = c3[1] * 0.6 + 0.2; c3[2] = c3[2] * 0.6 + 0.3 }
  else { c3[0] = c3[0] * 0.35; c3[1] = c3[1] * 0.3; c3[2] = c3[2] * 0.25 }
}
function colourAt(hs) {
  ramp(hs)
  var r0 = c3[0], g0 = c3[1], b0 = c3[2]
  var f = (hs - SEA) / DLT, n = round(f), d = abs(f - n) * DLT
  var k = d < LW ? 1 - d / LW : 0
  if (n < 0 && (style == 2 || n % 2 != 0)) k = 0
  if (k > 0) {
    lineCol(hs)
    c3[0] = r0 + (c3[0] - r0) * k; c3[1] = g0 + (c3[1] - g0) * k; c3[2] = b0 + (c3[2] - b0) * k
  }
}
var nk = 0
function key(v) { if (nk < NS && v >= -1 && v <= 1) { keys[nk] = v; nk++ } }
var dirty = 1
function buildPalette() {
  SEA = style == 1 ? -1 : seaPct / 50
  DLT = style == 1 ? 2 / contours : (1 - SEA) / contours
  LW = DLT * (style == 2 ? 0.22 : 0.3)
  nk = 0
  for (var i = 0; i < NS; i++) keys[i] = 9
  key(-1); key(0); key(1); key(SEA)
  if (style != 1) { key(SEA - 0.001); var u = 1 - SEA; key(SEA + u * 0.05); key(SEA + u * 0.5); key(SEA + u * 0.75); key(SEA + u * 0.9); key(SEA - (SEA + 1) * 0.2) }
  for (var j = -40; j <= 40; j++) {
    var c = SEA + j * DLT
    if (c < -1 - DLT || c > 1 + DLT) continue
    if (j < 0 && (style == 2 || j % 2 != 0)) continue   // sea: every other line
    key(c - LW); key(c); key(c + LW)
    if (style == 2) key(c - 0.001)
  }
  arraySort(keys)
  var o = 0
  for (var pass = 0; pass < 2; pass++) for (i = 0; i < nk; i++) {
    var hs = keys[i]
    if (pass == 0 ? hs < 0 : hs >= 0) continue
    colourAt(hs)
    pal[o] = pass == 0 ? hs / 2 : 1 + hs / 2
    if (pass == 1 && hs == -1) pal[o] = 0.5001
    pal[o + 1] = c3[0]; pal[o + 2] = c3[1]; pal[o + 3] = c3[2]
    o += 4
  }
  colourAt(0)
  while (o < NS * 4) { pal[o] = 1; pal[o + 1] = c3[0]; pal[o + 2] = c3[1]; pal[o + 3] = c3[2]; o += 4 }
  setPalette(pal)
  dirty = 0
}

var z = 0, drift = 0, gain = 0.4, CW = 32, CH = 32
var rs = 0                          // 1 once a big grid renders at <= 4,096 cells
function bigGrid() {                // above 4,096 px: render <= 4,096 cells, engine expands bilinearly
  if (rs || pixelCount <= 4096 || !has2DMap() || gridWidth() < 1 || gridHeight() < 2) return
  var gw = gridWidth(), gh = gridHeight(), w = min(gw, floor(64 * sqrt(gw / gh) + 0.001))
  setRenderSize(w, min(gh, floor(4096 / w)), 1)   // gridWidth/gridHeight/pixelCount follow at once
  rs = 1
}

export function beforeRender(delta) {
  bigGrid()
  if (dirty) buildPalette()
  var dt = min(delta, 100) / 1000
  z = mod(z + dt / flowS * 2, 4096)
  drift = mod(drift + dt / flowS * 0.6, 4096)
}

function field(w, h, sx, sy) {
  fillNoise3D(A, w, h, sx, sy, drift, 3.7, z, 0)
  fillNoise3D(B, w, h, sx * 2.2, sy * 2.2, 11.3 - drift, 7.1, z * 1.6, 1)
  arrayScale(B, 0.3)
  arrayAdd(A, B)
  var target = 0.495 / max(arrayMaxAbs(A), 0.05)
  gain = target < gain ? target : gain + (target - gain) * 0.05
  arrayScale(A, gain)
}

export function renderBulk() {
  if (has2DMap()) {
    var gw = gridWidth(), gh = gridHeight()
    if (gw == 0) { gw = 32; gh = 32 }
    CW = min(gw, CAP); CH = min(gh, CAP)
    var sx = hills / CW
    field(CW, CH, sx, sx * gh / CH * CW / gw)
    paintCanvas(A, CW, CH, 1, gw > CW || gh > CH ? 1 : 0)   // bilinear only when upscaling
  } else {
    var n = min(pixelCount, CAP * CAP), k = n / pixelCount   // (i * n would overflow 16.16)
    field(n, 1, hills / n, 0)
    for (var i = 0; i < pixelCount; i++) {
      paint(A[floor(i * k)], 1)
      setPixel(i)
    }
  }
}
