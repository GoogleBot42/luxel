// name: Physarum
// tags: panel, nature
// Curated original (hand-written showcase of the Luxel language/builtins).
// Slime-mould agents sniff a shared trail map at three angles, turn toward the
// strongest scent, step and deposit; one agent per cell (a blocked agent turns
// at random), so the map never condenses into one vein; drifting food spots
// keep the networks merging and rerouting. Trail canvas from the grid (cap 64):
// canvasGet sensors, blur2D + feedback, grey fillCanvas + setOutputPalette.

var CAP = 64
var MAXA = 800
var NDIR = 256                  // headings are quantised to 256 directions
var trail = array(CAP * CAP)
var occ = array(CAP * CAP)      // 1 where an agent stands
var axd = array(MAXA)           // x + heading * 64 (x < 64), packed to save the budget
var ay = array(MAXA)
var pal = array(16)
var W = 0
var H = 0
var iW = 0
var iH = 0
var gwSeen = -1
var acc = 0
var t = 0
var palSeen = -1
var live = 0
var FOODDEP = 2

var agents = 500
//# min=100 max=800 step=50 default=500
export function sliderAgents(v) { agents = clamp(floor(v), 1, MAXA) }

// angle between the centre sensor and each side sensor, degrees
var sensorDeg = 30
//# min=10 max=90 step=5 default=30
export function sliderSensorAngle(v) { sensorDeg = clamp(v, 5, 120) }

// trail half-life, seconds
var decay = 0.3
//# min=0.1 max=2 step=0.05 default=0.3
export function sliderDecay(v) { decay = clamp(v, 0.05, 10) }

// food sources: drifting nutrient spots the network wires together
var food = 5
//# min=0 max=8 step=1 default=5
export function sliderFood(v) { food = clamp(floor(v), 0, 8) }

// 0 gold on indigo, 1 bioluminescent, 2 magma, 3 ice
var palette = 0
//# min=0 max=3 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 3) }

function stop(k, p, r, g, b) {
  pal[k * 4] = p
  pal[k * 4 + 1] = r
  pal[k * 4 + 2] = g
  pal[k * 4 + 3] = b
}

function buildPalette() {
  palSeen = palette
  if (palette == 0) {
    stop(0, 0, 0.03, 0.01, 0.09); stop(1, 0.22, 0.13, 0.04, 0.32)
    stop(2, 0.6, 0.85, 0.5, 0.08); stop(3, 1, 1, 0.93, 0.62)
  } else if (palette == 1) {
    stop(0, 0, 0, 0.02, 0.05); stop(1, 0.25, 0, 0.16, 0.26)
    stop(2, 0.62, 0.08, 0.8, 0.58); stop(3, 1, 0.85, 1, 0.9)
  } else if (palette == 2) {
    stop(0, 0, 0.04, 0, 0.03); stop(1, 0.3, 0.42, 0, 0.16)
    stop(2, 0.66, 1, 0.36, 0.05); stop(3, 1, 1, 0.95, 0.55)
  } else {
    stop(0, 0, 0.01, 0.02, 0.07); stop(1, 0.3, 0.1, 0.16, 0.46)
    stop(2, 0.7, 0.5, 0.75, 1); stop(3, 1, 1, 1, 1)
  }
  setOutputPalette(pal)
}

function cellOf(x, y) { return floor(y) * W + floor(x) }

// agents enter one at a time onto free cells, so the count dial works live
function admit(i) {
  for (var k = 0; k < 8; k++) {
    var x = 0.5 + random(W - 1)
    var y = 0.5 + random(H - 1)
    var c = cellOf(x, y)
    if (occ[c] == 0) {
      occ[c] = 1
      axd[i] = x + floor(random(NDIR)) * 64
      ay[i] = y
      return 1
    }
  }
  return 0
}

function size() {
  var gw = gridWidth()
  gwSeen = gw
  W = gw > 0 ? min(gw, CAP) : 32
  H = gw > 0 ? min(gridHeight(), CAP) : 32
  W = max(W, 8)
  H = max(min(H, floor(CAP * CAP / W)), 8)
  iW = 1 / W
  iH = W / (CAP * CAP)     // canvasGet/Add read the buffer as W x (CAP*CAP/W)
  feedback(trail, 0)
  feedback(occ, 0)
  live = 0
}

function step() {
  var want = max(40, ceil(agents * (W * H / 4096)))   // the dial counts agents per 64x64 of canvas
  while (live < want && admit(live)) live++
  while (live > want) {
    live--
    occ[cellOf(axd[live] % 64, ay[live])] = 0
  }
  var sa = floor(sensorDeg * (0.85 + 0.15 * sin(t * 0.12)) * NDIR / 360)  // the mesh slowly re-scales
  var ra = min(floor(sa * 1.5), 48)
  var sd = max(2.5, W * 0.07)
  var ss = max(0.5, W * 0.014)
  var dep = 0.32
  var ka = PI2 / NDIR
  for (var i = 0; i < live; i++) {
    var p = axd[i]
    var d = floor(p / 64)
    var x = p - d * 64
    var y = ay[i]
    var a = d * ka
    var fc = canvasGet(trail, W, (x + cos(a) * sd) * iW, (y + sin(a) * sd) * iH)
    var fl = canvasGet(trail, W, (x + cos(a + sa * ka) * sd) * iW, (y + sin(a + sa * ka) * sd) * iH)
    var fr = canvasGet(trail, W, (x + cos(a - sa * ka) * sd) * iW, (y + sin(a - sa * ka) * sd) * iH)
    if (fc >= fl && fc >= fr) d += floor(random(3)) - 1
    else if (fc < fl && fc < fr) d += random(1) < 0.5 ? ra : -ra
    else if (fl > fr) d += ra
    else d -= ra
    d = mod(d, NDIR)
    a = d * ka
    var nx = x + cos(a) * ss
    var ny = y + sin(a) * ss
    var c0 = cellOf(x, y)
    var c1 = cellOf(nx, ny)
    if (nx < 0.5 || nx > W - 0.5 || ny < 0.5 || ny > H - 0.5 || (c1 != c0 && occ[c1] > 0)) {
      d = floor(random(NDIR))    // blocked: stay put, face somewhere new
    } else {
      occ[c0] = 0
      occ[c1] = 1
      x = nx
      y = ny
      canvasAdd(trail, W, x * iW, y * iH, dep)
    }
    axd[i] = x + d * 64
    ay[i] = y
  }
  for (var k = 0; k < food; k++) {
    var fx = 0.5 + 0.4 * sin(t * (0.05 + k * 0.013) + k * 2.4)
    var fy = 0.5 + 0.4 * sin(t * (0.041 + k * 0.011) + k * 1.3 + 0.7)
    canvasAdd(trail, W, fx, fy * H * iH, FOODDEP * (W * H / 4096))
  }
  blur2D(trail, W, H, 1)
  feedback(trail, pow(0.5, 0.0333 / decay))
}

export function beforeRender(delta) {
  if (gridWidth() != gwSeen) size()
  if (palette != palSeen) buildPalette()
  var dt = min(delta, 100) * 0.001
  t += dt
  if (t > 3600) t -= 3600
  acc += dt
  var n = 0
  while (acc >= 0.0333 && n < 2) { step(); acc -= 0.0333; n++ }
  if (acc > 0.0333) acc = 0     // a slow device skips sim time rather than spiral
}

export function renderBulk() {
  fillCanvas(0, 0, trail, W, H, gridWidth() > W || gridHeight() > H ? 1 : 0)   // bilinear only when upscaling
}
