// name: Maze
// Curated original (hand-written showcase of the Luxel language/builtins).
// A white-hot mole carves a perfect maze through dark rock (recursive
// backtracker on an explicit stack), its trail cooling behind it. Then a
// rainbow flood pours in from the entrance (breadth-first, hue = distance),
// the shortest path to the exit lights up with a comet running along it, and
// the maze fades for the next one. Sized to the panel; only cells that change
// are drawn (the frame persists), each as one native fillRect.

var CAP = 48
var link = array(CAP * CAP)    // bit 1: open east, bit 2: open south
var dist = array(CAP * CAP)    // carve: visited flag; flood: BFS distance
var stk = array(CAP * CAP)     // carve stack, then flood queue, then the path
var TRAIL = 12
var trail = array(TRAIL)
var nbr = array(4)
var rgbOut = array(3)
var VW = 0
var VH = 0
var N = 1
var M = 1
var cw = 2
var P = 3
var ox = 0
var oy = 0
var gwSeen = -1
var phase = 0                  // 0 carve, 1 flood, 2 solve, 3 show, 4 fade
var pt = 0
var dt = 0
var acc = 0
var sp = 0
var qh = 0
var qt = 0
var steps = 0
var lastOut = -1
var plen = 0
var drawn = 0
var comet = 0
var hue0 = 0
var dirty = 1

var corridor = 2
//# min=1 max=4 step=1 default=2
export function sliderCorridorPixels(v) { corridor = clamp(floor(v), 1, 4); dirty = 1 }

var buildS = 10
//# min=3 max=60 step=1 default=10
export function sliderBuildSeconds(v) { buildS = clamp(v, 3, 60) }

var holdS = 5
//# min=1 max=30 step=1 default=5
export function sliderHoldSeconds(v) { holdS = clamp(v, 1, 30) }

// one virtual-pixel rectangle, inclusive, as a coordinate-space fill
function rect(x0, y0, x1, y1) {
  fillRect((x0 - 0.4) / (VW - 1), (y0 - 0.4) / (VH - 1), (x1 + 0.4) / (VW - 1), (y1 + 0.4) / (VH - 1))
}

function cellRect(id) {
  var x = ox + 1 + (id % N) * P
  var y = oy + 1 + floor(id / N) * P
  rect(x, y, x + cw - 1, y + cw - 1)
}

// the wall gap between neighbouring cells a < b
function linkRect(a, b) {
  var x = ox + 1 + (a % N) * P
  var y = oy + 1 + floor(a / N) * P
  if (b == a + 1) rect(x + cw, y, x + cw, y + cw - 1)
  else rect(x, y + cw, x + cw - 1, y + cw)
}

function both(a, b) {
  cellRect(b)
  linkRect(min(a, b), max(a, b))
}

function rock() { oklch(0.15, 0.035, 0.74) }
function floorCol() { rgb(0.07, 0.26, 0.36) }
function floodCol(d) { oklch(0.72, 0.16, hue0 + d / (max(N, M) * 1.6)) }
function pathCol(k) { oklch(0.8 + 0.2 * k, 0.05 * (1 - k), 0.25) }

function open(a, b) {
  if (b == a + 1) return link[a] & 1
  if (a == b + 1) return link[b] & 1
  if (b == a + N) return link[a] & 2
  return link[b] & 2
}

function reset() {
  var gw = gridWidth()
  gwSeen = gw
  VW = gw > 0 ? gw : 48
  VH = gw > 0 ? gridHeight() : 48
  cw = corridor
  P = cw + 1
  N = clamp(floor((VW - 1) / P), 1, CAP)
  M = clamp(floor((VH - 1) / P), 1, CAP)
  ox = floor((VW - N * P - 1) / 2)
  oy = floor((VH - M * P - 1) / 2)
  for (var i = 0; i < N * M; i++) { link[i] = 0; dist[i] = 0 }
  for (var k = 0; k < TRAIL; k++) trail[k] = -1
  lastOut = -1
  rock()
  fill()
  floorCol()
  rect(ox, oy + 1, ox, oy + cw)                     // entrance, top-left
  rect(ox + N * P, oy + M * P - cw, ox + N * P, oy + M * P - 1)  // exit, bottom-right
  cellRect(0)
  dist[0] = 1
  stk[0] = 0
  sp = 1
  steps = 0
  acc = 0
  pt = 0
  phase = 0
  hue0 = random(1)
  dirty = 0
}

// one backtracker step: dig to a random unvisited neighbour, or back up
function carve() {
  var c = stk[sp - 1]
  var i = c % N
  var j = floor(c / N)
  var n = 0
  if (i > 0 && !dist[c - 1]) { nbr[n] = c - 1; n++ }
  if (i < N - 1 && !dist[c + 1]) { nbr[n] = c + 1; n++ }
  if (j > 0 && !dist[c - N]) { nbr[n] = c - N; n++ }
  if (j < M - 1 && !dist[c + N]) { nbr[n] = c + N; n++ }
  if (n == 0) {
    sp--
  } else {
    var b = nbr[floor(random(n))]
    if (b == c + 1) link[c] |= 1
    else if (b == c - 1) link[b] |= 1
    else if (b == c + N) link[c] |= 2
    else link[b] |= 2
    dist[b] = 1
    floorCol()
    both(c, b)
    stk[sp] = b
    sp++
  }
  // the trail ring: the evicted cell goes back to plain floor
  var old = trail[steps % TRAIL]
  if (old >= 0) {
    floorCol()
    if (lastOut >= 0 && lastOut != old && open(lastOut, old)) both(lastOut, old)
    else cellRect(old)
  }
  lastOut = old
  trail[steps % TRAIL] = sp > 0 ? stk[sp - 1] : 0
  steps++
}

// head first, oldest last; each cell also lights the gap to the one before
// it, and the oldest has cooled all the way to floor
function drawTrail() {
  for (var a = TRAIL - 1; a >= 0; a--) {
    var c = trail[((steps - 1 - a) % TRAIL + TRAIL) % TRAIL]
    if (c < 0) continue
    mixColors(1, 0.9, 0.6, 0.07, 0.26, 0.36, easeInQuad(a / (TRAIL - 1)), rgbOut)
    rgb(rgbOut[0], rgbOut[1], rgbOut[2])
    var p = a < TRAIL - 1 ? trail[((steps - 2 - a) % TRAIL + TRAIL) % TRAIL] : lastOut
    if (p >= 0 && p != c && open(p, c)) both(p, c)
    else cellRect(c)
  }
}

function startFlood() {
  for (var k = 0; k < TRAIL; k++) { if (trail[k] >= 0) { floorCol(); cellRect(trail[k]) } }
  for (var i = 0; i < N * M; i++) dist[i] = -1
  dist[0] = 0
  stk[0] = 0
  qh = 0
  qt = 1
  floodCol(0)
  cellRect(0)
  rect(ox, oy + 1, ox, oy + cw)
  phase = 1
  acc = 0
}

function floodVisit(c, b) {
  if (dist[b] >= 0 || !open(c, b)) return
  dist[b] = dist[c] + 1
  stk[qt] = b
  qt++
  floodCol(dist[b])
  both(c, b)
}

function flood() {
  var c = stk[qh]
  qh++
  var i = c % N
  if (i > 0) floodVisit(c, c - 1)
  if (i < N - 1) floodVisit(c, c + 1)
  if (c >= N) floodVisit(c, c - N)
  if (c + N < N * M) floodVisit(c, c + N)
}

// walk back from the exit along falling distance: path[0] is the entrance
function solve() {
  var c = N * M - 1
  plen = dist[c] + 1
  for (var d = plen - 1; d >= 0; d--) {
    stk[d] = c
    if (d == 0) break
    var i = c % N
    if (i > 0 && dist[c - 1] == d - 1 && open(c, c - 1)) c = c - 1
    else if (i < N - 1 && dist[c + 1] == d - 1 && open(c, c + 1)) c = c + 1
    else if (c >= N && dist[c - N] == d - 1 && open(c, c - N)) c = c - N
    else c = c + N
  }
  drawn = 0
  comet = 0
  phase = 2
  pt = 0
}

function paintPath(k, glow) {
  pathCol(glow)
  if (k == 0) { cellRect(stk[0]); rect(ox, oy + 1, ox, oy + cw) }
  else both(stk[k - 1], stk[k])
  if (k == plen - 1) rect(ox + N * P, oy + M * P - cw, ox + N * P, oy + M * P - 1)
}

export function beforeRender(delta) {
  dt = min(delta, 100) / 1000
  pt += dt
}

export function renderBulk() {
  if (dirty || gridWidth() != gwSeen) reset()
  if (phase == 0) {
    acc += dt * (2 * N * M) / buildS
    while (acc >= 1 && sp > 0) { acc -= 1; carve() }
    drawTrail()
    if (sp == 0) startFlood()
  } else if (phase == 1) {
    acc += dt * N * M / (buildS * 0.5)
    while (acc >= 1 && qh < qt) { acc -= 1; flood() }
    if (qh >= qt) { phase = 5; pt = 0 }
  } else if (phase == 5) {
    fade(pow(0.45, dt / 0.6))          // dim the flood under the coming path
    if (pt > 0.6) solve()
  } else if (phase == 2) {
    var upto = min(plen, ceil(pt / 1.5 * plen))
    while (drawn < upto) { paintPath(drawn, 0); drawn++ }
    if (drawn >= plen) { phase = 3; pt = 0 }
  } else if (phase == 3) {
    // a comet runs entrance → exit along the path, over and over
    var L = 8
    comet += dt * max(plen, 10) / 2
    if (comet >= plen + L) {
      for (var k = max(0, plen - L - 2); k < plen; k++) paintPath(k, 0)
      comet = 0
    }
    var h = floor(comet)
    for (var k = max(0, h - L - 1); k <= min(h, plen - 1); k++) paintPath(k, max(0, 1 - (h - k) / L))
    if (pt > holdS) { phase = 4; pt = 0 }
  } else {
    fade(pow(0.05, dt / 0.8))
    if (pt > 0.8) reset()
  }
}
