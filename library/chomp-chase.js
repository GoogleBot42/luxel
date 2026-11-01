// name: Chomp Chase
// Curated original (hand-written showcase of the Luxel language/builtins); the dotted-line
// chase nods to the WLED "PacMan" strip effect — shapes and rules are our own, no game assets.
// A yellow chomper eats a line of dots around a looped track, four coloured ghosts on its
// tail, each with its own pace and plan. A power pellet turns them blue: the chomper wheels
// round and hunts them, eaten ghosts fly home as eyes. A cleared board flashes and refills,
// a caught chomper pops and respawns — it never stops. On a strip the track is the strip
// (the ends are a wrap-around tunnel); on a panel it is a walled loop round a ghost house
// with the score. renderBulk: fillRange actors on strips, keyed sprite blits on panels.

var lapS = 12           // seconds for the chomper to run one lap
//# min=4 max=40 step=1 default=12
export function sliderLapTimeS(v) { lapS = clamp(v, 2, 120) }

var frightS = 6         // how long a power pellet keeps the ghosts blue
//# min=2 max=12 step=0.5 default=6
export function sliderBlueTimeS(v) { frightS = clamp(v, 1, 30) }

export var score = 0
export var level = 1

var MAXP = 512
var pel = array(MAXP)                 // 0 eaten, 1 dot, 2 power pellet
var gp = array(4), gd = array(4), gs = array(4), gw = array(4)   // pos, dir, state, house wait
var GHUE = [0.0, 0.9, 0.5, 0.07]      // red, pink, cyan, orange
var GPACE = [0.86, 0.8, 0.75, 0.72]
var GS = array(8), GV = array(8), CV = array(12)   // sprite canvases (panel)
for (var k = 0; k < 8; k++) { GS[k] = array(36); GV[k] = array(36) }
for (k = 0; k < 12; k++) CV[k] = array(36)

var T = 60, sz = 2, gap = 3, P = 20, door = 30, panel = 0
var a = 2, W1 = 1, H1 = 1, cw = 0, ch = 0, seenN = -1, seenW = -1
var cp = 0, cd = 1, chomp = 0, deathT = 0, coolT = 0, frightT = 0, levelT = 0, popT = 9, popP = 0
var combo = 0, left = 0, readyT = 0, modeT = 0

// ---- layout -----------------------------------------------------------------
function layout() {
  seenN = pixelCount
  seenW = gridWidth()
  cw = gridWidth()
  ch = gridHeight()
  panel = has2DMap() && cw >= 12 && ch >= 12
  if (panel) {
    sz = clamp(floor(min(cw, ch) / 16), 2, 6)
    a = 2
    W1 = cw - 2 * a - sz
    H1 = ch - 2 * a - sz
    T = 2 * (W1 + H1)
    gap = sz + 1
    door = floor(W1 / 2)
    sprites()
  } else {
    T = pixelCount
    sz = clamp(round(T / 40), 2, 6)
    gap = max(round(sz * 0.8) + 1, ceil(T / MAXP))
    door = floor(T / 2)
  }
  P = min(floor(T / gap), MAXP)
  level = 1
  newBoard()
  respawn()
}

function newBoard() {
  left = 0
  for (var k = 0; k < P; k++) { pel[k] = 1; left++ }
  pel[floor(P / 4)] = 2
  pel[floor(P * 3 / 4)] = 2
}

function respawn() {
  cp = mod(door + T / 2, T)
  cd = random(1) < 0.5 ? 1 : -1
  deathT = 0
  frightT = 0
  readyT = 1
  for (var g = 0; g < 4; g++) { gs[g] = 3; gw[g] = 1 + g * 2.2; gp[g] = door; gd[g] = 1 }
}

// Ghost and chomper sprites for size sz: 2 skirt frames x 4 headings, 3 mouth frames x 4.
function sprites() {
  var c = (sz - 1) / 2, r2 = sz * sz / 4 + 0.3
  for (var f = 0; f < 2; f++) for (var d = 0; d < 4; d++) {
    var S = GS[f * 4 + d], V = GV[f * 4 + d]
    for (var y = 0; y < sz; y++) for (var x = 0; x < sz; x++) {
      var i = y * sz + x, on = 1
      if (sz >= 3 && y == 0 && (x == 0 || x == sz - 1)) on = 0
      if (sz >= 4 && y == sz - 1 && (x + f) % 2) on = 0
      S[i] = 1; V[i] = on
    }
    // eyes look where the ghost is heading
    var ey = sz >= 4 ? (d == 1 ? 2 : d == 3 ? 0 : 1) : 0
    var ex = d == 0 ? 1 : d == 2 ? 0 : (sz >= 5 ? 1 : 0)
    if (sz < 4) eye(S, V, d == 2 ? 0 : sz - 1, sz == 3 ? 1 : 0)
    else { eye(S, V, ex, ey); eye(S, V, ex + (sz >= 5 ? 3 : 2), ey) }
  }
  var dx = [1, 0, -1, 0], dy = [0, 1, 0, -1]
  for (var m = 0; m < 3; m++) for (d = 0; d < 4; d++) {
    var Cv = CV[m * 4 + d], half = m * 0.45
    for (y = 0; y < sz; y++) for (x = 0; x < sz; x++) {
      var px = x - c, py = y - c, in = px * px + py * py <= r2
      var along = px * dx[d] + py * dy[d], side = abs(px * dy[d] - py * dx[d])
      if (m > 0 && along > 0 && side <= along * half + 0.25) in = 0   // the mouth wedge
      Cv[y * sz + x] = in ? 1 : 0
    }
  }
}
function eye(S, V, x, y) {
  if (x < 0 || x >= sz || y < 0 || y >= sz) return
  S[y * sz + x] = 0; V[y * sz + x] = 1
}

// ---- simulation ---------------------------------------------------------------
function wd(from, to) { return mod(to - from + T / 2, T) - T / 2 }   // signed shortest offset

function eatAt(p, tol) {
  var k = mod(round(p / gap), P)
  if (pel[k] && abs(wd(p, k * gap)) <= tol) {
    if (pel[k] == 2) {
      frightT = frightS
      combo = 0
      for (var g = 0; g < 4; g++) if (gs[g] <= 1) { gs[g] = 1; gd[g] = -gd[g] }
      score += 50
    } else score += 10
    pel[k] = 0
    left--
    if (score >= 30000) score -= 30000
  }
}

export function beforeRender(delta) {
  if (pixelCount != seenN || gridWidth() != seenW) layout()
  var dt = min(delta, 50) / 1000
  var v = T / lapS
  chomp = mod(chomp + dt * 7, 1)
  popT += dt
  if (levelT > 0) {                       // board cleared: flash, then refill
    levelT -= dt
    if (levelT <= 0) { newBoard(); respawn(); level = level >= 30000 ? 1 : level + 1 }
    return
  }
  if (deathT > 0) {                       // caught: the pop plays out, then a fresh start
    deathT -= dt
    if (deathT <= 0) respawn()
    return
  }
  if (readyT > 0) { readyT -= dt; return }

  frightT = max(0, frightT - dt)
  coolT = max(0, coolT - dt)
  if (frightT <= 0) modeT = mod(modeT + dt, 13)
  var scatter = modeT >= 8

  // chomper: hunt blue ghosts; otherwise run on, turning back from a ghost ahead
  var near = T, g, o
  if (frightT > 0) {
    for (g = 0; g < 4; g++) if (gs[g] == 1) {
      o = wd(cp, gp[g])
      if (abs(o) < abs(near)) near = o
    }
    if (near < T && coolT <= 0 && near * cd < 0) { cd = -cd; coolT = 0.4 }
  } else if (coolT <= 0) {
    var ahead = T, behind = T
    for (g = 0; g < 4; g++) if (gs[g] == 0) {
      o = wd(cp, gp[g]) * cd
      if (o > 0) ahead = min(ahead, o)
      else behind = min(behind, -o)
    }
    if (ahead < T * 0.2 && behind > ahead + sz * 2) { cd = -cd; coolT = 1.2 }
  }
  var step = v * dt
  cp = mod(cp + cd * step, T)
  eatAt(cp, step + 0.5)
  if (left <= 0) { levelT = 1.6; return }

  // ghosts
  var pace = min(0.15, level * 0.015)    // ghosts quicken a little every level
  for (g = 0; g < 4; g++) {
    var s = gs[g]
    if (s == 3) {                         // waiting in the house
      gw[g] -= dt
      if (gw[g] <= 0 && frightT <= 0) { gs[g] = 0; gp[g] = door }
      continue
    }
    var target = cp, spd = v * min(0.97, GPACE[g] + pace)
    if (s == 0) {
      // chase 8 s, then 5 s of scatter to each ghost's own quarter of the track
      if (scatter || (g == 3 && abs(wd(gp[g], cp)) < T * 0.15)) target = door + (g + 0.5) * T / 4
      else if (g == 1) target = cp + cd * sz * 4            // pink heads you off
      else if (g == 2) target = 2 * cp - gp[0]              // cyan closes the other side of red
      var to = wd(gp[g], target)
      // past the far side keep going; near a scatter post pace back and forth
      if (abs(to) < T * 0.42 && abs(to) > T * 0.05) gd[g] = to >= 0 ? 1 : -1
    } else if (s == 1) {
      spd = v * 0.5
      gd[g] = wd(gp[g], cp) >= 0 ? -1 : 1                   // flee
      if (frightT <= 0) gs[g] = 0
    } else {
      spd = v * 2.5                                         // eyes race home
      var h = wd(gp[g], door)
      gd[g] = h >= 0 ? 1 : -1
      if (abs(h) <= spd * dt + 0.5) { gs[g] = 3; gw[g] = 1; gp[g] = door; continue }
    }
    gp[g] = mod(gp[g] + gd[g] * spd * dt, T)
    if (abs(wd(gp[g], cp)) < sz * 0.75) {
      if (gs[g] == 1) {
        gs[g] = 2
        combo++
        score += 100 * min(pow(2, combo), 16)
        if (score >= 30000) score -= 30000
        popT = 0
        popP = gp[g]
      } else if (gs[g] == 0) {
        deathT = 1.6
        popT = 0
        popP = cp
        return
      }
    }
  }
}

// ---- drawing ----------------------------------------------------------------
// Track position -> top-left cell of an sz x sz actor on the panel loop.
var tx = 0, ty = 0
function cell(p) {
  p = mod(floor(p), T)
  if (p < W1) { tx = a + p; ty = a }
  else if (p < W1 + H1) { tx = a + W1; ty = a + p - W1 }
  else if (p < 2 * W1 + H1) { tx = a + W1 - (p - W1 - H1); ty = a + H1 }
  else { tx = a; ty = a + H1 - (p - 2 * W1 - H1) }
}
// Screen heading (0 right, 1 down, 2 left, 3 up) of motion along the loop.
function heading(p, d) {
  p = mod(floor(p), T)
  var seg = p < W1 ? 0 : p < W1 + H1 ? 1 : p < 2 * W1 + H1 ? 2 : 3
  return d > 0 ? seg : (seg + 2) % 4
}
function box(x0, y0, x1, y1) {
  var sx = 1 / (cw - 1), sy = 1 / (ch - 1)
  fillRect((x0 - 0.4) * sx, (y0 - 0.4) * sy, (x1 + 0.4) * sx, (y1 + 0.4) * sy)
}
function frame(x0, y0, x1, y1) {
  box(x0, y0, x1, y0); box(x0, y1, x1, y1); box(x0, y0, x0, y1); box(x1, y0, x1, y1)
}

function ghostLook(g) {
  if (gs[g] == 1) {
    var white = frightT < 1.5 && square(frightT * 4, 0.5)
    return white ? -1 : 0.64
  }
  return GHUE[g]
}

function drawPanel() {
  clear()
  var flash = levelT > 0 ? square(levelT * 3, 0.5) : 0
  hsv(0.64, 1 - flash * 0.9, 0.6 + flash * 0.4)
  frame(a - 1, a - 1, cw - a, ch - a)                     // outer wall
  frame(a + sz, a + sz, cw - 1 - a - sz, ch - 1 - a - sz) // inner wall
  // ghost house with its pink door, and the score under it
  var hx0 = floor(cw / 2) - sz * 2 - 1, hx1 = floor(cw / 2) + sz * 2
  var hy0 = floor(ch / 2) - sz, hy1 = hy0 + sz + 3
  hsv(0.64, 1, 0.6)
  frame(hx0, hy0, hx1, hy1)
  if (min(cw, ch) >= 48) {                               // a few maze blocks on big panels
    var L0 = a + sz + 4, R0 = cw - 5 - a - sz, T0 = a + sz + 4, B0 = ch - 5 - a - sz, mx = floor(cw / 2)
    frame(L0, T0, mx - 3, T0 + 2); frame(mx + 2, T0, R0, T0 + 2)
    frame(L0, B0 - 2, mx - 3, B0); frame(mx + 2, B0 - 2, R0, B0)
    frame(L0, hy0, L0 + 2, hy1); frame(R0 - 2, hy0, R0, hy1)
  }
  hsv(0.92, 0.5, 0.8)
  box(floor(cw / 2) - 1, hy0, floor(cw / 2), hy0)
  var k, g, o = floor((sz - 1) / 2)
  // dots and blinking power pellets
  rgb(0.75, 0.6, 0.45)
  for (k = 0; k < P; k++) {
    if (pel[k] != 1) continue
    cell(k * gap)
    if (sz >= 6) box(tx + o, ty + o, tx + o + 1, ty + o + 1)
    else setPixel(gridIndex(tx + o, ty + o))
  }
  if (square(chomp * 2, 0.6)) {
    rgb(1, 0.75, 0.55)
    for (k = 0; k < P; k++) if (pel[k] == 2) {
      cell(k * gap)
      box(tx + 1, ty + 1, tx + sz - 2, ty + sz - 2)
      if (sz < 4) setPixel(gridIndex(tx + o, ty + o))
    }
  }
  // ghosts: in the house, on the loop, or just a pair of eyes
  var skirt = floor(chomp * 2)
  for (g = 0; g < 4; g++) {
    if (deathT > 0 && deathT < 1.1) continue
    var gx, gy, hd = 0
    if (gs[g] == 3) { gx = hx0 + 1 + g * sz; gy = hy0 + 2 }
    else { cell(gp[g]); gx = tx; gy = ty; hd = heading(gp[g], gd[g]) }
    var spr = skirt * 4 + hd, lk = ghostLook(g)
    if (gs[g] == 2) { rgb(1, 1, 1); setPixel(gridIndex(gx + o, gy + o)); setPixel(gridIndex(gx + o + 1, gy + o)) }
    else if (lk < 0) blit(0, 0, GV[spr], sz, sz, gx, gy, 3)
    else blit(lk, GS[spr], GV[spr], sz, sz, gx, gy, 3)
  }
  // the chomper: mouth working while it runs; caught, it gapes and shrinks away
  cell(cp)
  var m = readyT > 0 ? 0 : floor(triangle(chomp) * 2.99)
  if (deathT > 0) m = 2
  var hdc = heading(cp, cd)
  if (deathT > 1.1 || deathT <= 0) blit(0.15, 1, CV[m * 4 + hdc], sz, sz, tx, ty, 3)
  if (popT < 0.5) {                                   // a ring of light on a catch
    var pc = popP
    cell(pc)
    hsv(deathT > 0 ? 0.15 : 0.55, 0.4, 1 - popT * 2)
    splat((tx + sz / 2) / (cw - 1), (ty + sz / 2) / (ch - 1), (sz + popT * 16) / cw, 1)
  }
  // score, centred under the house
  font("tiny")
  rgb(0.9, 0.75, 0.2)
  var digits = score >= 10000 ? 5 : score >= 1000 ? 4 : score >= 100 ? 3 : score >= 10 ? 2 : 1
  if (hy1 + 8 < ch - a - sz - 1) drawNumber(score, floor(cw / 2) - digits * 2, hy1 + 2, 1, 0)
}

// wrap-aware run on the strip
function run(p, n) {
  p = mod(round(p), T)
  fillRange(p, p + n)
  if (p + n > T) fillRange(0, p + n - T)
}

function drawStrip() {
  clear()
  if (levelT > 0) { hsv(0.64, 0.8, square(levelT * 3, 0.5) * 0.25); fill() }
  hsv(0.92, 0.5, 0.2)
  setPixel(door)
  var k, g, o = floor((sz - 1) / 2)
  rgb(0.4, 0.3, 0.2)
  for (k = 0; k < P; k++) if (pel[k] == 1) setPixel(mod(k * gap + o, T))
  hsv(0.07, 0.45, square(chomp * 2, 0.6) ? 1 : 0.35)
  for (k = 0; k < P; k++) if (pel[k] == 2) run(k * gap, sz)
  for (g = 0; g < 4; g++) {
    if (gs[g] == 3 || (deathT > 0 && deathT < 1.1)) continue
    var lead = gd[g] > 0 ? sz - 1 : 0
    if (gs[g] == 2) { rgb(0.7, 0.7, 0.8); run(gp[g] + lead, 1); continue }
    var lk = ghostLook(g)
    if (lk < 0) rgb(1, 1, 1)
    else hsv(lk, 1, 1)
    run(gp[g], sz)
    rgb(1, 1, 1)
    run(gp[g] + lead, 1)                                // the eye, looking ahead
  }
  if (deathT <= 0 || deathT > 1.1) {
    var lit = deathT > 0 ? sz * (deathT - 1.1) * 2 : sz
    hsv(0.15, 1, 1)
    run(cp + (sz - lit) / 2, lit)
    hsv(0.15, 1, readyT > 0 ? 1 : 0.15 + 0.85 * triangle(chomp))   // the mouth opens and shuts
    run(cp + (cd > 0 ? sz - 1 : 0), 1)
  }
  if (popT < 0.5) {
    hsv(deathT > 0 ? 0.15 : 0.55, 0.3, 1 - popT * 2)
    var r = round(popT * sz * 6)
    setPixel(mod(round(popP) - r, T))
    setPixel(mod(round(popP) + sz + r, T))
  }
}

// Mapless strips stay strips: this export suppresses the default grid.
export function render(index) { hsv(0, 0, 0) }

export function renderBulk() {
  if (panel) drawPanel()
  else drawStrip()
}
