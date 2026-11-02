// name: Rocket Launch
// tags: both, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/rocket-by-tony-hampton.md, rainbow-rocket-sparks.md — reimagined, not a replica.
// A launch, told along a strip (index 0 is the pad) or up a panel: a beacon
// blinks through the countdown, the engine lights and sprays the pad, the rocket
// climbs, stages with a flash and boosts, then bursts at apogee or streaks off
// the top, leaving smoke puffs to drift and fade. Exhaust sparks cool from
// white-hot through orange to red, or take a rainbow fixed to the strip.
// Persistent frame: fade() + splatIndex/fillRangeAA (strip) or splat/drawLine (panel).

var flightS = 4
//# min=1 max=10 step=0.5 default=4
export function sliderFlight(v) { flightS = clamp(v, 1, 10) }

var sizePx = 3
//# min=1 max=10 step=1 default=3
export function sliderSize(v) { sizePx = clamp(floor(v), 1, 10) }

// seconds on the pad between launches
var delayS = 3
//# min=0 max=20 step=0.5 default=3
export function sliderDelay(v) { delayS = clamp(v, 0, 20) }

// second-stage thrust, times the first stage
var boost = 2
//# min=1 max=5 step=0.1 default=2
export function sliderBoost(v) { boost = clamp(v, 1, 5) }

// 0 physical (white-hot cooling to red), 1 rainbow fixed to the strip
var exhaust = 0
//# min=0 max=1 step=1 default=0
export function sliderExhaust(v) { exhaust = clamp(floor(v), 0, 1) }

var apexBurst = 1
//# min=0 max=1 step=1 default=1
export function toggleApexBurst(v) { apexBurst = v > 0.5 }

var NP = 64                  // exhaust + burst sparks, a ring buffer
var px = array(NP)
var py = array(NP)
var pvx = array(NP)
var pvy = array(NP)
var life = array(NP)
var rate = array(NP)         // life spent per second
var hue = array(NP)          // < 0: exhaust; else a burst star's hue
var next = 0

var NS = 18                  // smoke puffs
var smX = array(NS)
var smY = array(NS)
var smL = array(NS)
var smNext = 0

var phase = 0                // 0 pad, 1 flight, 2 burst/clear
var phaseT = 0
var pos = 0                  // rocket tail height, 0..1
var vel = 0
var staged = 0
var rx = 0.5                 // panel column
var emitAcc = 0
var smokeAcc = 0
var burstHue = 0
var flash = 0
var dt = 0.05

var strip = 0
var N1 = 1
var unit = 0.03              // one cell of a 32-wide panel, in map units

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

export function beforeRender(delta) { dt = min(delta, 80) * 0.001 }

function spawn(x, y, vx, vy, lifeS, h) {
  var i = next
  next = (next + 1) % NP
  px[i] = x; py[i] = y; pvx[i] = vx; pvy[i] = vy
  life[i] = 1; rate[i] = 1 / lifeS; hue[i] = h
}

function puff(x, y) {
  smX[smNext] = x + (random(2) - 1) * unit
  smY[smNext] = y
  smL[smNext] = 1
  smNext = (smNext + 1) % NS
}

// the brush for a spark of life L at height y
function sparkColour(L, y, h) {
  var v = L * (2 - L)
  if (h >= 0) { hsv(h, clamp(1.6 - L, 0, 1), v); return }
  var s = clamp(1.7 - 1.7 * L, 0, 1)
  if (exhaust == 1) hsv(y * 3, s, v)
  else hsv(0.01 + 0.12 * L, s, v)
}

function dot(x, y, r) {
  if (strip) splatIndex(y * N1 + 0.5, r, 2)
  else splat(x, 1 - y, r * unit, 2)
}

function streak(x0, y0, x1, y1, r) {
  if (strip) {
    var a = y0 * N1, b = y1 * N1
    if (abs(b - a) > 1) fillRangeAA(min(a, b) + 0.5, max(a, b) + 0.5, 2)
    splatIndex(b + 0.5, r, 2)
  } else drawLine(x0, 1 - y0, x1, 1 - y1, r * unit, 2)
}

function launchPad() {
  phase = 0; phaseT = 0; pos = 0; vel = 0; staged = 0
  rx = 0.3 + random(0.4)
}

function burst() {
  phase = 2; phaseT = 0; flash = 0.15
  burstHue = random(1)
  var n = strip ? 20 : 28
  for (var k = 0; k < n; k++) {
    var sp = 0.5 + random(0.5)
    var h = burstHue + (k % 2) * 0.08
    if (strip) spawn(rx, pos, 0, sp * (random(2) - 1) * 1.6, 1.4 + random(0.6), h)
    else {
      var a = (k + random(1)) / n * PI2
      spawn(rx, pos, cos(a) * sp, sin(a) * sp, 1.4 + random(0.6), h)
    }
  }
}

export function renderBulk() {
  strip = !has2DMap()
  N1 = pixelCount - 1
  if (!strip) {
    var gw = gridWidth()
    if (gw < 1) gw = 32
    unit = clamp(gw / 32, 1, 3) / gw
  }
  var lenU = strip ? sizePx / max(1, N1) : sizePx * unit
  fade(pow(0.5, dt / 0.09))
  phaseT += dt

  // smoke: slow, widening, cooling puffs that outlive the spark trails
  for (var k = 0; k < NS; k++) {
    var L = smL[k]
    if (L <= 0) continue
    L -= dt / 5
    smL[k] = L
    smY[k] += dt * 0.015
    var gr = 0.2 * L
    rgb(gr, gr * 0.95, gr * 1.1)
    dot(smX[k], smY[k], 2 + (1 - L) * 4)
  }

  // a few faint twinkling stars in the sky above the pad
  var tw = time(0.06)
  for (var k = 0; k < 14; k++) {
    var sv = 0.05 + 0.09 * wave(tw + hash(k + 0.5))
    rgb(sv, sv, sv * 1.2)
    dot(hash(k + 20.5), 0.4 + 0.58 * hash(k + 40.5), 0.7)
  }

  // the pad: floodlit ground (panel) or a warm glow at the base (strip)
  if (strip) {
    hsv(0.07, 0.7, 0.14)
    fillRangeAA(0, max(2, pixelCount * 0.03), 2)
  } else {
    rgb(0.04, 0.035, 0.05)
    fillRect(0, 1 - 1.5 * unit, 1, 1)
  }

  var thrust = 0
  if (phase == 0) {
    // countdown: a red beacon blinks; the engine lights for the last second
    var b = frac(phaseT) < 0.15 ? 0.7 : 0.08
    rgb(b, 0, 0)
    if (strip) splatIndex(0.5, 1, 2)
    else {
      rgb(0.06, 0.06, 0.07)
      drawLine(rx + 4 * unit, 1, rx + 4 * unit, 1 - lenU - 6 * unit, 0.6 * unit, 2)
      rgb(b, 0, 0)
      splat(rx + 4 * unit, 1 - lenU - 6 * unit, 1.2 * unit, 2)
    }
    var ign = clamp(1 - (delayS - phaseT), 0, 1)
    thrust = ign * ign * 0.7
    if (phaseT >= delayS) { phase = 1; phaseT = 0 }
  } else if (phase == 1) {
    var a = 2 / (flightS * flightS)
    if (staged == 0 && pos > 0.35) { staged = 1; flash = 0.08; phaseT = 0 }
    // a short coast between stages, then the boost
    var coast = staged && phaseT < 0.18
    if (staged && !coast) a *= boost
    if (coast) a = -0.1
    var p0 = pos
    vel += a * dt
    pos += vel * dt
    thrust = coast ? 0 : staged ? 1.4 : 1
    smokeAcc += dt * (coast ? 0 : 7)
    if (smokeAcc >= 1) { smokeAcc -= 1; puff(rx, p0) }
    if (apexBurst && pos + lenU > (strip ? 0.9 : 0.82)) burst()
    else if (pos > 1.05) { phase = 2; phaseT = 0 }
  } else if (phaseT > (apexBurst ? 1.2 : 0.6)) launchPad()

  // exhaust: emitted at the nozzle, rate following thrust and rocket size
  emitAcc += dt * thrust * (30 + sizePx * 8)
  while (emitAcc >= 1) {
    emitAcc -= 1
    var ve = 0.35 + random(0.45)
    spawn(rx + (random(2) - 1) * unit * 0.5, pos, (random(2) - 1) * 0.06,
      vel - ve, 0.35 + random(0.5), -1)
  }

  if (flash > 0) {
    flash -= dt
    rgb(1, 1, 0.9)
    dot(rx, pos + (phase == 2 ? 0 : lenU * 0.5), phase == 2 ? 4 : 2.5)
  }

  var dk = min(3 * dt, 0.9)
  for (var i = 0; i < NP; i++) {
    var L = life[i]
    if (L <= 0) continue
    L -= rate[i] * dt
    life[i] = L
    if (L <= 0) continue
    var x0 = px[i], y0 = py[i]
    var vx = pvx[i] * (1 - dk)
    var vy = pvy[i] * (1 - dk) - 0.25 * dt
    var x1 = x0 + vx * dt, y1 = y0 + vy * dt
    if (y1 < 0) {
      // the pad sprays exhaust sideways (panel) or back up a little (strip)
      y1 = 0; vy = -vy * 0.25
      if (!strip) vx = (vx < 0 ? -1 : 1) * (0.15 + random(0.3))
    }
    pvx[i] = vx; pvy[i] = vy; px[i] = x1; py[i] = y1
    var flick = hue[i] < 0 && L < 0.4 && random(1) < 0.3 ? 0.4 : 1
    sparkColour(L * flick, y1, hue[i])
    streak(x0, y0, x1, y1, hue[i] < 0 ? 1 : 1.1)
  }

  if (phase == 1) {
    // the body: engine glow at the tail, white hull, red nose
    rgb(1, 0.85, 0.5)
    dot(rx, pos, 1.6)
    rgb(0.85, 0.85, 0.9)
    if (strip) fillRangeAA(pos * N1 + 0.5, (pos + lenU) * N1 + 0.5, 2)
    else drawLine(rx, 1 - pos, rx, 1 - pos - lenU, 0.9 * unit, 2)
    rgb(1, 0.08, 0.05)
    dot(rx, pos + lenU, 0.9)
  }
}
