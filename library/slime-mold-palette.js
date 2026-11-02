// name: Crystal Growth
// tags: panel, nature, ambient
// Clean-room reimplementation from the prose spec docs/pattern-specs/slime-mold-palette.md — reimagined, not a replica.
// "Rainbow smoke" growth at the panel's own resolution: each placement picks a
// target colour and puts it on the frontier cell whose painted neighbours match
// it best, so colours clump into agate bands and dendrites behind a glowing
// growth front. Sampling a frontier list keeps a placement O(1), and only new
// cells are drawn (the frame persists). A finished crystal glints, then dims to
// a ghost the next one grows over. Poke the panel (events) to seed a cell.

var CAP = 64
var cellv = array(CAP * CAP)   // -1 empty, -2 on the frontier, else colour 0..1
var front = array(CAP * CAP)   // frontier cell indices
var RING = 256                 // recent placements, drawn hot until they cool
var ringI = array(RING)
var ringT = array(RING)
var ev = array(4)
var nF = 0
var head = 0
var tail = 0
var W = 0
var H = 0
var S = 1                      // panel pixels per cell edge
var N = 0
var gwSeen = -1
var painted = 0
var phase = 0                  // 0 grow, 1 hold, 2 dim to a ghost
var now = 0
var pt = 0
var dt = 0
var pending = 1
var acc = 0
var pal = 0
var base = 0
var fresh = 1

// 0 rainbow, 1 ember, 2 lagoon, 3 orchid dusk, 4 aurora, 5 a new one each crystal
var palette = 5
//# min=0 max=5 step=1 default=5
export function sliderPalette(v) { palette = clamp(floor(v), 0, 5); if (palette < 5) pal = palette }

var seeds = 3
//# min=1 max=12 step=1 default=3
export function sliderSeeds(v) { seeds = clamp(floor(v), 1, 12) }

var growS = 20
//# min=4 max=60 step=1 default=20
export function sliderGrowSeconds(v) { growS = clamp(v, 4, 60) }

var holdS = 6
//# min=1 max=60 step=1 default=6
export function sliderHoldSeconds(v) { holdS = clamp(v, 1, 60) }

export function triggerNewCrystal() { phase = 2; pt = 0 }

// colour position p (0..1, cyclic) through palette `pal` in OKLCH;
// hot = the growth front's whiter, brighter version
function brush(p, hot) {
  var u = triangle(p)
  var l = 0.72
  var c = 0.15
  var h = p
  if (pal == 1) { l = 0.4 + 0.45 * u; c = 0.19 - 0.07 * u; h = 0.06 + 0.12 * u }         // ember
  else if (pal == 2) { l = 0.48 + 0.32 * u; c = 0.13; h = 0.5 + 0.25 * u }              // lagoon
  else if (pal == 3) { l = 0.5 + 0.3 * u; c = 0.17; h = 0.88 + 0.27 * u }               // orchid dusk
  else if (pal == 4) { l = 0.55 + 0.25 * u; c = 0.14; h = 0.4 + 0.4 * u }               // aurora
  if (hot) oklch(min(l + 0.25, 0.98), c * 0.35, h)
  else oklch(l, c, h)
}

function draw(i) {
  var c = i % W
  var r = floor(i / W)
  if (S == 1) { setPixel(gridIndex(c, r)); return }
  if (S == 0) { fillRect(c / W, r / H, (c + 1) / W, (r + 1) / H); return }
  for (var y = 0; y < S; y++) for (var x = 0; x < S; x++) setPixel(gridIndex(c * S + x, r * S + y))
}

function size() {
  var gw = gridWidth()
  gwSeen = gw
  if (gw > 0) {
    S = max(ceil(gw / CAP), ceil(gridHeight() / CAP))
    W = ceil(gw / S)
    H = ceil(gridHeight() / S)
  } else {
    S = 0                      // no grid: draw cells as coordinate rects
    W = 32
    H = 32
  }
  N = W * H
  fresh = 1
  restart()
}

function cdist(a, b) {
  var d = abs(a - b)
  return d > 0.5 ? 1 - d : d
}

function addFront(c, r) {
  if (c < 0 || c >= W || r < 0 || r >= H) return
  var i = r * W + c
  if (cellv[i] != -1) return
  cellv[i] = -2
  front[nF] = i
  nF++
}

function place(i, p) {
  if (cellv[i] < 0) painted++
  cellv[i] = p
  var c = i % W
  var r = floor(i / W)
  for (var dy = -1; dy <= 1; dy++) for (var dx = -1; dx <= 1; dx++) addFront(c + dx, r + dy)
  brush(p, 1)
  draw(i)
  if (head - tail >= RING) cool()
  ringI[head % RING] = i
  ringT[head % RING] = now
  head++
}

function cool() {
  var i = ringI[tail % RING]
  tail++
  if (cellv[i] >= 0) { brush(cellv[i], 0); draw(i) }
}

// mean colour distance from p to the painted 8-neighbours of cell i
function score(i, p) {
  var c = i % W
  var r = floor(i / W)
  var sum = 0
  var n = 0
  for (var dy = -1; dy <= 1; dy++) {
    var y = r + dy
    if (y < 0 || y >= H) continue
    for (var dx = -1; dx <= 1; dx++) {
      var x = c + dx
      if (x < 0 || x >= W) continue
      var v = cellv[y * W + x]
      if (v >= 0) { sum += cdist(v, p); n++ }
    }
  }
  return n ? sum / n + 0.012 * (8 - n) : 9      // + a pull toward compact fronts
}

function grow() {
  if (nF == 0) return
  // the target walks the palette as the crystal fills: bands, not confetti
  var p = (base + painted / N * 1.3 + random(0.16) - 0.08 + 1) % 1
  var best = 0
  var bestS = 99
  var k = min(nF, 48)
  for (var j = 0; j < k; j++) {
    var slot = floor(random(nF))
    var s = score(front[slot], p)
    if (s < bestS) { bestS = s; best = slot }
  }
  var i = front[best]
  nF--
  front[best] = front[nF]
  place(i, p)
}

function restart() {
  for (var i = 0; i < N; i++) cellv[i] = -1
  nF = 0
  head = 0
  tail = 0
  painted = 0
  acc = 0
  now = 0
  pt = 0
  phase = 0
  pending = 1
  if (palette == 5) pal = (pal + 1) % 5
  base = random(1)
}

export function beforeRender(delta) {
  dt = min(delta, 100) / 1000
  now += dt
  pt += dt
}

export function renderBulk() {
  if (gridWidth() != gwSeen) size()
  if (pending) {
    if (fresh) { oklch(0.22, 0.05, base); fill(); fresh = 0 }   // a substrate, not black
    for (var s = 0; s < seeds; s++) place(floor(random(N)), (base + random(0.1)) % 1)
    pending = 0
  }
  while (readEvent(ev)) {
    if (phase == 0) place(clamp(floor(ev[2] * H), 0, H - 1) * W + clamp(floor(ev[1] * W), 0, W - 1), random(1))
  }
  if (phase == 0) {
    acc += dt * N / growS
    var n = min(floor(acc), 600)
    acc -= n
    for (var j = 0; j < n; j++) grow()
    if (nF == 0) { phase = 1; pt = 0 }
  } else if (phase == 1) {
    // a finished crystal glints: random cells flash hot and cool again
    var g = dt * N / 120 + random(1)
    for (var j = 0; j < g; j++) {
      var i = floor(random(N))
      if (cellv[i] >= 0) place(i, cellv[i])
    }
    if (pt > holdS) { phase = 2; pt = 0 }
  } else {
    while (head > tail) cool()
    fade(pow(0.3, dt / 1.2))    // down to a ghost in 1.2 s
    if (pt > 1.2) restart()
  }
  while (head > tail && now - ringT[tail % RING] > 0.35) cool()
}
