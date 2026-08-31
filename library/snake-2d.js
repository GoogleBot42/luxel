// name: Infinite Snake
// Curated original for the Luxel library.
//
// A snake plays itself, forever. It hunts the food pellet, grows a segment
// each time it eats, and when it finally runs into a wall or into its own
// body the board flashes and a fresh snake starts. How well it plays is a
// dial: at Smartness 0 it wanders blindly and dies within seconds, at 100 it
// checks that a move does not seal it into a pocket smaller than its own
// body and survives for minutes.
//
// The game runs on a fixed 16x16 board on its own step clock (Steps Per
// Second), independent of the frame rate, and render2D samples that board
// through normalized map coordinates — so it plays the same game on a 16x16
// panel, a 32x32 one, or (via render) a bare strip. Head is near-white, body
// is a hue ramp that drifts as the score climbs, food is its own colour and
// pulses. `snakeLength`, `bestLength`, `deaths` and `moves` are exported for
// the vars watcher — a live scoreboard (the counters roll at 30000).

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
}

function die() {
  dead = 1
  deathT = 0
  deaths = deaths >= 30000 ? 0 : deaths + 1
}

export function beforeRender(delta) {
  var dt = delta / 1000
  if (!inited) { inited = 1; reset() }

  if (dead) {
    deathT += dt
    if (deathT >= DEATH_T) { dead = 0; reset() }
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
}

function paintCell(i) {
  var o = occ[i]
  if (dead) {
    var f = 1 - deathT / DEATH_T
    var fl = square(deathT * 12, 0.5)
    if (o == 2) hsv(0.02, 1 - fl * 0.85, f * (0.45 + 0.55 * fl))
    else if (o == 1) hsv(foodHue, 1, f * 0.25)
    else hsv(0.02, 1, f * f * 0.09)   // the board itself washes red
    return
  }
  if (o == 1) {
    hsv(foodHue, 0.85, foodPulse)
  } else if (o == 2) {
    var age = mod(head - seg[i], N)
    if (age == 0) {
      hsv(bodyHue + 0.02, 0.2, 1)          // head: almost white
    } else {
      var t = len > 1 ? age / (len - 1) : 0
      hsv(bodyHue + t * 0.1, 0.8 + t * 0.2, 1 - t * 0.62)
    }
  } else {
    hsv(0, 0, 0.01)                        // the board, barely lit
  }
}

export function render2D(index, x, y) {
  var cx = clamp(floor(x * (W - 0.01)), 0, W - 1)
  var cy = clamp(floor(y * (H - 0.01)), 0, H - 1)
  paintCell(cy * W + cx)
}

// No map: run the board out along the strip, row after row.
export function render(index) {
  paintCell(clamp(floor(index / pixelCount * N), 0, N - 1))
}
