// name: Infinite Snake
// Curated original (hand-written showcase of the Luxel language/builtins).
// A snake plays itself forever: it hunts the pellet, grows, and when it hits a wall or itself
// the board flashes red and a new snake starts. Smartness is how well it plays — above 55 % it
// flood-fills ahead and refuses moves that would seal it into a pocket shorter than itself.
// The board is sized from the grid (half resolution, up to 32 cells) inside a faint border and
// painted into a small canvas only when a step changes it; renderBulk is one fillCanvas plus a
// splat that glides the head between steps. snakeLength, bestLength, deaths and moves are a
// live scoreboard for the vars watcher (they roll at 30000).

var boardCells = 0      // 0 = auto: half the grid width, up to 32
//# min=0 max=32 step=1 default=0
export function sliderBoardCells(v) { boardCells = clamp(floor(v), 0, 32) }

var smart = 0.7
//# min=0 max=100 step=1 default=70
export function sliderSmartness(v) { smart = clamp(v / 100, 0, 1) }

var sps = 10
//# min=1 max=20 step=1 default=10
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

export var snakeLength = 3
export var bestLength = 3
export var deaths = 0
export var moves = 0

var MAXN = 1024          // canvas cells, border included (32 x 32)
var DEATH_T = 0.55       // seconds of death flash before the board resets
var occ = array(MAXN)    // 0 empty, 1 food, 2 snake, 3 border
var ring = array(MAXN)   // body ring buffer: canvas cell of each segment
var vis = array(MAXN)    // flood-fill visit stamps
var q = array(MAXN)      // flood-fill queue
var hC = array(MAXN), sC = array(MAXN), vC = array(MAXN)
var DX = [1, 0, -1, 0], DY = [0, 1, 0, -1]

var CW = 0, CH = 0, IW = 1, IH = 1, N = 1   // canvas and interior (playable) size
var seenW = -1, seenH = -1, seenB = -1
var head = 0, len = 3, dir = 0, prevCell = 0, food = 0
var acc = 0, dead = 0, deathT = 0, visGen = 0
var bodyHue = 100 / 360, paintedHue = -1, paintedFood = -1
var popCell = 0, popT = 9, breath = 1

function idx(cx, cy) { return (cy + 1) * CW + cx + 1 }   // interior -> canvas cell
function cx(i) { return i % CW - 1 }
function cy(i) { return floor(i / CW) - 1 }

function layout() {
  seenW = gridWidth(); seenH = gridHeight(); seenB = boardCells
  var gw = seenW > 0 ? seenW : 32, gh = seenW > 0 ? seenH : 32
  if (boardCells > 0) {
    CW = clamp(boardCells, 8, 32)
    CH = clamp(round(boardCells * gh / gw), 8, 32)
  } else {
    CW = clamp(floor(gw / 2), 8, 32)
    CH = clamp(floor(gh / 2), 8, 32)
  }
  IW = CW - 2
  IH = CH - 2
  N = CW * CH
  for (var i = 0; i < N; i++) {
    var x = i % CW, y = floor(i / CW)
    var edge = x == 0 || y == 0 || x == CW - 1 || y == CH - 1
    occ[i] = edge ? 3 : 0
    vis[i] = 0
  }
  reset()
}

function reset() {
  for (var i = 0; i < N; i++) if (occ[i] != 3) occ[i] = 0
  head = 0
  len = max(3, floor(IW / 5))   // a bigger board starts a longer snake
  dir = floor(random(4))
  var sx = floor(IW / 2), sy = floor(IH / 2)
  for (var k = 0; k < len; k++) {
    var c = idx(sx - DX[dir] * k, sy - DY[dir] * k)
    ring[mod(-k, MAXN)] = c
    occ[c] = 2
  }
  prevCell = ring[0]
  placeFood()
  repaint()
}

function placeFood() {
  for (var t = 0; t < 60; t++) {
    var c = idx(floor(random(IW)), floor(random(IH)))
    if (occ[c] == 0) { occ[c] = 1; food = c; return }
  }
  for (var j = 0; j < N; j++) if (occ[j] == 0) { occ[j] = 1; food = j; return }
}

function foodDist(c) {
  var dx = abs(cx(c) - cx(food)), dy = abs(cy(c) - cy(food))
  if (wrap) { dx = min(dx, IW - dx); dy = min(dy, IH - dy) }
  return dx + dy
}

// The canvas cell one step from c in direction d, or -1 off the board.
function nextCell(c, d) {
  var x = cx(c) + DX[d], y = cy(c) + DY[d]
  if (wrap) { x = mod(x, IW); y = mod(y, IH) }
  else if (x < 0 || x >= IW || y < 0 || y >= IH) return -1
  return idx(x, y)
}

// Reachable free cells from c, counted up to cap (bounded so a step stays cheap).
function freeSpace(c, cap) {
  visGen++
  if (visGen > 30000) { visGen = 1; for (var z = 0; z < N; z++) vis[z] = 0 }
  var qh = 0, qt = 1, count = 0
  q[0] = c
  vis[c] = visGen
  while (qh < qt && count < cap) {
    var cur = q[qh++]
    count++
    for (var d = 0; d < 4; d++) {
      var n = nextCell(cur, d)
      if (n < 0 || vis[n] == visGen || occ[n] >= 2) continue
      vis[n] = visGen
      if (qt < MAXN) q[qt++] = n
    }
  }
  return count
}

// Never reverse, never into a wall or body, close on the food; when smart, refuse a
// move that leaves less room than the snake is long. -1 when every option is fatal.
function chooseDir() {
  var back = (dir + 2) % 4, best = -1, bestScore = -30000
  var deep = smart > 0.55, cap = min(len + 4, 120)
  for (var d = 0; d < 4; d++) {
    if (d == back) continue
    var n = nextCell(ring[head], d)
    if (n < 0 || occ[n] >= 2) continue
    var sc = -foodDist(n)
    if (deep) {
      var room = freeSpace(n, cap)
      if (room < len + 1) sc -= (len + 1 - room) * 8
    }
    sc += random(0.6)            // tie-break jitter keeps it from looking robotic
    if (sc > bestScore) { bestScore = sc; best = d }
  }
  return best
}

// The blunder: straight half the time, otherwise a turn, no safety check.
function wanderDir() {
  var u = random(1)
  return u < 0.5 ? dir : u < 0.75 ? (dir + 3) % 4 : (dir + 1) % 4
}

function step() {
  moves = moves >= 30000 ? 0 : moves + 1
  var d = random(1) < (1 - smart) * (1 - smart) ? wanderDir() : chooseDir()
  if (d < 0) d = wanderDir()
  dir = d
  var n = nextCell(ring[head], d)
  var tail = ring[mod(head - len + 1, MAXN)]
  if (n < 0 || (occ[n] >= 2 && n != tail)) { die(); return }
  var ate = occ[n] == 1
  if (ate) {
    len = min(len + 1, IW * IH - 2)
    popCell = n
    popT = 0
    placeFood()
    paintCell(food)
  } else {
    occ[tail] = 0
    paintCell(tail)
  }
  prevCell = ring[head]
  head = mod(head + 1, MAXN)
  ring[head] = n
  occ[n] = 2
  paintBody()
}

function die() {
  dead = 1
  deathT = 0
  deaths = deaths >= 30000 ? 0 : deaths + 1
  prevCell = ring[head]
}

function paintCell(i) {
  var o = occ[i]
  if (o == 3) { hC[i] = 0.62; sC[i] = 0.55; vC[i] = 0.14 }      // faint border
  else if (o == 1) { hC[i] = foodHue; sC[i] = 0.85; vC[i] = 1 }
  else { hC[i] = 0; sC[i] = 0; vC[i] = 0 }
}

// Body cells by age: a hue ramp that darkens toward the tail. The head cell is drawn
// dim here; the gliding head splat in renderBulk is the bright part.
function paintBody() {
  for (var k = 0; k < len; k++) {
    var c = ring[mod(head - k, MAXN)]
    var t = len > 1 ? k / (len - 1) : 0
    hC[c] = bodyHue + t * 0.1
    sC[c] = 0.8 + t * 0.2
    vC[c] = k == 0 ? 0.55 : 1 - t * 0.62
  }
}

function repaint() {
  for (var i = 0; i < N; i++) paintCell(i)
  paintBody()
  paintedHue = bodyHue
  paintedFood = foodHue
}

export function beforeRender(delta) {
  if (gridWidth() != seenW || gridHeight() != seenH || boardCells != seenB) layout()
  var dt = delta / 1000
  if (dead) {
    deathT += dt
    if (deathT >= DEATH_T) { dead = 0; reset() }
    else {
      var f = 1 - deathT / DEATH_T, fl = square(deathT * 12, 0.5)
      for (var i = 0; i < N; i++) {
        var o = occ[i]
        if (o == 2) { hC[i] = 0.02; sC[i] = 1 - fl * 0.85; vC[i] = f * (0.45 + 0.55 * fl) }
        else if (o == 0) { hC[i] = 0.02; sC[i] = 1; vC[i] = f * f * 0.09 }  // the board washes red
      }
    }
  } else {
    acc += min(delta, 250)       // a long stall must not fast-forward the game
    var ms = 1000 / sps, guard = 0
    while (acc >= ms && guard < 8 && !dead) { acc -= ms; step(); guard++ }
    if (acc > ms) acc = ms
  }
  snakeLength = len
  bestLength = max(bestLength, len)
  bodyHue = snakeHue + min(len * 0.003, 0.12)   // the strand shifts as the score climbs
  if (!dead && (bodyHue != paintedHue || foodHue != paintedFood)) repaint()
  breath = wave(time(0.008))
  vC[food] = 0.65 + 0.35 * breath                 // the pellet breathes
  popT += dt
}

export function renderBulk() {
  fillCanvas(hC, sC, vC, CW, CH)
  if (dead) return
  // a soft halo on the pellet, and a ring of light where the last one was eaten
  hsv(foodHue, 0.9, 0.12 + 0.12 * breath)
  splat((food % CW + 0.5) / CW, (floor(food / CW) + 0.5) / CH, 2.6 / CW, 1)
  if (popT < 0.4) {
    hsv(bodyHue, 0.5, 0.6 * (1 - popT / 0.4))
    splat((popCell % CW + 0.5) / CW, (floor(popCell / CW) + 0.5) / CH, (1.5 + popT * 12) / CW, 1)
  }
  // the head glides from the cell it left to the one it entered over each step
  var t = clamp(acc * sps / 1000, 0, 1)
  var c = ring[head]
  var x = mix(prevCell % CW, c % CW, t), y = mix(floor(prevCell / CW), floor(c / CW), t)
  if (abs(prevCell % CW - c % CW) > 1 || abs(floor(prevCell / CW) - floor(c / CW)) > 1) {
    x = c % CW; y = floor(c / CW)                  // a wrap-around jump does not glide
  }
  hsv(bodyHue + 0.02, 0.2, 1)
  splat((x + 0.5) / CW, (y + 0.5) / CH, 0.8 / CW, 2)
}
