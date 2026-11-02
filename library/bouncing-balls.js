// name: Bouncing Balls
// Clean-room reimplementation from the prose specs docs/pattern-specs/2d-bouncing-additive-primaries.md,
// bouncing-rgb-balls-2d.md, bouncing-balls-rgb.md, bouncing-balls-hsv.md, bouncer3d.md — reimagined, not a replica.
// Glowing balls in three motions: billiard ricochets, gravity bounces where
// each ball keeps a little less of its speed (they fall out of step, and a
// spent ball is kicked back up), or a slow Lissajous drift. Each ball is two
// additive splats plus a drawLine from its last position over fade() trails,
// so the cost is per ball, not per pixel. On a strip the same balls run along
// the length (anti-aliased sub-pixel spans, no buffers).

var MAXB = 24
var bx = array(MAXB), by = array(MAXB), ox = array(MAXB), oy = array(MAXB)
var ux = array(MAXB), uy = array(MAXB), vy = array(MAXB), el = array(MAXB)
for (var k = 0; k < MAXB; k++) {
  bx[k] = 0.12 + 0.76 * hash(k + 0.5)
  by[k] = 0.12 + 0.76 * hash(k + 9.5)
  var a = (k % 4 + 0.15 + 0.7 * hash(k + 21.5)) * PI / 2   // never axis-aligned
  var sp = 0.75 + 0.5 * hash(k + 33.5)
  ux[k] = cos(a) * sp
  uy[k] = sin(a) * sp
  el[k] = 0.92 - k * 0.011         // restitution spread: the gravity desync
  ox[k] = bx[k]
  oy[k] = by[k]
}

var motion = 0
// 0 billiard, 1 gravity, 2 Lissajous drift
//# min=0 max=2 step=1 default=0
export function sliderMotion(v) { motion = clamp(floor(v), 0, 2) }

var palette = 0
// 0 rainbow, 1 additive red/green/blue, 2 the picker's colour
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 2) }

// Picker colour for palette 2 (hue and saturation; its brightness is ignored).
var pickH = 0.58, pickS = 0.8
export function hsvPickerColor(h, s, v) { pickH = h; pickS = s }

var count = 6
//# min=1 max=24 step=1 default=6
export function sliderCount(v) { count = clamp(floor(v), 1, MAXB) }

// Ball radius in percent of the panel (a strip uses a quarter of its length).
var radiusPct = 8
//# min=2 max=30 step=1 default=8
export function sliderRadius(v) { radiusPct = clamp(v, 2, 30) }

// How much each ball's radius swells and shrinks, in percent.
var breathePct = 0
//# min=0 max=50 step=1 default=0
export function sliderBreathing(v) { breathePct = clamp(v, 0, 50) }

// Seconds for a trail to fade out (0 = no trail, just a motion blur).
var trail = 0.3
//# min=0 max=2 step=0.05 default=0.3
export function sliderTrail(v) { trail = clamp(v, 0, 2) }

// Panel widths per second (gravity: the fall scales with it).
var speed = 0.6
//# min=0.1 max=3 step=0.05 default=0.6
export function sliderSpeed(v) { speed = clamp(v, 0.1, 3) }

// Strip only: 0 balls bounce off the start, 1 off the end, 2 off both ends.
var dir = 0
//# min=0 max=2 step=1 default=0
export function sliderDirection(v) { dir = clamp(floor(v), 0, 2) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var tm = 0, lt = 0, fadeK = 0, m = 0.05

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  tm = mod(tm + dt, 3600)
  lt = mod(lt + dt * speed, 3600)
  fadeK = trail < 0.02 ? 0 : pow(0.05, dt / trail)
  m = radiusPct / 100 * 0.6                     // centre-to-wall margin
  var lo = m, hi = 1 - m
  var g = 5.8 * speed * speed                   // ~0.9 s full-height drop at the default
  var v0 = sqrt(2 * g * (hi - lo))
  for (var i = 0; i < count; i++) {
    ox[i] = bx[i]
    oy[i] = by[i]
    if (motion == 2) {
      var f = 0.4 + 0.6 * hash(i + 45.5)
      bx[i] = 0.5 + (0.5 - m) * sin(lt * PI2 * f * 0.4 + i * 2.1)
      by[i] = 0.5 + (0.5 - m) * sin(lt * PI2 * f * 0.31 * (1 + hash(i + 57.5)) + i * 1.3)
      continue
    }
    var sx = motion == 1 ? 0.35 : 1             // gravity drifts sideways slowly
    bx[i] += ux[i] * speed * sx * dt
    if (bx[i] < lo) { bx[i] = 2 * lo - bx[i]; ux[i] = abs(ux[i]) }
    if (bx[i] > hi) { bx[i] = 2 * hi - bx[i]; ux[i] = -abs(ux[i]) }
    if (motion == 0) {
      by[i] += uy[i] * speed * dt
      if (by[i] < lo) { by[i] = 2 * lo - by[i]; uy[i] = abs(uy[i]) }
      if (by[i] > hi) { by[i] = 2 * hi - by[i]; uy[i] = -abs(uy[i]) }
    } else {
      vy[i] = clamp(vy[i] + g * dt, -v0 * 1.5, v0 * 1.5)   // y grows downward
      by[i] += vy[i] * dt
      if (by[i] > hi) {                         // the floor
        by[i] = hi
        vy[i] = -abs(vy[i]) * el[i]
        if (-vy[i] < v0 * 0.45) vy[i] = -v0 * (0.75 + 0.25 * hash2(i, tm))
      }
      if (by[i] < lo) { by[i] = lo; vy[i] = abs(vy[i]) * 0.5 }
    }
    bx[i] = clamp(bx[i], lo, hi)
    by[i] = clamp(by[i], lo, hi)
  }
}

var ch = 0, cs = 0
function colour(i) {
  if (palette == 1) { ch = (i % 3) / 3; cs = 1 }
  else if (palette == 2) { ch = pickH + 0.1 * (i / max(count - 1, 1) - 0.5); cs = pickS }
  else { ch = i / count + tm * 0.02; cs = 0.85 }
}

function rad(i) {
  return radiusPct / 100 * (1 + breathePct / 100 * sin(tm * PI2 / (3 + 4 * hash(i + 69.5)) + i))
}

// strip: one ball as an anti-aliased tent with a flat core, max-blended over the
// trails — two gradientRange ramps (0 -> 1.7, the hsv clamp makes the core) meeting
// at the centre, plus the one pixel they share; no per-pixel loop
function ball1D(c, hw) {
  var cc = c + 0.5, p = floor(cc)
  gradientRange(cc - hw, cc, ch, cs, 0, ch, cs, 1.7, 2)
  gradientRange(cc, cc + hw, ch, cs, 1.7, ch, cs, 0, 2)
  hsv(ch, cs, min(1, 1.7 * (1 - abs(p + 0.5 - cc) / hw)))
  fillRangeAA(p, p + 1, 2)
}

function strip() {
  var n = pixelCount - 1, span = dir == 2 ? n / 2 : n
  for (var i = 0; i < count; i++) {
    colour(i)
    var u = motion == 1 ? (1 - m - by[i]) : (bx[i] - m)
    var u0 = motion == 1 ? (1 - m - oy[i]) : (ox[i] - m)
    u = u / (1 - 2 * m) * span
    u0 = u0 / (1 - 2 * m) * span
    if (dir == 1) { u = n - u; u0 = n - u0 }
    var hw = max(0.7, rad(i) * pixelCount / 4)
    hsv(ch, cs, 0.45)                           // the swept span, sub-pixel ends
    fillRangeAA(min(u, u0) + 0.5, max(u, u0) + 0.5)
    ball1D(u, hw)
    if (dir == 2) {
      hsv(ch, cs, 0.45)
      fillRangeAA(n - max(u, u0) + 0.5, n - min(u, u0) + 0.5)
      ball1D(n - u, hw)
    }
  }
}

export function renderBulk() {
  if (fadeK > 0) fade(fadeK)
  else clear()
  if (!has2DMap()) { strip(); return }
  for (var i = 0; i < count; i++) {
    colour(i)
    var r = rad(i)
    hsv(ch, cs, 0.5)
    drawLine(ox[i], oy[i], bx[i], by[i], r * 0.55, 1)   // motion blur from last frame
    hsv(ch, cs, 1)
    splat(bx[i], by[i], r, 1)                           // soft glow …
    splat(bx[i], by[i], r * 0.55, 1)                    // … over a full core
    if (palette != 1) {                                 // a small specular sheen
      hsv(0, 0, 0.35)
      splat(bx[i] - r * 0.25, by[i] - r * 0.25, r * 0.3, 1)
    }
  }
}
