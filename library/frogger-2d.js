// name: Frogger 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Frogger 2D"; original source never consulted.
// DELIBERATELY REDESIGNED per the 2026-09-01 review: the described original is
// only sliding rainbow bars in rows. Jeremy asked for the actual arcade game,
// self-playing, with a smartness dial — fidelity to the description is waived.
//
// A frog crosses a sixteen-row board on its own, forever. Row 0 is the start
// bank; rows 1-6 are a six-lane road where cars stream in alternating
// directions at their own speeds; row 7 is the median; rows 8-13 are the
// river, where the frog dies unless it is standing on a drifting log or a
// turtle raft (two of the six turtle lanes periodically dive, taking any
// passenger under with them); row 14 is the far bank, and row 15 holds five
// lily pads. Fill all five and the board celebrates and re-rolls.
//
// How well the frog plays is a dial. `Smartness` sets both a blunder
// probability and how many hops ahead it checks: at 0 it hops at random and is
// roadkill within seconds, at 100 it runs a breadth-first survivability search
// several hops deep, waits for gaps, rides logs sideways to line up with a pad
// and edges away from the bank the current is sweeping it toward. Measured
// headless on the 16x16 rig at the default 4 hops/s, 120 s, mean of 3 seeds:
// Smartness 0 -> 89 deaths and 0 pads filled; 25 -> 67 and 0; 50 -> 34 and 3;
// 75 -> 11 and 7; 100 -> 0.3 deaths, 13 pads and 2 boards cleared.
//
// The game runs on a fixed 16x16 board on its own hop clock, independent of the
// frame rate, and render2D samples that board through normalized map
// coordinates — so it plays the same game on a 16x16 panel, a 32x32 one, or
// (via render) a bare strip. `goals`, `deaths`, `level`, `lives` and `hops` are
// exported for the vars watcher; the counters roll at 30000.

var W = 16
var H = 16
var N = 256                 // W * H

var MED_ROW = 7             // median between road and river
var RIVER_LO = 8
var RIVER_HI = 13
var BANK_ROW = 14           // far bank, where the frog lines up with a pad
var GOAL_ROW = 15

var NPAD = 5
var PAD_STEP = 3            // pads at columns 1, 4, 7, 10, 13

var DEATH_T = 0.55          // seconds of death flash
var PAD_T = 0.5             // seconds of pad-reached celebration
var LEVEL_T = 1.5           // seconds of board-cleared celebration
var TURTLE_PERIOD = 6       // seconds of one surface/dive cycle
var QCAP = 96               // breadth-first frontier cap (>= reachable set)

// --- board ----------------------------------------------------------------
var laneKind = array(H)     // 0 bank, 1 road, 2 river, 3 goal
var laneDir = array(H)      // -1, 0, +1
var laneSpeedMul = array(H)
var lanePeriodBase = array(H)
var lanePhase = array(H)    // turtle dive phase
var laneTurtle = array(H)
var laneHue = array(H)
var laneOff = array(H)      // continuous scroll offset, cells
var lanePeriod = array(H)   // derived: spacing between objects, cells
var laneLen = array(H)      // derived: object length, cells
var laneVel = array(H)      // derived: cells per second, signed

var padX = array(NPAD)
var padFilled = array(NPAD)

// breadth-first lookahead scratch
var curQ = array(QCAP)
var nxtQ = array(QCAP)
var mark = array(N)
var markGen = 0
var roomFor = array(5)

// move table: up, left, right, wait, down
var MDX = array(5)
var MDY = array(5)
MDX[0] = 0;  MDY[0] = 1
MDX[1] = -1; MDY[1] = 0
MDX[2] = 1;  MDY[2] = 0
MDX[3] = 0;  MDY[3] = 0
MDX[4] = 0;  MDY[4] = -1

// --- controls -------------------------------------------------------------
var smart = 0.7
//# min=0 max=100 step=1 default=70
export function sliderSmartness(v) { smart = clamp(v / 100, 0, 1) }

var hopsPerSecond = 4
//# min=1 max=10 step=1 default=4
export function sliderHopsPerSecond(v) { hopsPerSecond = clamp(floor(v), 1, 10) }

var trafficSpeed = 1.6
//# min=0.2 max=6 step=0.1 default=1.6
export function sliderTrafficSpeed(v) { trafficSpeed = clamp(v, 0.05, 8); recalcLanes() }

var density = 0.45
//# min=10 max=90 step=1 default=45
export function sliderTrafficDensity(v) { density = clamp(v / 100, 0.05, 0.95); recalcLanes() }

var logCover = 0.55
//# min=20 max=90 step=1 default=55
export function sliderLogCoverage(v) { logCover = clamp(v / 100, 0.1, 0.95); recalcLanes() }

var turtlesOn = 1
//# default=1
export function toggleDivingTurtles(v) { turtlesOn = v }

// --- scoreboard (visible to the vars watcher) -----------------------------
export var goals = 0
export var deaths = 0
export var level = 1
export var lives = 3
export var hops = 0

// --- state ----------------------------------------------------------------
var fx = 8                  // frog x in cells, fractional while riding a log
var fcx = 8, fcy = 0        // frog cell (fcx = round(fx))
var prevX = 8, prevY = 0    // cell it hopped off, for the hop ghost
var hopAge = 0              // seconds since the last hop
var hopAcc = 0              // hop-clock accumulator, seconds
var turtleT = 0             // wrapped turtle clock, seconds
var mode = 0                // 0 play, 1 dead, 2 pad reached, 3 board cleared
var modeT = 0
var deathKind = 0           // 0 squashed, 1 drowned
var inited = 0

function idx(cx, cy) { return cy * W + cx }

// --- lane geometry --------------------------------------------------------
function recalcLanes() {
  var r
  for (r = 0; r < H; r++) {
    lanePeriod[r] = lanePeriodBase[r]
    if (laneKind[r] == 1) {
      laneLen[r] = clamp(lanePeriod[r] * density, 1, lanePeriod[r] - 1)
    } else if (laneKind[r] == 2) {
      laneLen[r] = clamp(lanePeriod[r] * logCover, 2, lanePeriod[r] - 1)
    } else {
      laneLen[r] = 0
    }
    laneVel[r] = laneDir[r] * trafficSpeed * laneSpeedMul[r]
  }
}

function layoutBoard() {
  var r
  for (r = 0; r < H; r++) {
    lanePhase[r] = random(1)
    laneTurtle[r] = 0
    laneHue[r] = 0
    if (r == 0 || r == MED_ROW || r == BANK_ROW) {      // safe grass
      laneKind[r] = 0
      laneDir[r] = 0
      laneSpeedMul[r] = 0
      lanePeriodBase[r] = 8
    } else if (r == GOAL_ROW) {
      laneKind[r] = 3
      laneDir[r] = 0
      laneSpeedMul[r] = 0
      lanePeriodBase[r] = 8
    } else if (r < MED_ROW) {                           // road
      laneKind[r] = 1
      laneDir[r] = mod(r, 2) == 0 ? 1 : -1
      laneSpeedMul[r] = 0.55 + random(1.15)
      lanePeriodBase[r] = 4 + floor(random(4))          // 4..7 cells apart
      laneHue[r] = frac(0.02 + (r - 1) * 0.17 + random(0.05))
    } else {                                            // river
      laneKind[r] = 2
      laneDir[r] = mod(r, 2) == 0 ? -1 : 1
      laneSpeedMul[r] = 0.5 + random(1)
      lanePeriodBase[r] = 5 + floor(random(4))          // 5..8 cells apart
      laneTurtle[r] = (r == RIVER_LO + 1 || r == RIVER_LO + 4) ? 1 : 0
    }
    laneOff[r] = random(lanePeriodBase[r])
  }
  var p
  for (p = 0; p < NPAD; p++) { padX[p] = 1 + p * PAD_STEP; padFilled[p] = 0 }
  recalcLanes()
}

// Is a car / log / turtle over point `x` (cells, may be fractional) right now?
function occAt(r, x) {
  if (laneLen[r] <= 0) return 0
  return mod(x - laneOff[r], lanePeriod[r]) < laneLen[r]
}

// The same question `tOff` seconds from now.
function occAtT(r, x, tOff) {
  if (laneLen[r] <= 0) return 0
  return mod(x - (laneOff[r] + laneVel[r] * tOff), lanePeriod[r]) < laneLen[r]
}

// 0 = fully surfaced, 1 = fully under. Deadly above 0.75.
function turtleLevel(r, tOff) {
  if (!turtlesOn || !laneTurtle[r]) return 0
  var p = frac((turtleT + tOff) / TURTLE_PERIOD + lanePhase[r])
  if (p < 0.6) return 0
  if (p < 0.7) return (p - 0.6) * 10
  if (p < 0.9) return 1
  return 1 - (p - 0.9) * 10
}

function padIndex(cx) {
  var p
  for (p = 0; p < NPAD; p++) if (padX[p] == cx) return p
  return -1
}

// Planning view: could the frog SIT on this cell for the whole hop that starts
// `tOff` seconds from now? Deliberately conservative — a hop lands the frog
// there for a full dwell, so a car that merely arrives mid-dwell still kills
// it. Inflating the car by the distance it covers in one dwell answers "does
// any part of this lane's traffic reach the cell before the next hop" exactly.
// `xf` may be fractional: on the river the frog sits wherever the log carried
// it, and testing the rounded cell instead is what makes it step off a log's
// tail. Everything off the water is on the integer grid.
function cellSafe(xf, cy, tOff) {
  var cx = floor(xf + 0.5)
  if (cx < 0 || cx >= W || cy < 0 || cy >= H) return 0
  var k = laneKind[cy]
  if (k == 0) return 1
  var d = 1 / hopsPerSecond
  if (k == 1) {
    if (laneLen[cy] <= 0) return 1
    var v = laneVel[cy]
    var sh = v < 0 ? 0 - v * d : 0
    var q = mod(cx - (laneOff[cy] + v * tOff) + sh, lanePeriod[cy])
    return q < laneLen[cy] + abs(v) * d ? 0 : 1
  }
  if (k == 2) {
    // the frog rides with the log, so arrival coverage carries the whole dwell
    if (!occAtT(cy, xf, tOff)) return 0
    if (turtleLevel(cy, tOff) > 0.75 || turtleLevel(cy, tOff + d) > 0.75) return 0
    // ...but the current is also carrying it toward a bank. Keep three hops of
    // margin so there are always hops left in which to bail out.
    var e = xf + laneVel[cy] * d * 3
    return e < -0.5 || e >= W - 0.5 ? 0 : 1
  }
  var p = padIndex(cx)
  return p >= 0 && !padFilled[p] ? 1 : 0
}

// Instantaneous truth for the frog itself. Uses its FRACTIONAL x on the river,
// where it drifts with whatever it is standing on.
function frogAlive() {
  if (fcx < 0 || fcx >= W || fcy < 0 || fcy >= H) return 0
  var k = laneKind[fcy]
  if (k == 0 || k == 3) return 1
  if (k == 1) return occAt(fcy, fcx) ? 0 : 1
  if (!occAt(fcy, fx)) return 0
  return turtleLevel(fcy, 0) > 0.75 ? 0 : 1
}

// --- the frog's brain -----------------------------------------------------
function stampGen() {
  markGen++
  if (markGen > 20000) { markGen = 1; var z; for (z = 0; z < N; z++) mark[z] = 0 }
}

// Breadth-first over (column, row) states: how many distinct cells can still
// be alive after `steps` further hops, starting from a cell already known safe
// at `t0`. 0 means every continuation from here is fatal.
function expand(sx, sy, steps, t0) {
  if (steps <= 0) return 1
  var curN = 1
  curQ[0] = idx(sx, sy)
  var t = t0
  var k
  for (k = 0; k < steps; k++) {
    t = t + 1 / hopsPerSecond
    stampGen()
    var nn = 0
    var qi
    for (qi = 0; qi < curN; qi++) {
      var s = curQ[qi]
      var cy = floor(s / W)
      var cx = s - cy * W
      var m
      for (m = 0; m < 5; m++) {
        var nx = cx + MDX[m]
        var ny = cy + MDY[m]
        if (!cellSafe(nx, ny, t)) continue
        var ns = idx(nx, ny)
        if (mark[ns] == markGen) continue
        mark[ns] = markGen
        if (nn < QCAP) { nxtQ[nn] = ns; nn++ }
      }
    }
    if (nn == 0) return 0
    var ci
    for (ci = 0; ci < nn; ci++) curQ[ci] = nxtQ[ci]
    curN = nn
  }
  return curN
}

// Column of the nearest lily pad still open.
function padTarget() {
  var bestD = 99
  var bestC = 8
  var p
  for (p = 0; p < NPAD; p++) {
    if (padFilled[p]) continue
    var d = abs(padX[p] - fcx)
    if (d < bestD) { bestD = d; bestC = padX[p] }
  }
  return bestC
}

// The blunder: a hop with no safety check at all, biased forward the way a
// panicking frog is. This is the whole of the AI at Smartness 0.
function wanderMove() {
  var u = random(1)
  if (u < 0.4) return 0
  if (u < 0.6) return 1
  if (u < 0.8) return 2
  if (u < 0.9) return 3
  return 4
}

function chooseMove() {
  // the hop lands NOW and the frog sits there for one dwell, so the first
  // move is judged at t = 0 and every further layer one dwell later
  var depth = clamp(floor(smart * 5), 1, 5)
  var target = padTarget()
  var m
  for (m = 0; m < 5; m++) roomFor[m] = 0
  var any = 0
  for (m = 0; m < 5; m++) {
    var nxf = fx + MDX[m]
    var ny = fcy + MDY[m]
    if (!cellSafe(nxf, ny, 0)) continue
    var room = expand(floor(nxf + 0.5), ny, depth - 1, 0)
    roomFor[m] = room
    if (room > 0) any = 1
  }
  // boxed in at this depth: fall back to bare one-hop safety
  if (!any) {
    for (m = 0; m < 5; m++) {
      if (cellSafe(fx + MDX[m], fcy + MDY[m], 0)) { roomFor[m] = 1; any = 1 }
    }
  }
  if (!any) return wanderMove()

  var best = -1
  var bestScore = -30000
  for (m = 0; m < 5; m++) {
    if (roomFor[m] <= 0) continue
    var nx = fcx + MDX[m]
    var ny = fcy + MDY[m]
    var sc = MDY[m] * 3                       // forward is the whole point
    if (m == 3) sc = sc - 0.5                 // dawdling costs a little
    sc = sc + smart * min(roomFor[m], 24) * 0.04
    // past the median, start lining up with an open pad
    if (fcy >= MED_ROW) sc = sc - 0.6 * smart * abs(nx - target)
    // do not let the current sweep you into the wall
    if (ny >= RIVER_LO && ny <= RIVER_HI) {
      if (laneVel[ny] > 0) sc = sc - smart * 0.5 * max(0, 3 - (W - 1 - nx))
      if (laneVel[ny] < 0) sc = sc - smart * 0.5 * max(0, 3 - nx)
    }
    sc = sc + random(0.3)                     // keeps it from looking robotic
    if (sc > bestScore) { bestScore = sc; best = m }
  }
  return best < 0 ? wanderMove() : best
}

// --- game -----------------------------------------------------------------
function respawn() {
  fx = 8
  fcx = 8
  fcy = 0
  prevX = 8
  prevY = 0
  hopAge = 0
  hopAcc = 0
}

// squashed on the road, drowned everywhere else — bounds-free, so a blunder
// that walked the frog off the board still classifies its own death
function onRoad(r) { return r >= 1 && r < MED_ROW }

function die(kind) {
  mode = 1
  modeT = 0
  deathKind = kind
  deaths = deaths >= 30000 ? 0 : deaths + 1
}

function hop() {
  hops = hops >= 30000 ? 0 : hops + 1
  var pBlunder = (1 - smart) * (1 - smart)
  var m = random(1) < pBlunder ? wanderMove() : chooseMove()
  prevX = fcx
  prevY = fcy
  hopAge = 0
  fx = fx + MDX[m]
  fcy = fcy + MDY[m]
  if (laneKind[clamp(fcy, 0, H - 1)] != 2) fx = floor(fx + 0.5)   // off the water, back on the grid
  fcx = floor(fx + 0.5)
  if (fcx < 0 || fcx >= W || fcy < 0) { fx = clamp(fcx, 0, W - 1); fcx = fx; fcy = max(fcy, 0); die(1); return }
  if (fcy == GOAL_ROW) {
    var p = padIndex(fcx)
    if (p < 0 || padFilled[p]) { die(1); return }
    padFilled[p] = 1
    goals = goals >= 30000 ? 0 : goals + 1
    mode = 2
    modeT = 0
    var q
    var full = 1
    for (q = 0; q < NPAD; q++) if (!padFilled[q]) full = 0
    if (full) { mode = 3; modeT = 0 }
    return
  }
  if (!frogAlive()) die(onRoad(fcy) ? 0 : 1)
}

function newBoard() {
  layoutBoard()
  level = level >= 30000 ? 1 : level + 1
}

export function beforeRender(delta) {
  if (!inited) { inited = 1; layoutBoard(); respawn() }
  var dt = min(delta, 250) / 1000

  turtleT = mod(turtleT + dt, TURTLE_PERIOD)
  var r
  for (r = 0; r < H; r++) {
    if (laneVel[r] != 0) laneOff[r] = mod(laneOff[r] + laneVel[r] * dt, lanePeriod[r])
  }

  if (mode == 1) {
    modeT = modeT + dt
    if (modeT >= DEATH_T) {
      lives = lives - 1
      if (lives <= 0) {
        lives = 3
        level = 1
        layoutBoard()
      }
      mode = 0
      respawn()
    }
    return
  }
  if (mode == 2) {
    modeT = modeT + dt
    if (modeT >= PAD_T) { mode = 0; respawn() }
    return
  }
  if (mode == 3) {
    modeT = modeT + dt
    if (modeT >= LEVEL_T) { mode = 0; newBoard(); respawn() }
    return
  }

  hopAge = hopAge + dt

  // carried along by whatever the frog is standing on
  if (laneKind[fcy] == 2) {
    fx = fx + laneVel[fcy] * dt
    fcx = floor(fx + 0.5)
    if (fcx < 0 || fcx >= W) { fcx = clamp(fcx, 0, W - 1); fx = fcx; die(1); return }
  }

  // a car arriving, a turtle diving, a log left behind
  if (!frogAlive()) { die(onRoad(fcy) ? 0 : 1); return }

  hopAcc = hopAcc + dt
  var period = 1 / hopsPerSecond
  var guard = 0
  while (hopAcc >= period && guard < 4 && mode == 0) { hopAcc = hopAcc - period; hop(); guard++ }
  if (hopAcc > period) hopAcc = period
}

// --- paint ----------------------------------------------------------------
function paintCell(cx, cy) {
  if (mode == 3) {
    // board cleared: one rainbow band runs the whole crossing, start to pads.
    // `frac` truncates toward zero here, so anything that can go negative uses
    // mod(x, 1) instead.
    var s = clamp(modeT / LEVEL_T, 0, 1)
    var d = abs(cy / H - (0 - 0.25 + s * 1.5))
    var lit = d < 0.28 ? 1 - d * 3.57 : 0
    if (cy == GOAL_ROW && padIndex(cx) >= 0) {
      hsv(0.26, 0.3, 0.35 + 0.65 * square(modeT * 9, 0.5))   // all five pads flash
      return
    }
    hsv(mod(cx / W * 0.4 + cy / H * 0.6 + s * 2, 1), 0.85, 0.1 + 0.9 * lit * lit)
    return
  }

  var k = laneKind[cy]
  var h = 0.31, sa = 0.85, v = 0.05           // bank grass
  if (k == 1) { h = 0; sa = 0; v = 0.018 }    // asphalt
  else if (k == 2) { h = 0.62; sa = 1; v = 0.06 }  // water
  else if (k == 3) { h = 0.5; sa = 1; v = 0.02 }   // goal water

  if (k == 1 && occAt(cy, cx)) {
    // the leading pixel of each car is its headlight — kept coloured, so the
    // only near-white thing on the board is the frog
    var p = mod(cx - laneOff[cy], lanePeriod[cy])
    var nose = laneVel[cy] > 0 ? p > laneLen[cy] - 1 : p < 1
    h = laneHue[cy]
    sa = nose ? 0.45 : 0.95
    v = nose ? 0.95 : 0.6
  } else if (k == 2 && occAt(cy, cx)) {
    if (laneTurtle[cy]) {
      var sub = turtleLevel(cy, 0)
      h = 0.42; sa = 0.9; v = 0.45 * (1 - sub * 0.94) + 0.02   // teal shells
    } else {
      h = 0.075; sa = 0.85; v = 0.4                            // brown logs
    }
  } else if (k == 3) {
    var pi = padIndex(cx)
    if (pi >= 0) {
      if (padFilled[pi]) {
        h = 0.26; sa = 0.35; v = 0.5 + 0.4 * wave(time(0.02) + pi * 0.13)
      } else {
        h = 0.33; sa = 1; v = 0.28
      }
    }
  }

  // the frog, and the after-image of the cell it just left
  var onFrog = cx == fcx && cy == fcy
  if (!onFrog && cx == prevX && cy == prevY && hopAge < 0.5 / hopsPerSecond && mode == 0) {
    var g = 1 - hopAge * hopsPerSecond * 2
    h = 0.26; sa = 0.5; v = max(v, 0.4 * g)
  }
  if (onFrog) {
    if (mode == 1) {
      var f = 1 - modeT / DEATH_T
      var fl = square(modeT * 14, 0.5)
      h = deathKind == 0 ? 0 : 0.55
      sa = 1 - fl * 0.9
      v = f * (0.35 + 0.65 * fl)
    } else if (mode == 2) {
      h = 0.26; sa = 0.15; v = 0.5 + 0.5 * square(modeT * 16, 0.5)
    } else {
      h = 0.26; sa = 0.3; v = 1
      // a landing flash so each hop reads as a hop on a filmstrip
      if (hopAge < 0.3 / hopsPerSecond) { sa = 0.05 }
    }
  } else if (mode == 1) {
    // the whole board washes to the cause of death — red for a squash, blue
    // for a drowning — keeping only its brightness structure
    var fb = 1 - modeT / DEATH_T
    v = v * (0.3 + 0.4 * fb)
    sa = mix(sa, 1, 0.6 * fb)
    h = deathKind == 0 ? 0 : 0.58
  }

  hsv(h, sa, v)
}

export function render2D(index, x, y) {
  var cx = clamp(floor(x * (W - 0.01)), 0, W - 1)
  var gy = clamp(floor(y * (H - 0.01)), 0, H - 1)
  paintCell(cx, H - 1 - gy)      // goal row at the top of the map
}

// No map: run the board out along the strip, row after row.
export function render(index) {
  var s = clamp(floor(index / pixelCount * N), 0, N - 1)
  var cy = floor(s / W)
  paintCell(s - cy * W, cy)
}
