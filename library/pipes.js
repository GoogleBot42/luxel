// name: Pipes
// tags: panel, retro
// Curated original (hand-written showcase of the Luxel language/builtins).
// The screensaver classic: shaded pipes grow cell by cell across a coarse
// occupancy grid, turning 90 degrees at ball joints. When the grid is full the
// old run dims to a ghost layer and a fresh set grows over it in new colours.
// renderBulk on the persistent frame: each frame redraws only the growing
// heads (one fillRect per cylinder shading band), so nothing else is touched.

var cells = 10       // grid cells across the panel (pipe pitch)
var speed = 4        // cells per second each pipe grows
var count = 2        // pipes growing at once
var turns = 30       // % chance to turn at each cell

const MAXG = 6, MAXC = 32
var occ = array(MAXC * MAXC)
var gi = array(MAXG), gj = array(MAXG), gd = array(MAXG), gp = array(MAXG), gh = array(MAXG)
var DX = array(4), DY = array(4)
DX[0] = 1; DY[1] = 1; DX[2] = -1; DY[3] = -1

var gw = 0, ght = 0, cs = 4, nc = 8, nr = 8, ox = 0, oy = 0, rad = 1, pxu = 0.03, pyu = 0.03
var filled = 0, dimT = 0, dimK = 1, baseHue = 0.1, nextHue = 0, needClear = 1, lastCs = 0, lastW = -1

function setup() {
  gw = gridWidth() > 1 ? gridWidth() : 32
  ght = gridHeight() > 1 ? gridHeight() : 32
  cs = clamp(floor(gw / cells), 4, 16)
  nc = clamp(floor(gw / cs), 2, MAXC)
  nr = clamp(floor(ght / cs), 2, MAXC)
  ox = floor((gw - nc * cs) / 2) + floor(cs / 2)
  oy = floor((ght - nr * cs) / 2) + floor(cs / 2)
  rad = floor((cs - 2) / 2)
  pxu = 1 / (gw - 1); pyu = 1 / (ght - 1)
  lastCs = cells; lastW = gridWidth()
}

function restart() {
  var k
  for (k = 0; k < nc * nr; k++) occ[k] = 0
  filled = 0
  baseHue = mod(baseHue + 0.37, 1)
  for (k = 0; k < MAXG; k++) gp[k] = -1
}

function claim(i, j) { occ[j * nc + i] = 1; filled++ }
function free(i, j) { return i >= 0 && j >= 0 && i < nc && j < nr && occ[j * nc + i] == 0 }

function spawn(g) {
  var t, i, j
  gp[g] = -1                          // -1 = no live pipe
  for (t = 0; t < 16; t++) {
    i = floor(random(nc)); j = floor(random(nr))
    if (free(i, j)) {
      claim(i, j)
      gi[g] = i; gj[g] = j; gd[g] = floor(random(4)); gp[g] = 0
      gh[g] = mod(baseHue + nextHue * 0.618, 1); nextHue++
      cap(g)
      return
    }
  }
}

function shade(g, k) {           // band k of rad+1: 0 = specular core
  var e = 1 - k / (rad + 1)
  oklch(0.3 + 0.42 * e + 0.2 * e * e * e * e, 0.17 - 0.12 * e * e * e * e, gh[g])
}

function cap(g) {                // a ball joint at the pipe's current cell
  var x = (ox + gi[g] * cs) * pxu, y = (oy + gj[g] * cs) * pyu, r = rad + 1.3
  shade(g, rad + 1)
  fillCircle(x, y, r * pxu)
  shade(g, rad * 0.6)
  fillCircle(x - 0.25 * r * pxu, y - 0.25 * r * pyu, 0.62 * r * pxu)
  shade(g, 0)
  fillCircle(x - 0.4 * r * pxu, y - 0.4 * r * pyu, 0.3 * r * pxu)
}

function segment(g, p) {         // cylinder from the cell centre, p cells long
  var d = gd[g], cx = ox + gi[g] * cs, cy = oy + gj[g] * cs, len = p * cs
  var k, hw, ex, sh
  for (k = rad; k >= 0; k--) {
    hw = k + 0.4
    sh = -0.45 * (rad - k)        // the highlight sits toward the light, up-left
    ex = len + (p > 0 ? hw : 0)
    shade(g, k)
    if (DX[d] != 0) fillRect((cx - 0.4 * DX[d]) * pxu, (cy + sh - hw) * pyu, (cx + ex * DX[d]) * pxu, (cy + sh + hw) * pyu)
    else fillRect((cx + sh - hw) * pxu, (cy - 0.4 * DY[d]) * pyu, (cx + sh + hw) * pxu, (cy + ex * DY[d]) * pyu)
  }
}

function advance(g) {            // head reached the next cell: move and steer
  var d = gd[g], nd, c, tries, turned = 0
  gi[g] += DX[d]; gj[g] += DY[d]
  nd = d
  if (random(100) < turns || !free(gi[g] + DX[d], gj[g] + DY[d])) {
    c = random(1) < 0.5 ? 1 : 3
    for (tries = 0; tries < 2; tries++) {
      nd = (d + c) % 4
      if (free(gi[g] + DX[nd], gj[g] + DY[nd])) break
      c = 4 - c
      nd = d
    }
  }
  if (!free(gi[g] + DX[nd], gj[g] + DY[nd])) { cap(g); spawn(g); return }
  if (nd != d) turned = 1
  gd[g] = nd
  claim(gi[g] + DX[nd], gj[g] + DY[nd])
  gp[g] = 0
  if (turned) cap(g)
}

export function beforeRender(delta) {
  var dt = delta * 0.001, g
  if (gridWidth() != lastW || cells != lastCs) { setup(); restart(); needClear = 1 }
  if (dimT > 0) { dimT -= dt; if (dimT <= 0) restart(); return }
  dimK = pow(0.2, dt / 1.2)            // ghost the old run to 20 % in 1.2 s
  for (g = 0; g < count; g++) if (gp[g] >= 0) gp[g] += speed * dt
  if (filled > nc * nr * 0.8) dimT = 1.2
}

export function renderBulk() {
  var g
  if (needClear) { clear(); needClear = 0 }
  if (dimT > 0) { fade(dimK); return }
  for (g = 0; g < count; g++) {
    if (gp[g] < 0) { spawn(g); continue }
    if (gp[g] >= 1) { segment(g, 1); advance(g) }
    else segment(g, gp[g])
  }
}

//# min=4 max=32 step=1 default=10
export function sliderCells(v) { cells = clamp(floor(v), 4, MAXC) }

//# min=1 max=20 step=0.5 default=4
export function sliderSpeed(v) { speed = clamp(v, 1, 20) }

//# min=1 max=6 step=1 default=2
export function sliderPipes(v) { count = clamp(floor(v), 1, MAXG) }

//# min=0 max=80 step=5 default=30
export function sliderTurns(v) { turns = clamp(v, 0, 80) }
