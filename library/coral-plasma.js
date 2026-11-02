// name: Neon Veins
// tags: panel, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/coral-plasma.md — reimagined, not a replica.
// Also folds docs/pattern-specs/patternflow/glow-tendrils.md and a-big-hit.md (Patternflow presets, CC BY-SA 4.0 — prose-described ideas only, no code).
// Glowing veins where an evolving simplex field crosses zero: coral ridges with
// white-hot cores and a different tint on each flank, paired bioluminescent
// tendrils around every crossing, or two independent fields as neon tubes in two
// colours. The field is native fillNoise3D canvases; the vein profile, width and
// colours live in a palette rebuilt each frame (it drifts), and one paintCanvas
// draws it. A dim glow floor keeps it luma-rich as the Sunrise scene's ramp base.

var CAP = 64                        // canvas = the grid, or an integer fraction of it
var A = array(CAP * CAP), B = array(CAP * CAP)
var pal = array(64)

var style = 0
//# min=0 max=2 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 2) }
var featPx = 12                     // distance between neighbouring veins, px
//# min=4 max=32 step=1 default=12
export function sliderScalePx(v) { featPx = clamp(v, 4, 32) }
var flowS = 20                      // seconds for the vein network to re-form
//# min=3 max=120 step=1 default=20
export function sliderFlowS(v) { flowS = clamp(v, 3, 120) }
var widthPx = 2                   // glow half-width, px
//# min=1 max=4 step=0.5 default=2
export function sliderWidthPx(v) { widthPx = clamp(v, 1, 4) }
var palette = 0
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 2) }

// the two flank tints of each palette: hue, saturation (deep sea | neon | coral)
var PH = [0.47, 0.75, 0.83, 0.52, 0.02, 0.9]
var c3 = array(3)
var o = 0
function stop(p, h, s, v) {
  hsv2rgb(h, s, v, c3)
  pal[o] = p; pal[o + 1] = c3[0]; pal[o + 2] = c3[1]; pal[o + 3] = c3[2]; o += 4
}
var FLOOR = 0.07
function flank(c, w, h, dir) {      // a glowing line at palette position c, one side
  stop(c + dir * w * 0.3, h, 0.6, 0.85); stop(c + dir * w * 0.65, h, 1, 0.4); stop(c + dir * w, h, 1, FLOOR)
}
function buildPalette(w, hx, hy) {
  o = 0
  if (style == 2) {                 // tube X at 0.25, tube Y at 0.75
    stop(0, hx, 1, FLOOR)
    stop(0.25 - w, hx, 1, FLOOR); stop(0.25 - w * 0.5, hx, 1, 0.5); stop(0.25, hx, 0.2, 1); flank(0.25, w, hx, 1)
    stop(0.5, hx, 1, FLOOR * 0.6)
    stop(0.75 - w, hy, 1, FLOOR); stop(0.75 - w * 0.5, hy, 1, 0.5); stop(0.75, hy, 0.2, 1); flank(0.75, w, hy, 1)
    stop(1, hy, 1, FLOOR)
  } else if (style == 1) {          // a strand either side of each crossing, dark seam between
    var c = w * 1.8, q = w * 0.7
    stop(0, hx, 1, 0.02); stop(c - q, hx, 1, 0.1); stop(c, hx, 0.3, 1); flank(c, q, hx, 1)
    stop(0.5, hx, 1, FLOOR * 0.6)
    stop(1 - c - q, hy, 1, FLOOR); stop(1 - c - q * 0.65, hy, 1, 0.4); stop(1 - c - q * 0.3, hy, 0.6, 0.85)
    stop(1 - c, hy, 0.3, 1); stop(1 - c + q, hy, 1, 0.1); stop(1, hy, 1, 0.02)
  } else {                          // coral: white core, a tint down each flank
    stop(0, hx, 0.15, 1); flank(0, w, hx, 1)
    stop(0.5, hx, 1, FLOOR * 0.6)
    stop(1 - w, hy, 1, FLOOR); stop(1 - w * 0.65, hy, 1, 0.4); stop(1 - w * 0.3, hy, 0.6, 0.85); stop(1, hy, 0.15, 1)
  }
  while (o < 64) { pal[o] = 1; o += 4 }
  setPalette(pal)
}

var z = 0, dx = 0, hueT = 0, K = 0.5, CW = 32, CH = 32, M = 1
export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  z = mod(z + dt / flowS, 4096)
  dx = mod(dx + dt / flowS * 0.4, 4096)
  hueT = mod(hueT + dt / 90, 1)

  // vein half-width in palette units: field slope per pixel x the requested px
  K = style == 2 ? 0.35 : 0.5        // field -> palette units; tubes keep |field| < 0.25
  var w = clamp(widthPx * K * 1.1 / featPx, 0.01, 0.2)
  var drift = 0.06 * sin(hueT * PI2)
  buildPalette(w, PH[palette * 2] + drift, PH[palette * 2 + 1] - drift)
}

export function renderBulk() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  CW = ceil(gw / ceil(gw / CAP)); CH = ceil(gh / ceil(gh / CAP))
  var f = style == 1 ? 0.35 : 0.5
  var sx = f / featPx * gw / CW, sy = f / featPx * gh / CH
  fillNoise3D(A, CW, CH, sx, sy, dx, 1.3, z, 0)
  arrayScale(A, K)
  if (style == 2) {                 // a second, independent field: the other tube
    fillNoise3D(B, CW, CH, sx * 0.8, sy * 0.8, 17.9, dx * 0.7 + 3.3, z * 0.8 + 40, 2)
    arrayScale(B, K)
    var n = CW * CH
    for (var i = 0; i < n; i++) {   // whichever crossing is nearer owns the cell
      var a = A[i], b = B[i]
      A[i] = a * a < b * b ? 0.25 + a : 0.75 + b
    }
  } else {                          // octave two roughens the veins
    fillNoise3D(B, CW, CH, sx * 2.3, sy * 2.3, 5.1 - dx, 9.7, z * 1.4, 1)
    arrayScale(B, K * 0.35)
    arrayAdd(A, B)
  }
  paintCanvas(A, CW, CH, 1, gw > CW || gh > CH ? 1 : 0)   // bilinear only when upscaling
}
