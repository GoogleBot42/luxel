// name: Pong Clock
// Curated original (hand-written showcase of the Luxel language/builtins).
// Two paddles rally forever and the score is the time: hours on the left,
// minutes on the right. When the minute turns, the hour paddle lets the ball
// slip past so the minute side scores; on the hour the minute paddle misses.
// The paddles aim for the predicted intercept with a little lag; phosphor
// persistence (fade) and setGlow give it a CRT look. A few fillRects a frame.

var look = 0
var h24 = 0

// 0 white, 1 green phosphor, 2 amber, 3 neon
//# min=0 max=3 step=1 default=0
export function sliderLook(v) { look = clamp(floor(v), 0, 3) }

//# default=0
export function toggle24Hour(v) { h24 = v }

var gw = 32, gh = 32, kx = 1 / 31, ky = 1 / 31, s = 1, ph = 5
var bx = 15, by = 15, vx = 30, vy = 12, serveT = 0.8
var lY = 16, rY = 16, lAim = 0, rAim = 0
var dispH = -1, dispM = -1, missSide = 0, flashL = 0, flashR = 0
var seenW = -1, seenH = -1, fk = 0.3

function layout() {
  seenW = gridWidth(); seenH = gridHeight()
  gw = seenW; gh = seenH
  if (gw == 0) { gw = 32; gh = 32 }
  kx = 1 / (gw - 1); ky = 1 / (gh - 1)
  s = max(1, floor(min(gw, gh) / 32))
  ph = max(4, round(gh / 6))
  lY = gh / 2; rY = gh / 2
  serve(1)
}

function serve(dir) {
  bx = (gw - s) / 2; by = gh * (0.3 + random(0.4))
  vx = dir * gw / 1.15
  vy = (random(2) - 1) * gw * 0.45
  serveT = 0.7
}

// where the ball's top will be when it reaches column x, folded off the walls
function intercept(x) {
  var L = gh - s, yy = by + vy * (x - bx) / vx
  yy = mod(yy, 2 * L)
  return yy > L ? 2 * L - yy : yy
}

function steer(py, toward, px, aim, miss, dt) {
  var target = gh / 2
  if (toward) {
    target = intercept(px) + s / 2 + aim
    if (miss) target += (target < gh / 2 ? 1 : -1) * (ph / 2 + 2.5 * s)
  }
  var step = (target - py) * min(1, dt * (toward ? 9 : 2))
  var cap = gh * 1.7 * dt
  return clamp(py + clamp(step, -cap, cap), ph / 2, gh - ph / 2)
}

function hourShown(h) { return h24 ? h : (h % 12 == 0 ? 12 : h % 12) }

export function beforeRender(delta) {
  if (gridWidth() != seenW || gridHeight() != seenH) layout()
  var dt = min(delta, 100) / 1000
  fk = pow(0.3, min(delta, 200) / 33)     // phosphor persistence, frame-rate independent
  var h = clockHour(), m = clockMinute()
  if (dispH < 0) { dispH = h; dispM = m }
  if (missSide == 0 && (h != dispH || m != dispM)) missSide = m == 0 || h != dispH ? 2 : 1
  flashL = max(0, flashL - dt); flashR = max(0, flashR - dt)

  lY = steer(lY, vx < 0, 2 * s, lAim, missSide == 1, dt)
  rY = steer(rY, vx > 0, gw - 2 * s - s, rAim, missSide == 2, dt)
  if (serveT > 0) { serveT -= dt; return }

  bx += vx * dt; by += vy * dt
  if (by < 0) { by = -by; vy = -vy }
  if (by > gh - s) { by = 2 * (gh - s) - by; vy = -vy }
  var spd = min(abs(vx) * 1.05, gw / 0.55)
  if (vx < 0 && bx <= 2 * s && bx > 0 && by + s > lY - ph / 2 && by < lY + ph / 2) {
    bx = 2 * s; vx = spd
    vy = (by + s / 2 - lY) / (ph / 2) * spd * 0.8
    rAim = (random(2) - 1) * ph * 0.35
  }
  if (vx > 0 && bx + s >= gw - 2 * s && bx + s < gw && by + s > rY - ph / 2 && by < rY + ph / 2) {
    bx = gw - 3 * s; vx = -spd
    vy = (by + s / 2 - rY) / (ph / 2) * spd * 0.8
    lAim = (random(2) - 1) * ph * 0.35
  }
  if (bx < -3 * s || bx > gw + 2 * s) {      // a point: the scoreboard catches up with the clock
    var leftScored = bx > 0
    if (missSide) { dispH = h; dispM = m; missSide = 0 }
    if (leftScored) flashL = 1.2
    else flashR = 1.2
    serve(leftScored ? 1 : -1)
  }
}

function rect(x0, y0, w, hgt) { fillRect((x0 - 0.4) * kx, (y0 - 0.4) * ky, (x0 + w - 0.6) * kx, (y0 + hgt - 0.6) * ky) }

// role: 0 field/line, 1 left, 2 right, 3 ball
function ink(role, v) {
  if (look == 1) hsv(0.33, 0.75, v)
  else if (look == 2) hsv(0.085, 0.9, v)
  else if (look == 3) hsv(role == 1 ? 0.9 : role == 2 ? 0.5 : role == 3 ? 0.15 : 0.75, role == 0 ? 0.6 : 0.8, v)
  else hsv(0.6, 0.05, v)
}

export function renderBulk() {
  fade(fk)
  setGlow(0.3)
  var i
  ink(0, 0.3)                                 // dashed net
  for (i = 0; i < gh; i += 2 * s) rect(floor((gw - s) / 2), i, s, s)
  if (gw >= 48) font("regular")
  else font("tiny")
  var cw = textWidth("0"), tw = 2 * cw - 1, ty = 2 * s
  ink(1, 0.55 + 0.45 * min(1, flashL))
  drawNumber(hourShown(dispH), round(gw / 4 - tw / 2), ty, 2, 0)
  ink(2, 0.55 + 0.45 * min(1, flashR))
  drawNumber(dispM, round(gw * 3 / 4 - tw / 2), ty, 2, 0)
  ink(1, 1); rect(s, round(lY - ph / 2), s, ph)
  ink(2, 1); rect(gw - 2 * s, round(rY - ph / 2), s, ph)
  if (serveT <= 0 || mod(serveT, 0.3) < 0.15) { ink(3, 1); rect(round(bx), round(by), s, s) }
}
