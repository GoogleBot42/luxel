// name: Starfield
// tags: both, nature, ambient
// Curated original (hand-written showcase of the Luxel language/builtins).
// Three skies from one star pool. Warp: stars stream at you through a
// perspective divide, each drawn as a drawLine from last frame's projection to
// this one, so speed becomes streak length; the vanishing point banks slowly.
// Drift: a calm twinkling night sky over a deep-blue gradient, with the odd
// shooting star — the base layer for night scenes. Trails: a long exposure —
// stars wheel round an off-centre pole, laying arcs onto the persistent frame
// behind a tree line, until the plate fades and a new exposure starts.

var mode = 0              // 0 warp, 1 drift, 2 trails
//# min=0 max=2 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 2); restart() }

// Warp speed; in Drift it is how fast the sky pans, in Trails how fast it turns.
var warpPct = 40
//# min=0 max=100 step=1 default=40
export function sliderWarpPercent(v) { warpPct = clamp(v, 0, 100) }

var density = 20          // stars per 1000 pixels (capped at 160 stars)
//# min=5 max=60 step=1 default=20
export function sliderStarsPer1000Px(v) { density = clamp(v, 5, 60) }

// Hue the faint stars lean toward; bright ones stay near white.
var tintDeg = 215
//# min=0 max=360 step=1 default=215
export function sliderTintDegrees(v) { tintDeg = clamp(v, 0, 360) }

export function render(index) { hsv(0, 0, 0) }

var MAXS = 160
var sx = array(MAXS), sy = array(MAXS), sz = array(MAXS), sh = array(MAXS)
var px = array(MAXS), py = array(MAXS)
var n = 20, t = 0, dt = 0, cx = 0.5, cy = 0.5, pw = 1 / 31
var expo = 0, pole = 0.62, poleX = 0.35, poleY = 0.3, ang = 0, lastMode = -1
var mOn = 0, mx = 0, my = 0, mvx = 0, mvy = 0, mLife = 0

function seedStar(i, far) {
  sx[i] = random(2) - 1
  sy[i] = random(2) - 1
  sz[i] = far ? 1 : 0.08 + random(0.92)
  sh[i] = random(1)
  px[i] = -9
}
function restart() {
  for (var i = 0; i < MAXS; i++) seedStar(i, 0)
  expo = 0
  ang = random(PI2)
  poleX = 0.25 + random(0.5); poleY = 0.15 + random(0.25)
  lastMode = -1
}
restart()

function tintColor(v, white) {           // faint stars lean to the tint, bright ones whiten
  hsv(tintDeg / 360, 0.55 * (1 - white), v)
}
function starColor(i, v) {               // trails show true star colours: a few warm ones
  if (hash(i * 3.1) < 0.3) hsv(0.07 + 0.04 * hash(i), 0.55, v)
  else tintColor(v, sh[i])
}

export function beforeRender(delta) {
  dt = min(delta, 100) / 1000
  t = mod(t + dt, 3600)
  var w = has2DMap() ? gridWidth() : 0
  pw = 1 / max((w > 0 ? w : 32) - 1, 1)
  n = clamp(floor(density * (pixelCount / 1000)), 8, MAXS)   // pixelCount * 60 overflows 16.16
  var sp = warpPct / 100
  if (mode == 0) {
    cx = 0.5 + 0.16 * sin(t * 0.21); cy = 0.5 + 0.12 * sin(t * 0.13 + 1)
  } else if (mode == 2) {
    ang += dt * (0.01 + 0.06 * sp)
    expo += dt
  }
  if (mode == 1 || mode == 2) {          // a shooting star now and then
    if (mLife > 0) {
      mLife -= dt
      mx += mvx * dt; my += mvy * dt
    } else if (random(1) < dt / 7) {
      mLife = 0.5 + random(0.4)
      mx = 0.1 + random(0.8); my = random(0.35)
      var a = (random(1) < 0.5 ? 0.35 : PI - 0.35) + random(0.3)
      mvx = cos(a) * 0.9; mvy = sin(a) * 0.9
    }
  }
}

function warp() {
  var sp = warpPct / 100, vz = 0.08 + 1.6 * sp * sp + 0.3 * sp
  fade(pow(0.5 + 0.4 * sp, dt * 20))
  for (var i = 0; i < n; i++) {
    sz[i] -= vz * dt
    var x = cx + sx[i] / sz[i] * 0.5, y = cy + sy[i] / sz[i] * 0.5
    if (sz[i] < 0.04 || x < -0.1 || x > 1.1 || y < -0.1 || y > 1.1) { seedStar(i, 1); continue }
    var near = clamp(1.1 - sz[i], 0, 1), v = 0.08 + 0.92 * near
    if (v > 0.01) {
      tintColor(min(1, v * 1.2), near)
      if (px[i] < -1) splat(x, y, pw * (0.8 + near * 1.6), 2)
      else drawLine(px[i], py[i], x, y, pw * (0.6 + near * 1.4), 2)
    }
    px[i] = x; py[i] = y
  }
}

function drift() {
  var sp = warpPct / 100, off = mod(t * (0.002 + 0.03 * sp), 2)
  fillGradient(0.65, 0.9, 0.012, 0.62, 0.8, 0.065, 2)
  for (var i = 0; i < n; i++) {
    var x = mod(sx[i] * 0.5 + 0.5 + off, 1), y = sy[i] * 0.5 + 0.5
    var b = 0.35 + 0.65 * sh[i] * sh[i]
    var tw = 0.65 + 0.35 * sin(t * (1.3 + 3 * sh[i]) + i * 2.4)
    tintColor(b * tw, sh[i])
    splat(x, y, pw * (1.2 + 1.3 * b), 2)
  }
  meteor()
}

function meteor() {
  if (mLife <= 0) return
  var k = min(1, mLife * 3)
  if (mode == 2) {                              // the plate keeps it: one thin streak
    hsv(0.13, 0.1, 0.35 * k)
    drawLine(mx, my, mx - mvx * dt, my - mvy * dt, pw * 0.7, 2)
    return
  }
  hsv(0.13, 0.15, 0.25 * k)
  drawLine(mx, my, mx - mvx * 0.22, my - mvy * 0.22, pw * 1.2, 1)
  hsv(0.13, 0.05, 0.9 * k)
  drawLine(mx, my, mx - mvx * 0.06, my - mvy * 0.06, pw * 0.9, 2)
}

function treeLine() {
  rgb(0, 0, 0)
  var cols = 1 / pw + 1, st = ceil(cols / 48)
  for (var c = 0; c < cols; c += st) {
    var x = c * pw, h = 0.08 + 0.035 * sin(x * 5 + 2) + 0.05 * hash(floor(c / st) + 3) * hash(floor(c / st) * 7 + 1)
    fillRect(x - pw * 0.45, 1 - h, x + pw * (st - 0.55), 1)
  }
}

function trails() {
  var sp = warpPct / 100, T = 50 - 25 * sp      // seconds per exposure
  if (lastMode != 2) { clear(); lastMode = 2 }
  if (expo > T) {                               // the plate fades, a new one starts
    fade(pow(0.9, dt * 20))
    if (expo > T + 3) { expo = 0; poleX = 0.25 + random(0.5); poleY = 0.15 + random(0.25) }
  }
  if (expo < 0.3 || expo > T) {                 // town glow behind the trees; the plate keeps it
    rgb(0.07, 0.09, 0.16)
    splat(0.5, 1.2, 0.75, 2)
  }
  for (var i = 0; i < n; i++) {
    var r = 0.04 + 0.95 * sqrt(sx[i] * 0.5 + 0.5), a = ang + sy[i] * PI
    var x = poleX + r * cos(a), y = poleY + r * sin(a)
    var b = 0.3 + 0.7 * sh[i] * sh[i]
    starColor(i, b)
    if (px[i] > -1 && expo > 0.05) drawLine(px[i], py[i], x, y, pw * 0.8, 2)
    px[i] = x; py[i] = y
  }
  meteor()
  treeLine()
}

export function renderBulk() {
  if (mode != lastMode && mode != 2) { clear(); lastMode = mode }
  if (mode == 0) warp()
  else if (mode == 1) drift()
  else trails()
}
