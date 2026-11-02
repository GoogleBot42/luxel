// name: Falling Sand
// tags: panel, nature
// Curated original (hand-written showcase of the Luxel language/builtins).
// Sand pours from wandering spouts, falls under gravity, slides down the
// pile's slopes and settles in layered strata over a dusk backdrop. When a
// spout is buried, a trapdoor opens and the pile drops out of the frame.
// Only grains in flight are simulated (a capped particle list over an
// occupancy canvas sized from the grid, cap 64x64); one paintCanvas a frame.

var CAP = 64
var MAXA = 384                  // grains in flight at once
var SAND0 = 0.25                // palette: 0..0.2 backdrop, 0.25..1 sand
var cell = array(CAP * CAP)     // palette position; +2 while the grain is in flight
var ax = array(MAXA)
var ay = array(MAXA)
var afy = array(MAXA)
var avy = array(MAXA)
var nA = 0
var W = 0
var H = 0
var gwSeen = -1
var tickMs = 0
var draining = 0
var drop = 0
var dropV = 0
var dropped = 0
var pourAcc = array(3)
var spoutX = array(3)
var t = 0

// palette: 3 backdrop stops + 6 sand stops, rewritten in place (setPalette
// keeps a live reference, so it is installed once)
var pal = array(36)
var rgbTmp = array(3)

var pour = 90
//# min=10 max=400 step=10 default=90
export function sliderPourGrainsPerSecond(v) { pour = clamp(v, 10, 400) }

var spouts = 2
//# min=1 max=3 step=1 default=2
export function sliderSpouts(v) { spouts = clamp(floor(v), 1, 3) }

// 0 desert gold, 1 rainbow strata, 2 the picker's colour
var sandMode = 0
//# min=0 max=2 step=1 default=0
export function sliderSand(v) { sandMode = clamp(floor(v), 0, 2); buildPalette() }

// Sand mode 2's colour. The picker's V is ignored — Luxel's global
// brightness owns that.
var pickH = 0.58
var pickS = 0.7
export function hsvPickerSandColor(h, s, v) { pickH = h; pickS = s; buildPalette() }

function stop(k, pos, r, g, b) {
  pal[k * 4] = pos
  pal[k * 4 + 1] = r
  pal[k * 4 + 2] = g
  pal[k * 4 + 3] = b
}

function stopHsv(k, pos, h, s, v) {
  hsv2rgb(h, s, v, rgbTmp)
  stop(k, pos, rgbTmp[0], rgbTmp[1], rgbTmp[2])
}

function buildPalette() {
  stop(0, 0, 0.035, 0.03, 0.11)       // dusk: indigo at the top…
  stop(1, 0.2, 0.11, 0.045, 0.12)     // …plum at the floor
  stop(2, 0.22, 0.11, 0.045, 0.12)
  if (sandMode == 1) {
    for (var k = 0; k < 6; k++) stopHsv(3 + k, SAND0 + k * 0.15, k / 5, 0.8, 1)
  } else if (sandMode == 2) {
    stopHsv(3, SAND0, pickH, pickS, 0.35)
    stopHsv(4, 0.4, pickH, pickS, 0.65)
    stopHsv(5, 0.55, pickH, pickS, 1)
    stopHsv(6, 0.7, pickH, pickS * 0.55, 1)
    stopHsv(7, 0.85, pickH, pickS, 0.8)
    stopHsv(8, 1, pickH, pickS * 0.8, 0.5)
  } else {
    stop(3, SAND0, 0.42, 0.2, 0.05)    // umber
    stop(4, 0.4, 0.78, 0.45, 0.12)     // ochre
    stop(5, 0.55, 1, 0.72, 0.3)        // gold
    stop(6, 0.7, 1, 0.88, 0.6)         // pale dune
    stop(7, 0.85, 0.85, 0.55, 0.22)
    stop(8, 1, 0.55, 0.28, 0.08)       // rust
  }
}
buildPalette()
setPalette(pal)

function bg(r) { return 0.2 * r / max(1, H - 1) }

function reset() {
  for (var r = 0; r < H; r++) {
    var b = bg(r)
    for (var c = 0; c < W; c++) cell[r * W + c] = b
  }
  nA = 0
  draining = 0
}

function size() {
  var gw = gridWidth()
  gwSeen = gw
  W = gw > 0 ? min(gw, CAP) : 32
  H = gw > 0 ? min(gridHeight(), CAP) : 32
  for (var s = 0; s < 3; s++) spoutX[s] = W * (s + 1) / 4
  reset()
}

// what is at a cell: a backdrop value, sand, sand+2 in flight; walls are sand
function at(c, r) {
  if (c < 0 || c >= W || r >= H) return 1
  return cell[r * W + c]
}

function solid(c, r) {
  if (c < 0 || c >= W || r >= H) return 1
  return cell[r * W + c] >= SAND0
}

function grainColor() {
  if (sandMode == 1) return SAND0 + 0.75 * ((t * 0.05 + random(0.04)) % 1)
  // slow strata: the mean shade wanders, each grain jitters around it
  var m = 0.5 + 0.3 * sin(t * 0.35) + 0.1 * sin(t * 0.13)
  return SAND0 + 0.75 * clamp(m + random(0.3) - 0.15, 0, 0.999)
}

// drop a grain into the top row near column x; a stream that is still
// clearing the row just loses that grain, a buried spout means "full"
function spawn(x) {
  if (nA >= MAXA) return
  var c = clamp(floor(x + random(2.4) - 1.2), 0, W - 1)
  var v = cell[c]
  if (v >= SAND0) {
    if (v < 2) startDrain()
    return
  }
  cell[c] = 2 + grainColor()
  ax[nA] = c
  ay[nA] = 0
  afy[nA] = 0.5
  avy[nA] = 30 + random(10)
  nA++
}

function move(k, c, r) {
  var from = ay[k] * W + ax[k]
  cell[r * W + c] = cell[from]
  cell[from] = bg(ay[k])
  ax[k] = c
  ay[k] = r
}

function retire(k) {
  var i = ay[k] * W + ax[k]
  cell[i] = cell[i] - 2
  nA--
  ax[k] = ax[nA]
  ay[k] = ay[nA]
  afy[k] = afy[nA]
  avy[k] = avy[nA]
}

function step(dt) {
  for (var k = 0; k < nA; k++) {
    var c = ax[k]
    var r = ay[k]
    if (!solid(c, r + 1)) {
      avy[k] = min(avy[k] + 140 * dt, 70)       // cells/s, gravity 140 cells/s²
      afy[k] += avy[k] * dt
      while (afy[k] >= r + 1 && !solid(c, r + 1)) { r++ }
      if (r != ay[k]) move(k, c, r)
      if (afy[k] >= r + 1) afy[k] = r + 0.99     // landed this tick
    } else if (r + 1 < H && cell[(r + 1) * W + c] >= 2) {
      afy[k] = r + 0.99                           // tailgating: go the moment it clears
    } else {
      var d = random(1) < 0.5 ? -1 : 1
      var a = at(c + d, r + 1)
      var b = at(c - d, r + 1)
      if (a < SAND0) move(k, c + d, r + 1)        // slide down the slope
      else if (b < SAND0) move(k, c - d, r + 1)
      else if (a < 2 && b < 2) {                  // settled on three sides
        retire(k)
        k--
        continue
      }
      afy[k] = ay[k]
      avy[k] = avy[k] * 0.5
    }
  }
  // spouts wander on slow incommensurate sines and pour at `pour` grains/s
  for (var s = 0; s < spouts; s++) {
    var home = W * (s + 1) / (spouts + 1)
    spoutX[s] = clamp(home + W * 0.3 / spouts * (sin(t * (0.21 + s * 0.07) + s * 2.1) + 0.4 * sin(t * 0.53 + s)), 0, W - 1)
    pourAcc[s] += pour * dt
    while (pourAcc[s] >= 1) {
      pourAcc[s] -= 1
      if (!draining) spawn(spoutX[s])
    }
  }
}

function startDrain() {
  for (var k = 0; k < nA; k++) { var i = ay[k] * W + ax[k]; cell[i] = cell[i] - 2 }
  nA = 0
  draining = 1
  drop = 0
  dropV = 0
  dropped = 0
}

// trapdoor: the whole pile falls as one block, accelerating
function drainStep(dt) {
  dropV += 120 * dt
  drop += dropV * dt
  var n = floor(drop)
  if (n < 1) return
  drop -= n
  for (var r = H - 1; r >= 0; r--) {
    var b = bg(r)
    var src = r - n
    for (var c = 0; c < W; c++) {
      var v = src >= 0 ? cell[src * W + c] : 0
      cell[r * W + c] = v >= SAND0 ? v : b
    }
  }
  dropped += n
  if (dropped >= H) reset()
}

export function beforeRender(delta) {
  if (gridWidth() != gwSeen) size()
  t += delta / 1000
  tickMs += min(delta, 100)
  while (tickMs >= 16) {
    tickMs -= 16
    if (draining) drainStep(0.016)
    else step(0.016)
  }
}

export function renderBulk() {
  paintCanvas(cell, W, H)
}
