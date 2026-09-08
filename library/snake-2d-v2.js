// name: Infinite Snake v2
// Curated original for the Luxel library.
//
// The whole-frame rendering of `snake-2d.js`. The game is byte-for-byte the
// same — same board, same step clock, same brain, same controls, same
// scoreboard — only the way it reaches the panel changed.
//
// The original resolved `paintCell` once per pixel from `render2D`: a board
// read, a couple of branches and an `hsv()` for every LED, every frame. On
// the 64x64 HUB75 panel that is ~96 ms of VM per frame at 4096 px, almost
// all of it the per-pixel readout — and it grows with the panel while the
// game stays 16x16.
//
// So this version paints the board into three parallel canvas arrays
// instead: `hC`, `sC`, `vC`, each `array(N)`, one entry per BOARD CELL. A
// `dirty` flag — set by `reset()`, `step()` and `die()`, and by every frame
// of the death flash, which animates — repaints all 256 cells with exactly
// the original `paintCell` colour math. Nothing else changes between steps:
// `bodyHue` moves only when `len` does (an eat, which is a step), so the
// steady-state per-frame work is one cell (the food's `foodPulse`) plus the
// single `fillCanvas(hC, sC, vC, W, H)` that `renderFrame()` issues. The
// cost is per board cell per board CHANGE, not per pixel per frame.
//
// `fillCanvas` samples that 16x16 canvas through each pixel's mapped
// coordinates, which is exactly what `render2D` did — so it still plays the
// same game on a 16x16 panel, a 32x32 one or a 64x64 one. With no map at
// all the engine hands a `renderFrame` pattern that names a coordinate-space
// builtin its default `ceil(sqrt(n))` square grid, and that stands in for
// the original's `render` fallback of running the board out along the strip.
// The per-pixel entries are gone: `renderFrame` wins over them anyway, and a
// dead per-pixel path would only be a second thing to keep in step.
//
// One sampling detail: the original wrote `floor(x * (W - 0.01))`,
// `fillCanvas` uses nearest — `min(floor(x * W), W - 1)`. The `- 0.01` fudge
// existed only to keep x = 1 inside the board, which the clamp does properly.
// On the engine's grids the two agree exactly, and this was verified rather
// than assumed: 240 frames against `snake-2d.js` at 256 px (16x16) and
// 4096 px (64x64), same seed, on an installed coordinate map and on the
// procedural default grid alike — max per-channel difference 0.

var W = 16
var H = 16
var N = 256              // W * H
var DEATH_T = 0.55       // seconds of death flash before the board resets

var occ = array(N)       // 0 empty, 1 food, 2 snake
var seg = array(N)       // ring index of the head when it entered this cell
var bx = array(N)        // body ring buffer, cell x
var by = array(N)        // body ring buffer, cell y
var vis = array(N)       // flood-fill visit stamps
var qx = array(N)        // flood-fill queue
var qy = array(N)

// the display canvas: one HSV triple per board cell, handed to fillCanvas
var hC = array(N)
var sC = array(N)
var vC = array(N)
var dirty = 1            // the board changed — repaint every cell

var DX = array(4)
var DY = array(4)
DX[0] = 1;  DY[0] = 0
DX[1] = 0;  DY[1] = 1
DX[2] = -1; DY[2] = 0
DX[3] = 0;  DY[3] = -1

// --- controls -------------------------------------------------------------
var smart = 0.7
//# min=0 max=100 step=1 default=70
export function sliderSmartness(v) { smart = clamp(v / 100, 0, 1) }

var sps = 8
//# min=1 max=20 step=1 default=8
export function sliderStepsPerSecond(v) { sps = clamp(floor(v), 1, 30) }

var snakeHue = 100 / 360
//# min=0 max=360 step=1 default=100
export function sliderSnakeHue(v) { snakeHue = v / 360 }

var foodHue = 12 / 360
//# min=0 max=360 step=1 default=12
export function sliderFoodHue(v) { foodHue = v / 360 }

var wrap = 0
//# default=0
export function toggleWrapWalls(v) { wrap = v }

// --- scoreboard (visible to the vars watcher) -----------------------------
export var snakeLength = 3
export var bestLength = 3
export var deaths = 0
export var moves = 0

// --- state ----------------------------------------------------------------
var head = 0             // ring index of the head segment
var len = 3
var dir = 0
var fx = 8, fy = 4       // food cell
var acc = 0              // step-clock accumulator, ms
var dead = 0
var deathT = 0
var visGen = 0
var inited = 0
var foodPulse = 1
var bodyHue = 100 / 360
var paintedBodyHue = -1  // the hues the canvas was last painted with: the two
var paintedFoodHue = -1  // colour sliders are the one input a step can't see

function idx(cx, cy) { return cy * W + cx }

function reset() {
  for (var i = 0; i < N; i++) { occ[i] = 0; seg[i] = 0; vis[i] = 0 }
  visGen = 0
  head = 0
  len = 3
  dir = floor(random(4))
  for (var k = 0; k < len; k++) {
    var ri = mod(0 - k, N)          // ring index of the k-th segment back
    var px = 8 - DX[dir] * k
    var py = 8 - DY[dir] * k
    bx[ri] = px
    by[ri] = py
    var ci = idx(px, py)
    occ[ci] = 2
    seg[ci] = ri
  }
  placeFood()
  dirty = 1
}

function placeFood() {
  for (var t = 0; t < 60; t++) {
    var cx = clamp(floor(random(W)), 0, W - 1)
    var cy = clamp(floor(random(H)), 0, H - 1)
    var i = idx(cx, cy)
    if (occ[i] == 0) { occ[i] = 1; fx = cx; fy = cy; return }
  }
  for (var j = 0; j < N; j++) {
    if (occ[j] == 0) { occ[j] = 1; fx = j % W; fy = floor(j / W); return }
  }
}

// board distance to the food, wrap-aware
function foodDist(cx, cy) {
  var dx = abs(cx - fx)
  var dy = abs(cy - fy)
  if (wrap) { dx = min(dx, W - dx); dy = min(dy, H - dy) }
  return dx + dy
}

// Reachable empty cells from (sx, sy), counted up to `cap`. Bounded so the
// per-step cost stays flat no matter how big the open area is.
function freeSpace(sx, sy, cap) {
  visGen++
  if (visGen > 30000) { visGen = 1; for (var z = 0; z < N; z++) vis[z] = 0 }
  var qh = 0, qt = 0
  qx[0] = sx
  qy[0] = sy
  qt = 1
  vis[idx(sx, sy)] = visGen
  var count = 0
  while (qh < qt && count < cap) {
    var cx = qx[qh]
    var cy = qy[qh]
    qh++
    count++
    for (var d = 0; d < 4; d++) {
      var nx = cx + DX[d]
      var ny = cy + DY[d]
      if (wrap) { nx = mod(nx, W); ny = mod(ny, H) }
      else if (nx < 0 || nx >= W || ny < 0 || ny >= H) continue
      var ni = idx(nx, ny)
      if (vis[ni] == visGen) continue
      if (occ[ni] == 2) continue
      vis[ni] = visGen
      if (qt < N) { qx[qt] = nx; qy[qt] = ny; qt++ }
    }
  }
  return count
}

// The considered move: never reverse, never into a wall or body, prefer
// closing on the food — and above 55% smartness, refuse a move that leaves
// less room than the snake is long. Returns -1 when every option is fatal.
function chooseDir() {
  var back = (dir + 2) % 4
  var best = -1
  var bestScore = -30000
  var deep = smart > 0.55
  var cap = min(len + 4, 90)
  for (var d = 0; d < 4; d++) {
    if (d == back && len > 1) continue
    var nx = bx[head] + DX[d]
    var ny = by[head] + DY[d]
    if (wrap) { nx = mod(nx, W); ny = mod(ny, H) }
    else if (nx < 0 || nx >= W || ny < 0 || ny >= H) continue
    var ni = idx(nx, ny)
    if (occ[ni] == 2) continue
    var sc = -foodDist(nx, ny)
    if (deep) {
      var room = freeSpace(nx, ny, cap)
      if (room < len + 1) sc -= (len + 1 - room) * 8
    }
    sc += random(0.6)            // tie-break jitter: keeps it from looking robotic
    if (sc > bestScore) { bestScore = sc; best = d }
  }
  return best
}

// The blunder: straight half the time, otherwise a turn — no safety check at
// all, which is exactly how a dumb snake walks into a wall.
function wanderDir() {
  var u = random(1)
  return u < 0.5 ? dir : u < 0.75 ? (dir + 3) % 4 : (dir + 1) % 4
}

function step() {
  // 16.16 holds +-32768, so the scoreboard counters roll rather than wrap negative
  moves = moves >= 30000 ? 0 : moves + 1
  var pBlunder = (1 - smart) * (1 - smart)
  var d = random(1) < pBlunder ? wanderDir() : chooseDir()
  if (d < 0) d = wanderDir()     // boxed in: blunder on and take the loss
  dir = d

  var nx = bx[head] + DX[d]
  var ny = by[head] + DY[d]
  if (wrap) { nx = mod(nx, W); ny = mod(ny, H) }
  else if (nx < 0 || nx >= W || ny < 0 || ny >= H) { die(); return }

  var ni = idx(nx, ny)
  var tr = mod(head - (len - 1), N)
  var ate = occ[ni] == 1
  // the tail cell is legal to enter when the tail is about to vacate it
  if (occ[ni] == 2 && !(bx[tr] == nx && by[tr] == ny)) { die(); return }

  if (ate) {
    len = min(len + 1, N - 2)
    placeFood()                  // before the head lands, so it never overlaps
  } else {
    occ[idx(bx[tr], by[tr])] = 0 // tail vacates
  }
  head = mod(head + 1, N)
  bx[head] = nx
  by[head] = ny
  occ[ni] = 2
  seg[ni] = head
  dirty = 1
}

function die() {
  dead = 1
  deathT = 0
  deaths = deaths >= 30000 ? 0 : deaths + 1
  dirty = 1
}

export function beforeRender(delta) {
  var dt = delta / 1000
  if (!inited) { inited = 1; reset() }

  if (dead) {
    deathT += dt
    if (deathT >= DEATH_T) { dead = 0; reset() }
    dirty = 1                    // the death flash animates: repaint every frame
  } else {
    acc += min(delta, 250)       // a long stall must not fast-forward the game
    var ms = 1000 / sps
    var guard = 0
    while (acc >= ms && guard < 8 && !dead) { acc -= ms; step(); guard++ }
    if (acc > ms) acc = ms
  }

  snakeLength = len
  bestLength = max(bestLength, len)
  foodPulse = 0.65 + 0.35 * wave(time(0.008))
  bodyHue = snakeHue + min(len * 0.003, 0.12)  // strand shifts as the score climbs

  // A colour slider moves without the board moving, so watch the two hues the
  // canvas actually baked in; everything else that changes a cell's colour
  // goes through step()/reset()/die().
  if (bodyHue != paintedBodyHue || foodHue != paintedFoodHue) dirty = 1

  if (dirty) {
    paintBoard()
    dirty = 0
    paintedBodyHue = bodyHue
    paintedFoodHue = foodHue
  } else {
    vC[idx(fx, fy)] = foodPulse  // the only cell that breathes between steps
  }
}

// The original's per-pixel `paintCell`, run once per BOARD CELL into the
// canvas arrays: identical colour math, `hC[i]/sC[i]/vC[i]` where it called
// `hsv()`. The death-flash constants are loop-invariant, so they are lifted.
function paintBoard() {
  if (dead) {
    var f = 1 - deathT / DEATH_T
    var fl = square(deathT * 12, 0.5)
    var bodyS = 1 - fl * 0.85
    var bodyV = f * (0.45 + 0.55 * fl)
    var foodV = f * 0.25
    var boardV = f * f * 0.09
    for (var i = 0; i < N; i++) {
      var o = occ[i]
      if (o == 2) { hC[i] = 0.02; sC[i] = bodyS; vC[i] = bodyV }
      else if (o == 1) { hC[i] = foodHue; sC[i] = 1; vC[i] = foodV }
      else { hC[i] = 0.02; sC[i] = 1; vC[i] = boardV }   // the board washes red
    }
    return
  }
  for (var j = 0; j < N; j++) {
    var oc = occ[j]
    if (oc == 1) {
      hC[j] = foodHue; sC[j] = 0.85; vC[j] = foodPulse
    } else if (oc == 2) {
      var age = mod(head - seg[j], N)
      if (age == 0) {
        hC[j] = bodyHue + 0.02; sC[j] = 0.2; vC[j] = 1   // head: almost white
      } else {
        var t = len > 1 ? age / (len - 1) : 0
        hC[j] = bodyHue + t * 0.1; sC[j] = 0.8 + t * 0.2; vC[j] = 1 - t * 0.62
      }
    } else {
      hC[j] = 0; sC[j] = 0; vC[j] = 0                    // the board: empty cells are OFF
    }
  }
}

// One call, one frame: nearest-sampled through the map, exactly the cell
// `render2D` used to resolve per pixel.
export function renderFrame() {
  fillCanvas(hC, sC, vC, W, H)
}
