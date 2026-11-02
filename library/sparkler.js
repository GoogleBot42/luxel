// name: Sparkler
// tags: both, holiday
// Curated original (hand-written showcase of the Luxel language/builtins).
// A hand sparkler burning along the strip: the white-hot point creeps forward,
// throwing sparks both ways that fork once into smaller stars and wink out;
// behind it the burnt wire glows and cools, ahead lies the grey unlit wire. At
// the end it pops in a flash, cools, and the fuse rewinds for the next light.
// On a panel the sparkler is a slanted wire burning down from its tip toward
// the handle, the hot point spraying stars every way; they drag, droop and fork.
// Persistent frame: fade() for spark trails, gradientRange/fillRangeAA for the
// wire (drawLine on a panel), splatIndex/drawLine per spark; a spark ring
// buffer, no pixel arrays.

var burnS = 20
//# min=5 max=120 step=1 default=20
export function sliderBurn(v) { burnS = clamp(v, 5, 120) }

var rateHz = 60
//# min=10 max=200 step=5 default=60
export function sliderSparksPerSecond(v) { rateHz = clamp(v, 10, 200) }

// how far the biggest sparks fly, in pixels
var reachPx = 12
//# min=2 max=40 step=1 default=12
export function sliderReach(v) { reachPx = clamp(v, 2, 40) }

// 0 gold, 1 white, 2 rainbow
var colour = 0
//# min=0 max=2 step=1 default=0
export function sliderColour(v) { colour = clamp(floor(v), 0, 2) }

var NP = 40                 // the strip's ring; a panel uses all PNP slots
var PNP = 72
var sp = array(PNP)         // position (x on a panel)
var sv = array(PNP)
var sl = array(PNP)         // life 1 → 0
var sr = array(PNP)         // life spent per second
var sg = array(PNP)         // 0 primary, 1 forked
var sh = array(PNP)         // hue for the rainbow colour
var sy = array(PNP)         // panel only: y and its velocity
var svy = array(PNP)
var next = 0

var DRAG = 5                // per second
var phase = 0               // 0 burning, 1 cooling, 2 rewinding
var phaseT = 0
var head = 0
var emit = 0
var flash = 0.2
var dt = 0.05

function spark(p, v, lifeS, gen, h) {
  var i = next
  next = (next + 1) % NP
  sp[i] = p; sv[i] = v; sl[i] = 1; sr[i] = 1 / lifeS; sg[i] = gen; sh[i] = h
}

function throwSpark(p, power) {
  var v = (random(1) < 0.5 ? -1 : 1) * reachPx * DRAG * (0.35 + random(0.65)) * power
  spark(p, v, 0.25 + random(0.35), 0, random(1))
}

export function beforeRender(delta) { dt = min(delta, 80) * 0.001 }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) {}

// ---- panel: a slanted sparkler burning from its tip (TX, TY) down to the
// handle (BX, BY); `pu` is how far the flame has got, 0..1
var TX = 0.64, TY = 0.1, BX = 0.4, BY = 0.8
var pu = 0, unit = 0.03

function pspark(x, y, vx, vy, lifeS, gen, h) {
  var i = next
  next = (next + 1) % PNP
  sp[i] = x; sy[i] = y; sv[i] = vx; svy[i] = vy
  sl[i] = 1; sr[i] = 1 / lifeS; sg[i] = gen; sh[i] = h
}

function pthrow(x, y, power) {
  // reach counts pixels of a 32-wide panel, so the spray keeps its size
  var a = random(PI2), v = reachPx / 32 * DRAG * (0.35 + random(0.65)) * power
  pspark(x, y, cos(a) * v, sin(a) * v, 0.25 + random(0.35), 0, random(1))
}

function renderPanel() {
  var gw = gridWidth()
  if (gw < 1) gw = 32
  unit = 1 / gw
  fade(pow(0.5, dt / 0.05))
  phaseT += dt
  var hx = mix(TX, BX, pu), hy = mix(TY, BY, pu)

  // the handle, the unlit coating ahead of the flame, the burnt wire behind
  var glow = 1
  if (phase == 1) glow = max(0, 1 - phaseT / 1.6)
  if (phase == 2) glow = 0
  rgb(0.05, 0.035, 0.025)
  drawLine(BX, BY, BX - (TX - BX) * 0.25, BY + (BY - TY) * 0.25, unit * 0.9, 2)
  rgb(0.08, 0.075, 0.07)
  drawLine(hx, hy, BX, BY, unit * 1.1, 2)
  rgb(0.03 * glow + 0.012, 0.006 * glow + 0.005, 0.004)
  drawLine(TX, TY, hx, hy, unit * 0.8, 2)
  rgb(0.32 * glow + 0.01, 0.07 * glow + 0.005, 0.01)
  drawLine(mix(hx, TX, 0.14), mix(hy, TY, 0.14), hx, hy, unit * 1.1, 2)

  if (phase == 0) {
    pu += dt / burnS * (0.6 + random(0.8))
    emit += dt * rateHz * 1.4
    while (emit >= 1) { emit -= 1; pthrow(hx, hy, 1) }
    hsv(0.1, 0.6, 0.5 + random(0.3))
    splat(hx, hy, unit * 3.2, 2)
    rgb(1, 0.95, 0.85)
    splat(hx, hy, unit * 1.4, 2)
    if (pu >= 1) {
      phase = 1; phaseT = 0; flash = 0.18; pu = 1
      for (var k = 0; k < 30; k++) pthrow(BX, BY, 1.5)
    }
  } else if (phase == 1) {
    if (phaseT > 1.8) { phase = 2; phaseT = 0 }
  } else {
    // relight: a cool glint runs back up to the tip
    pu = max(0, 1 - phaseT / 1.2)
    rgb(0.25, 0.35, 0.6)
    splat(hx, hy, unit * 2, 2)
    if (phaseT >= 1.2) { phase = 0; phaseT = 0; pu = 0; flash = 0.12 }
  }

  if (flash > 0) {
    flash -= dt
    rgb(1, 1, 0.9)
    splat(hx, hy, unit * (phase == 1 ? 7 : 3.5), 2)
  }

  var dk = min(DRAG * dt, 0.9), drop = 0.35 * dt
  for (var i = 0; i < PNP; i++) {
    var L = sl[i]
    if (L <= 0) continue
    L -= sr[i] * dt
    var x0 = sp[i], y0 = sy[i]
    var vx = sv[i] * (1 - dk), vy = svy[i] * (1 - dk) + drop
    var x = x0 + vx * dt, y = y0 + vy * dt
    sv[i] = vx; svy[i] = vy; sp[i] = x; sy[i] = y
    if (L <= 0) {
      sl[i] = 0
      // a primary star forks into two small ones as it burns out
      if (sg[i] == 0 && random(1) < 0.6) {
        var fv = reachPx / 32 * DRAG * 0.3, fa = random(PI2)
        pspark(x, y, cos(fa) * fv, sin(fa) * fv, 0.15 + random(0.15), 1, sh[i])
        pspark(x, y, -cos(fa) * fv, -sin(fa) * fv, 0.15 + random(0.15), 1, sh[i])
      }
      continue
    }
    sl[i] = L
    var b = sg[i] ? 0.75 : 1
    if (colour == 0) hsv(0.05 + 0.08 * L, clamp(1.4 - L, 0.15, 1), b * (0.35 + 0.65 * L))
    else if (colour == 1) hsv(0.6, 0.35 * (1 - L), b * (0.4 + 0.6 * L))
    else hsv(sh[i], clamp(1.5 - L, 0.2, 1), b * (0.4 + 0.6 * L))
    drawLine(x0, y0, x, y, unit * 0.8, 2)
  }
}

export function renderBulk() {
  if (has2DMap()) { renderPanel(); return }
  var n = pixelCount
  fade(pow(0.5, dt / 0.05))
  phaseT += dt

  // the wire: grey ahead of the flame, a cooling glow behind it
  var glow = 1
  if (phase == 1) glow = max(0, 1 - phaseT / 1.6)
  if (phase == 2) glow = 0
  rgb(0.05, 0.045, 0.04)
  fillRangeAA(head + 1, n, 2)
  var g = max(4, n * 0.14)
  rgb(0.025 * glow + 0.01, 0.004 * glow + 0.004, 0.003)
  fillRangeAA(0, max(0, head - g), 2)
  gradientRange(head - g, head + 1, 0.01, 1, 0.03 * glow + 0.012, 0.05, 1, 0.32 * glow + 0.012, 2)

  if (phase == 0) {
    head += dt * n / burnS * (0.6 + random(0.8))
    emit += dt * rateHz
    while (emit >= 1) { emit -= 1; throwSpark(head, 1) }
    // the burning point: a flickering white core in a gold halo
    hsv(0.1, 0.6, 0.5 + random(0.3))
    splatIndex(head + 0.5, 3, 2)
    rgb(1, 0.95, 0.85)
    splatIndex(head + 0.5, 1.2, 2)
    if (head >= n - 1) {
      phase = 1; phaseT = 0; flash = 0.18; head = n - 1
      for (var k = 0; k < 24; k++) throwSpark(head, 1.6)
    }
  } else if (phase == 1) {
    if (phaseT > 1.8) { phase = 2; phaseT = 0 }
  } else {
    // rewind: a cool glint runs the fuse back to the start
    head = (n - 1) * max(0, 1 - phaseT / 1.2)
    rgb(0.25, 0.35, 0.6)
    splatIndex(head + 0.5, 2, 2)
    if (phaseT >= 1.2) { phase = 0; phaseT = 0; head = 0; flash = 0.12 }
  }

  if (flash > 0) {
    flash -= dt
    rgb(1, 1, 0.9)
    splatIndex(head + 0.5, phase == 1 ? 7 : 3, 2)
  }

  var dk = min(DRAG * dt, 0.9)
  for (var i = 0; i < NP; i++) {
    var L = sl[i]
    if (L <= 0) continue
    L -= sr[i] * dt
    var p0 = sp[i]
    var v = sv[i] * (1 - dk)
    var p = p0 + v * dt
    sv[i] = v; sp[i] = p
    if (L <= 0) {
      sl[i] = 0
      // a primary star forks into two small ones as it burns out
      if (sg[i] == 0 && random(1) < 0.6) {
        var fv = reachPx * DRAG * 0.3
        spark(p, fv * (0.4 + random(0.6)), 0.15 + random(0.15), 1, sh[i])
        spark(p, -fv * (0.4 + random(0.6)), 0.15 + random(0.15), 1, sh[i])
      }
      continue
    }
    sl[i] = L
    var b = sg[i] ? 0.75 : 1
    if (colour == 0) hsv(0.05 + 0.08 * L, clamp(1.4 - L, 0.15, 1), b * (0.35 + 0.65 * L))
    else if (colour == 1) hsv(0.6, 0.35 * (1 - L), b * (0.4 + 0.6 * L))
    else hsv(sh[i], clamp(1.5 - L, 0.2, 1), b * (0.4 + 0.6 * L))
    if (abs(p - p0) > 1) fillRangeAA(min(p, p0) + 0.5, max(p, p0) + 0.5, 2)
    splatIndex(p + 0.5, 0.8, 2)
  }
}
