// name: Circle Packing
// tags: panel, ambient
// Curated original (hand-written showcase of the Luxel language/builtins).
// Bubbles spawn in free space and inflate until they kiss a neighbour or a
// wall, then freeze with a glossy rim in a harmonious OKLCH palette. Once the
// panel is packed, a ripple pops them outward from a random point (an elastic
// pop) and a new palette regrows in the gaps. One fillCircle pair + a splat
// highlight per bubble; collision tests only for the few still growing.

var MAXC = 160
var cx = array(MAXC)           // panel-pixel units
var cy = array(MAXC)
var cr = array(MAXC)
var st = array(MAXC)           // 0 free slot, 1 growing, 2 frozen, 3 popping
var ch = array(MAXC)           // hue
var cl = array(MAXC)           // lightness
var cg = array(MAXC)           // generation (palette)
var cp = array(MAXC)           // pop clock
var VW = 0                     // the field: VW x VW panel-x pixels (fillCircle is round in map space)
var gwSeen = -1
var fresh = 1
var gen = 0
var hueBase = 0
var fam = 0
// palette families: main hue, its lightness, accent hue, its lightness (OKLCH)
var FAM = [0.53, 0.68, 0.08, 0.72,   0.80, 0.6, 0.24, 0.85,   0.70, 0.62, 0.15, 0.8,
           0.95, 0.64, 0.43, 0.82,   0.60, 0.72, 0.0, 0.66,   0.40, 0.7, 0.88, 0.66]
var dt = 0
var spawnAcc = 1
var hits = 1
var age = 0
var full = 0
var holdT = 0
var waving = 0
var wx = 0
var wy = 0
var wr = 0

var bubbles = 140
//# min=20 max=160 step=1 default=140
export function sliderBubbles(v) { bubbles = clamp(floor(v), 20, MAXC) }

var growPx = 12
//# min=2 max=30 step=1 default=12
export function sliderGrowPixelsPerSecond(v) { growPx = clamp(v, 2, 30) }

var holdS = 3
//# min=0 max=20 step=1 default=3
export function sliderHoldSeconds(v) { holdS = clamp(v, 0, 20) }

function size() {
  var gw = gridWidth()
  gwSeen = gw
  VW = gw > 0 ? gw : 48
  for (var i = 0; i < MAXC; i++) st[i] = 0
  fresh = 1
  spawnAcc = 1
  fam = floor(random(6))
  hueBase = FAM[fam * 4]
}

// largest radius a bubble at (x, y) could have right now
function room(x, y, skip) {
  var m = min(min(x, y), min(VW - 1 - x, VW - 1 - y))
  for (var j = 0; j < MAXC; j++) {
    if (st[j] == 0 || j == skip) continue
    m = min(m, dist(x, y, cx[j], cy[j]) - cr[j] - 0.7)
  }
  return m
}

function spawn() {
  var n = 0
  var slot = -1
  for (var j = 0; j < MAXC; j++) {
    if (st[j] == 0) { if (slot < 0) slot = j }
    else n++
  }
  if (slot < 0 || n >= bubbles) return 0
  for (var k = 0; k < 12; k++) {
    var x = 1 + random(VW - 3)
    var y = 1 + random(VW - 3)
    if (room(x, y, -1) > 1.6) {
      cx[slot] = x
      cy[slot] = y
      cr[slot] = 0.6
      st[slot] = 1
      // a family: mostly the main hue, about one in four the accent
      var f = fam * 4
      var a = random(1) < 0.27 ? 2 : 0
      ch[slot] = FAM[f + a] + random(0.08) - 0.04
      cl[slot] = FAM[f + a + 1] + random(0.12) - 0.06
      cg[slot] = gen
      return 1
    }
  }
  return 0
}

function bg() { oklch(0.13, 0.035, 0.74) }

function erase(i) {
  bg()
  fillCircle(cx[i] / (VW - 1), cy[i] / (VW - 1), cr[i] / (VW - 1))
}

function drawBubble(i, r, flash) {
  var x = cx[i] / (VW - 1)
  var y = cy[i] / (VW - 1)
  var k = 1 / (VW - 1)
  var h = ch[i]
  var l = cl[i]
  if (st[i] == 1) oklch(0.95, 0.06, h)            // a growing bubble's live rim
  else oklch(min(l + 0.17, 0.97), 0.07, h)
  fillCircle(x, y, r * k)
  if (r > 1.6) {
    oklch(l + (0.98 - l) * flash, 0.15 * (1 - flash), h)
    fillCircle(x, y, (r - max(0.8, r * 0.14)) * k)
  }
  if (r > 3) {
    rgb(0.3, 0.3, 0.3)
    splat(x - r * 0.35 * k, y - r * 0.35 * k, r * 0.45 * k, 1)
  }
}

export function beforeRender(delta) {
  dt = min(delta, 100) / 1000
}

export function renderBulk() {
  if (gridWidth() != gwSeen) size()
  // spawn slowly at first (so the early bubbles get big), then faster to fill
  // the gaps; when most attempts miss, the panel is full
  age += dt
  spawnAcc += dt * min(2 + age * 1.5, 16)
  while (spawnAcc >= 1) {
    spawnAcc -= 1
    if (!full) hits = hits * 0.92 + 0.08 * spawn()   // running success rate
  }
  if (!waving && (hits < 0.2 || age > 20)) full = 1
  // the frame persists: only growing and popping bubbles are redrawn, and a
  // grower never ends smaller than it was last drawn (neighbours respect it)
  if (fresh) { bg(); fill(); fresh = 0 }
  var growing = 0
  for (var i = 0; i < MAXC; i++) {
    if (st[i] == 1) {
      cr[i] += growPx * dt
      var m = room(cx[i], cy[i], i)
      if (cr[i] >= m || cr[i] > VW * 0.3) { cr[i] = max(0.6, min(cr[i], m)); st[i] = 2 }
      else growing++
      drawBubble(i, cr[i], 0)
    } else if (st[i] == 3) {
      cp[i] += dt
      erase(i)
      if (cp[i] > 0.5) st[i] = 0
      else drawBubble(i, cr[i] * max(0, 1 - easeOutElastic(cp[i] / 0.5)), 1 - cp[i] / 0.5)
    }
  }
  if (waving || growing || !full) holdT = 0
  else {
    holdT += dt
    if (holdT > holdS) {
      // the pop ripple: a new palette, centred somewhere random
      waving = 1
      full = 0
      holdT = 0
      age = 0
      hits = 1
      gen++
      fam = (fam + 1 + floor(random(4))) % 6
      hueBase = FAM[fam * 4]
      wx = random(VW)
      wy = random(VW)
      wr = 0
    }
  }
  if (waving) {
    wr += dt * VW * 0.3
    var left = 0
    for (var i = 0; i < MAXC; i++) {
      if (st[i] == 2 && cg[i] != gen) {
        if (dist(wx, wy, cx[i], cy[i]) < wr) { st[i] = 3; cp[i] = 0 }
        else left++
      }
    }
    if (!left) waving = 0
  }
}
