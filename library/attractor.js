// name: Strange Attractor
// tags: panel, ambient
// Curated original (hand-written showcase of the Luxel language/builtins).
// A Clifford or de Jong map iterated thousands of times a frame from one
// persistent point; every hit adds dust to a density canvas sized from the grid
// (cap 64x64) that slowly evaporates. The four parameters glide between chaotic
// settings with a gentle drift, so the nebula folds and unfurls. One grey
// fillCanvas, tone-mapped by a log-spaced setOutputPalette.

var CAP = 64
var dens = array(CAP * CAP)
var pal = array(24)
var NP = 6
// hand-searched chaotic settings, a b c d each (verified to stay chaotic under the drift)
var CL = [1.56, -1.66, 1.24, -1.52, -1.59, 1.85, -1.27, -0.37, 1.82, -1.51, 1.49, -0.94,
          1.53, 1.87, -0.9, 1.45, -1.42, -1.89, -1.26, 1.37, 1.66, 1.78, -1.26, -0.94]
var DJ = [-2.27, 2.8, -2.47, 2.22, 2.8, -1.66, -1.91, -2.38, -1.71, 2.81, 2.3, 1.39,
          2.06, 1.7, 2.45, -2.7, -2.33, 2.17, -1.48, -2.43, 2.55, -1.85, 2.83, 1.27]
var W = 0
var H = 0
var yS = 1
var gwSeen = -1
var palSeen = -1
var px = 0.1
var py = 0.1
var t = 0
var morphT = 0
var dt = 0
var pa = 0
var pb = 0
var pc = 0
var pd = 0

var points = 2000
//# min=500 max=4000 step=100 default=2000
export function sliderPointsPerFrame(v) { points = clamp(floor(v), 50, 4000) }

// seconds from one shape to the next (half holding, half gliding)
var morph = 30
//# min=8 max=120 step=1 default=30
export function sliderMorphSeconds(v) { morph = clamp(v, 4, 600) }

// 0 Clifford, 1 de Jong
var map = 0
//# min=0 max=1 step=1 default=0
export function sliderMap(v) { map = clamp(floor(v), 0, 1) }

// 0 nebula, 1 ember, 2 ice, 3 aurora
var palette = 0
//# min=0 max=3 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 3) }

function stop(k, p, r, g, b) {
  pal[k * 4] = p
  pal[k * 4 + 1] = r
  pal[k * 4 + 2] = g
  pal[k * 4 + 3] = b
}

// stops are log-spaced: faint dust stays visible, dense folds go white
function buildPalette() {
  palSeen = palette
  stop(0, 0, 0, 0, 0.02)
  if (palette == 0) {
    stop(1, 0.02, 0.1, 0.02, 0.22); stop(2, 0.08, 0.42, 0.06, 0.5)
    stop(3, 0.22, 0.95, 0.3, 0.35); stop(4, 0.5, 1, 0.75, 0.4)
  } else if (palette == 1) {
    stop(1, 0.02, 0.2, 0.02, 0.01); stop(2, 0.08, 0.6, 0.1, 0.02)
    stop(3, 0.22, 1, 0.42, 0.05); stop(4, 0.5, 1, 0.8, 0.35)
  } else if (palette == 2) {
    stop(1, 0.02, 0.02, 0.05, 0.22); stop(2, 0.08, 0.08, 0.25, 0.6)
    stop(3, 0.22, 0.25, 0.65, 1); stop(4, 0.5, 0.7, 0.92, 1)
  } else {
    stop(1, 0.02, 0, 0.12, 0.14); stop(2, 0.08, 0.02, 0.45, 0.35)
    stop(3, 0.22, 0.3, 0.9, 0.45); stop(4, 0.5, 0.8, 1, 0.75)
  }
  stop(5, 1, 1, 1, 1)
  setOutputPalette(pal)
}

function size() {
  var gw = gridWidth()
  gwSeen = gw
  W = gw > 0 ? max(min(gw, CAP), 8) : 32
  H = gw > 0 ? max(min(gridHeight(), floor(CAP * CAP / W)), 8) : 32
  yS = H * W / (CAP * CAP)   // canvasAdd reads the buffer as W x (CAP*CAP/W)
  feedback(dens, 0)
}

// glide the parameters: hold half the period, ease across the other half
function params() {
  var P = map == 0 ? CL : DJ
  var u = morphT / morph
  var k = floor(u) % NP
  var f = frac(u) * 2 - 1
  f = f < 0 ? 0 : easeInOutSine(f)
  var j = ((k + 1) % NP) * 4
  k = k * 4
  pa = mix(P[k], P[j], f) + 0.1 * sin(t * 0.21)
  pb = mix(P[k + 1], P[j + 1], f) + 0.1 * sin(t * 0.17 + 1)
  pc = mix(P[k + 2], P[j + 2], f) + 0.08 * sin(t * 0.13 + 2)
  pd = mix(P[k + 3], P[j + 3], f) + 0.08 * sin(t * 0.19 + 3)
}

export function beforeRender(delta) {
  if (gridWidth() != gwSeen) size()
  if (palette != palSeen) buildPalette()
  dt = min(delta, 100) * 0.001
  t += dt
  if (t > 3600) t -= 3600
  morphT += dt
  if (morphT > morph * NP) morphT -= morph * NP
  params()
  feedback(dens, pow(0.5, dt * 0.8))           // ~1.25 s half-life
  // analytic extents of the map, so the nebula fills the panel
  var ex = map == 0 ? 1 + abs(pc) : 2
  var ey = map == 0 ? 1 + abs(pd) : 2
  var sx = 0.48 / ex
  var sy = 0.48 / ey * yS
  var oy = 0.5 * yS
  var inc = 170 * dt * (W * H / 4096) / points  // light per cell-second, whatever the count or canvas
  var x = px
  var y = py
  var a = pa
  var b = pb
  var c = pc
  var d = pd
  if (map == 0) {
    for (var i = 0; i < points; i++) {
      var nx = sin(a * y) + c * cos(a * x)
      y = sin(b * x) + d * cos(b * y)
      x = nx
      if (canvasAdd(dens, W, 0.5 + x * sx, oy + y * sy, inc) > 0.7) canvasAdd(dens, W, 0.5 + x * sx, oy + y * sy, -inc)
    }
  } else {
    for (var i = 0; i < points; i++) {
      var nx = sin(a * y) - cos(b * x)
      y = sin(c * x) - cos(d * y)
      x = nx
      if (canvasAdd(dens, W, 0.5 + x * sx, oy + y * sy, inc) > 0.7) canvasAdd(dens, W, 0.5 + x * sx, oy + y * sy, -inc)
    }
  }
  px = x
  py = y
}

export function renderBulk() {
  fillCanvas(0, 0, dens, W, H, gridWidth() > W || gridHeight() > H ? 1 : 0)   // bilinear only when upscaling
}
