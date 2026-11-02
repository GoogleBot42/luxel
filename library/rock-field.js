// name: Rock Field
// tags: panel, game, retro
// Curated original (hand-written showcase of the Luxel language/builtins).
// Attract-mode vector arcade: jagged rocks tumble across a wrap-around field while a little
// triangle ship turns, leads its shots at the nearest rock and fires; hits split big rocks
// into two smaller ones and the smallest burst into sparks. When a rock comes too close the
// ship wheels away and burns its flickering engine; a collision blows it apart and it
// respawns once the middle is clear. Waves refill when the field is empty. renderBulk: a
// delta-timed fade() is the phosphor afterglow, every shape is a handful of drawLines.

var startRocks = 4      // big rocks per wave
//# min=1 max=8 step=1 default=4
export function sliderRocks(v) { startRocks = clamp(floor(v), 1, 8) }

var glowMs = 110        // phosphor afterglow half-life
//# min=20 max=400 step=10 default=110
export function sliderAfterglowMs(v) { glowMs = clamp(v, 10, 2000) }

// 0 neon (a hue per rock), 1 white vector, 2 green phosphor
var style = 0
//# min=0 max=2 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 2) }

export var score = 0
export var wave = 0

var MAXR = 16, MAXS = 8, MAXP = 32
var rx = array(MAXR), ry = array(MAXR), rvx = array(MAXR), rvy = array(MAXR)
var ra = array(MAXR), rs = array(MAXR), rl = array(MAXR), rk = array(MAXR)   // angle, spin, size level, seed
var sx_ = array(MAXS), sy_ = array(MAXS), svx = array(MAXS), svy = array(MAXS), sl = array(MAXS)
var px = array(MAXP), py = array(MAXP), pvx = array(MAXP), pvy = array(MAXP), pl = array(MAXP), ph = array(MAXP)
var RAD = [0.13, 0.075, 0.042], SIDES = [8, 7, 5]

var gw = -1, gh = -1, FW = 32, FH = 32, U = 32, nx = 1, ny = 1, lw = 0.02
var nRocks = 0, shx = 16, shy = 16, svxS = 0, svyS = 0, ang = 0, alive = 1, deadT = 0, shield = 0
var cool = 0, thrust = 0, cruiseT = 2, clock = 0, quiet = 0, fadeK = 0.8

function layout() {
  gw = gridWidth()
  gh = gridHeight()
  FW = gw > 0 ? gw : 32
  FH = gw > 0 ? gh : 32
  U = min(FW, FH)
  nx = 1 / max(FW - 1, 1)
  ny = 1 / max(FH - 1, 1)
  lw = max(0.75, U / 80) * (nx + ny) / 2
  nRocks = 0
  wave = 0
  newWave()
  spawnShip()
}

function addRock(x, y, lv, vx, vy) {
  if (nRocks >= MAXR) return
  var k = nRocks++
  rx[k] = x; ry[k] = y; rl[k] = lv; rvx[k] = vx; rvy[k] = vy
  ra[k] = random(PI2); rs[k] = (random(2) - 1) * (0.6 + lv * 0.5); rk[k] = random(100)
}

function newWave() {
  wave = wave >= 30000 ? 1 : wave + 1
  quiet = 0
  for (var k = 0; k < startRocks; k++) {
    var a = random(PI2), sp = U * (0.05 + random(0.05)) * min(1 + wave * 0.06, 1.6)
    var edge = random(1) < 0.5
    addRock(edge ? 0 : random(FW), edge ? random(FH) : 0, 0, cos(a) * sp, sin(a) * sp)
  }
}

function spawnShip() {
  shx = FW / 2; shy = FH / 2; svxS = 0; svyS = 0
  alive = 1; shield = 1.5
}

function wrapD(d, span) { return mod(d + span / 2, span) - span / 2 }

function burst(x, y, n, hue, sp) {
  for (var k = 0; k < MAXP && n > 0; k++) {
    if (pl[k] > 0) continue
    var a = random(PI2), s = U * sp * (0.4 + random(0.6))
    px[k] = x; py[k] = y; pvx[k] = cos(a) * s; pvy[k] = sin(a) * s
    pl[k] = 0.5 + random(0.5); ph[k] = hue
    n--
  }
}

function rockHue(k) {
  if (style == 1) return 0.62
  if (style == 2) return 0.36
  return mod(rk[k] * 0.137, 1)
}

function hitRock(k) {
  var lv = rl[k], x = rx[k], y = ry[k]
  score += lv == 0 ? 20 : lv == 1 ? 50 : 100
  if (score >= 30000) score -= 30000
  quiet = 0
  burst(x, y, 7 - lv * 2, rockHue(k), 0.25)
  var vx = rvx[k], vy = rvy[k]
  // swap-remove, then the two halves fly apart across the old heading
  nRocks--
  rx[k] = rx[nRocks]; ry[k] = ry[nRocks]; rvx[k] = rvx[nRocks]; rvy[k] = rvy[nRocks]
  ra[k] = ra[nRocks]; rs[k] = rs[nRocks]; rl[k] = rl[nRocks]; rk[k] = rk[nRocks]
  if (lv < 2) {
    var kick = U * (0.05 + random(0.05)), a = atan2(vy, vx) + PI / 2
    addRock(x, y, lv + 1, vx * 1.2 + cos(a) * kick, vy * 1.2 + sin(a) * kick)
    addRock(x, y, lv + 1, vx * 1.2 - cos(a) * kick, vy * 1.2 - sin(a) * kick)
  }
}

export function beforeRender(delta) {
  if (gridWidth() != gw || gridHeight() != gh) layout()
  var dt = min(delta, 50) / 1000, k, j
  clock += dt
  quiet += dt
  fadeK = pow(0.5, min(delta, 200) / glowMs)

  for (k = 0; k < nRocks; k++) {
    rx[k] = mod(rx[k] + rvx[k] * dt, FW)
    ry[k] = mod(ry[k] + rvy[k] * dt, FH)
    ra[k] += rs[k] * dt
  }
  if (nRocks == 0 || quiet > 30) newWave()

  // ship brain: aim ahead of the nearest rock and fire; dodge anything too close
  thrust = 0
  if (alive) {
    var best = 30000, bdx = 0, bdy = 0, bk = -1
    for (k = 0; k < nRocks; k++) {
      var dx = wrapD(rx[k] - shx, FW), dy = wrapD(ry[k] - shy, FH)
      var d = sqrt(dx * dx + dy * dy) - RAD[rl[k]] * U
      if (d < best) { best = d; bdx = dx; bdy = dy; bk = k }
    }
    var want = ang, danger = bk >= 0 && best < U * 0.14 && shield <= 0
    if (bk >= 0) {
      var tt = sqrt(bdx * bdx + bdy * bdy) / (U * 1.1)
      want = danger ? atan2(-bdy, -bdx) : atan2(bdy + rvy[bk] * tt, bdx + rvx[bk] * tt)
    }
    var err = mod(want - ang + PI, PI2) - PI
    ang += clamp(err, -3.5 * dt, 3.5 * dt)
    cool -= dt
    if (!danger && bk >= 0 && abs(err) < 0.14 && cool <= 0) {
      for (j = 0; j < MAXS; j++) if (sl[j] <= 0) {
        sx_[j] = shx + cos(ang) * U * 0.05; sy_[j] = shy + sin(ang) * U * 0.05
        svx[j] = svxS + cos(ang) * U * 1.1; svy[j] = svyS + sin(ang) * U * 1.1
        sl[j] = 0.8
        cool = 0.45
        break
      }
    }
    cruiseT -= dt
    if (cruiseT < -0.5) cruiseT = 2 + random(3)
    if ((danger && abs(err) < 0.7) || (cruiseT < 0 && !danger)) thrust = 1
    var acc = U * (danger ? 0.9 : 0.35) * thrust
    svxS = (svxS + cos(ang) * acc * dt) * pow(0.55, dt)
    svyS = (svyS + sin(ang) * acc * dt) * pow(0.55, dt)
    shx = mod(shx + svxS * dt, FW)
    shy = mod(shy + svyS * dt, FH)
    shield = max(0, shield - dt)
    if (shield <= 0 && bk >= 0 && best < U * 0.035) {        // collision: the ship blows apart
      alive = 0
      deadT = 0
      burst(shx, shy, 12, 0.1, 0.35)
    }
  } else {
    deadT += dt
    var safe = 1
    for (k = 0; k < nRocks; k++) {
      var cx = wrapD(rx[k] - FW / 2, FW), cy = wrapD(ry[k] - FH / 2, FH)
      if (cx * cx + cy * cy < U * U * 0.06) safe = 0
    }
    if (deadT > 2 && (safe || deadT > 6)) spawnShip()
  }

  // shots
  for (j = 0; j < MAXS; j++) {
    if (sl[j] <= 0) continue
    sl[j] -= dt
    sx_[j] = mod(sx_[j] + svx[j] * dt, FW)
    sy_[j] = mod(sy_[j] + svy[j] * dt, FH)
    for (k = 0; k < nRocks; k++) {
      var ex = wrapD(rx[k] - sx_[j], FW), ey = wrapD(ry[k] - sy_[j], FH), rr = RAD[rl[k]] * U
      if (ex * ex + ey * ey < rr * rr) { hitRock(k); sl[j] = 0; break }
    }
  }
  for (k = 0; k < MAXP; k++) {
    if (pl[k] <= 0) continue
    pl[k] -= dt
    px[k] += pvx[k] * dt
    py[k] += pvy[k] * dt
  }
}

// One segment in field cells, drawn with the brush at the phosphor width.
function seg(x0, y0, x1, y1) { drawLine(x0 * nx, y0 * ny, x1 * nx, y1 * ny, lw, 2) }

function rock(k, ox, oy) {
  var n = SIDES[rl[k]], r = RAD[rl[k]] * U, x = rx[k] + ox, y = ry[k] + oy
  var lx = 0, ly = 0
  for (var i = 0; i <= n; i++) {
    var j = i % n, a = ra[k] + j * PI2 / n, rr = r * (0.72 + 0.4 * hash2(rk[k], j))
    var vx = x + cos(a) * rr, vy = y + sin(a) * rr
    if (i > 0) seg(lx, ly, vx, vy)
    lx = vx; ly = vy
  }
}

export function renderBulk() {
  fade(fadeK)
  var k, ox, oy
  for (k = 0; k < nRocks; k++) {
    var r = RAD[rl[k]] * U
    if (style == 1) rgb(0.75, 0.8, 1)
    else oklch(style == 2 ? 0.8 : 0.78, style == 2 ? 0.2 : 0.11, rockHue(k))
    var wx = rx[k] < r ? FW : rx[k] > FW - r ? -FW : 0
    var wy = ry[k] < r ? FH : ry[k] > FH - r ? -FH : 0
    rock(k, 0, 0)
    if (wx) rock(k, wx, 0)
    if (wy) rock(k, 0, wy)
    if (wx && wy) rock(k, wx, wy)
  }
  // shots: short hot streaks
  if (style == 2) oklch(0.95, 0.2, 0.36)
  else rgb(1, 0.95, 0.6)
  for (k = 0; k < MAXS; k++) if (sl[k] > 0) {
    seg(sx_[k], sy_[k], sx_[k] - svx[k] * 0.02, sy_[k] - svy[k] * 0.02)
  }
  // sparks
  for (k = 0; k < MAXP; k++) if (pl[k] > 0) {
    if (style == 2) oklch(0.5 + pl[k] * 0.45, 0.2, 0.36)
    else oklch(0.5 + pl[k] * 0.45, 0.16, ph[k] + (1 - pl[k]) * 0.08)
    splat(px[k] * nx, py[k] * ny, lw * 1.4, 1)
  }
  // the ship: a notched triangle, blinking while shielded, flame flickering behind
  if (alive && (shield <= 0 || square(clock * 8, 0.6))) {
    var c = cos(ang), s = sin(ang), z = U * 0.055
    var ax = shx + c * z * 1.3, ay = shy + s * z * 1.3
    var lx = shx + (-c * 0.8 - s * 0.75) * z, ly = shy + (-s * 0.8 + c * 0.75) * z
    var qx = shx + (-c * 0.8 + s * 0.75) * z, qy = shy + (-s * 0.8 - c * 0.75) * z
    var bx = shx - c * 0.45 * z, by = shy - s * 0.45 * z
    if (style == 2) oklch(0.95, 0.15, 0.36)
    else rgb(0.85, 1, 1)
    seg(ax, ay, lx, ly)
    seg(ax, ay, qx, qy)
    seg(lx, ly, bx, by)
    seg(qx, qy, bx, by)
    if (thrust && random(1) < 0.8) {
      oklch(0.8, 0.18, 0.12)
      var fl = z * (0.6 + random(0.6))
      seg(bx, by, bx - c * fl, by - s * fl)
    }
  }
}
