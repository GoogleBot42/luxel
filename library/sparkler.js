// name: Sparkler
// Curated original (hand-written showcase of the Luxel language/builtins).
// A hand sparkler burning along the strip: the white-hot point creeps forward,
// throwing sparks both ways that fork once into smaller stars and wink out;
// behind it the burnt wire glows and cools, ahead lies the grey unlit wire. At
// the end it pops in a flash, cools, and the fuse rewinds for the next light.
// Persistent frame: fade() for spark trails, gradientRange/fillRangeAA for the
// wire, splatIndex per spark; a 40-spark ring buffer, no pixel arrays.

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

var NP = 40
var sp = array(NP)
var sv = array(NP)
var sl = array(NP)          // life 1 → 0
var sr = array(NP)          // life spent per second
var sg = array(NP)          // 0 primary, 1 forked
var sh = array(NP)          // hue for the rainbow colour
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

export function renderBulk() {
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
