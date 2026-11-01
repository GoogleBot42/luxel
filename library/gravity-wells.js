// name: Gravity Wells
// Original work from the prose idea spec docs/pattern-specs/patternflow/mag-vortex.md
// (Patternflow preset, CC BY-SA 4.0 — prose-described idea only, no code); mode 0 is
// the classic multi-well particle toy. Mode 0: invisible wells wander and a stream of
// particles falls in under softened gravity, winds into accretion discs and is flung
// out when two wells pass close; colour = speed, a flare marks each capture. Mode 1:
// one vortex, Keplerian spin, signed inflow (in, rings, out). One additive drawLine
// per particle on the persistent frame, fade() trails: no buffers, any panel size.

var MAXP = 300
var qx = array(MAXP)   // mode 0: position (world units, 100 = panel width)  · mode 1: radius
var qy = array(MAXP)   //                                                      · angle
var vx = array(MAXP)   //         velocity                                     · spin factor
var vy = array(MAXP)   //                                                      · brightness
var age = array(MAXP)
var wx = array(3)
var wy = array(3)
var dt = 0
var t = 0
var cx = 50
var cy = 50
var lineW = 0.02
var G = 4000           // well strength, world units^3 / s^2
var SOFT = 6           // softening, world units^2: keeps 1/r^2 inside 16.16
var lastMode = -1

// 0 gravity wells, 1 vortex
var mode = 0
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1) }

var wells = 3
//# min=1 max=3 step=1 default=3
export function sliderWells(v) { wells = clamp(floor(v), 1, 3) }

var count = 150
//# min=30 max=300 step=10 default=150
export function sliderParticles(v) { count = clamp(floor(v), 1, MAXP) }

// trail half-life, seconds
var trail = 0.7
//# min=0.1 max=4 step=0.1 default=0.7
export function sliderTrail(v) { trail = clamp(v, 0.05, 6) }

// vortex radial flow, percent: + spirals in, 0 holds rings, - blooms outward
var inflow = 40
//# min=-100 max=100 step=5 default=40
export function sliderInflowPercent(v) { inflow = clamp(v, -100, 100) }

function spawnFall(i) {
  age[i] = 0
  if (random(1) < 0.65) {
    // straight into a near-circular orbit round a random well (same sense for all)
    var k = floor(random(wells))
    var r = 5 + random(20)
    var a = random(PI2)
    var c = cos(a)
    var s = sin(a)
    var v = sqrt(G / r) * (0.75 + random(0.35))
    qx[i] = wx[k] + r * c
    qy[i] = wy[k] + r * s
    vx[i] = -s * v
    vy[i] = c * v
    return
  }
  // or enter from a random edge, drifting inward with a shared swirl
  var e = floor(random(4))
  var p = random(100)
  qx[i] = e == 0 ? -2 : e == 1 ? 102 : p
  qy[i] = e == 2 ? -2 : e == 3 ? 102 : p
  var dx = 50 - qx[i]
  var dy = 50 - qy[i]
  var d = hypot(dx, dy) + 1
  var sw = 4 + random(6)
  vx[i] = (dx * 4 - dy * sw) / d
  vy[i] = (dy * 4 + dx * sw) / d
}

function spawnOrbit(i, r) {
  qx[i] = r
  qy[i] = random(PI2)
  vx[i] = 0.6 + random(0.8)
  vy[i] = 0.45 + random(0.55)
  age[i] = 0
}

function vortexSpawn(i) {
  if (inflow > 4) spawnOrbit(i, 40 + random(40))
  else if (inflow < -4) spawnOrbit(i, 1 + random(4))
  else spawnOrbit(i, 4 + random(60))
}

function reset() {
  lastMode = mode
  for (var i = 0; i < MAXP; i++) {
    if (mode == 0) spawnFall(i)
    else spawnOrbit(i, 2 + random(70))
    age[i] = random(2)
  }
}

export function beforeRender(delta) {
  dt = min(delta, 40) * 0.001
  t += dt
  if (t > 3600) t -= 3600
  var gw = gridWidth()
  lineW = gw > 1 ? 1.2 / (gw - 1) : 0.02
  // wells wander on incommensurate Lissajous paths, so pairs meet now and then
  for (var k = 0; k < 3; k++) {
    wx[k] = 50 + 30 * sin(t * (0.11 + k * 0.037) + k * 2.1)
    wy[k] = 50 + 28 * sin(t * (0.083 + k * 0.029) + k * 4.2)
  }
  cx = 50 + 8 * sin(t * 0.07)
  cy = 50 + 8 * sin(t * 0.053 + 1)
  if (mode != lastMode) reset()
}

// red when slow, through amber and white to blue when fast
function speedColour(s, b) {
  if (s < 0.55) {
    var u = s * 1.818
    oklch((0.62 + 0.26 * u) * b, 0.2 - 0.15 * u, 0.06 + 0.14 * u)
  } else {
    var w = (s - 0.55) * 2.22
    oklch((0.88 - 0.08 * w) * b, 0.05 + 0.09 * w, 0.68)
  }
}

function fall() {
  var h = dt
  var drag = 1 - 0.06 * h
  for (var i = 0; i < count; i++) {
    var x0 = qx[i]
    var y0 = qy[i]
    var ax = 0
    var ay = 0
    var hit = -1
    for (var k = 0; k < wells; k++) {
      var dx = wx[k] - x0
      var dy = wy[k] - y0
      var r2 = dx * dx + dy * dy + SOFT
      if (r2 < SOFT + 3) hit = k              // swallowed by well k
      var f = G / r2 / sqrt(r2)
      ax += dx * f
      ay += dy * f
    }
    if (hit >= 0) {
      oklch(0.55, 0.08, 0.12)                 // a small flare marks the meal
      splat(wx[hit] * 0.01, wy[hit] * 0.01, 2.5 * lineW, 1)
      spawnFall(i)
      continue
    }
    var ux = (vx[i] + ax * h) * drag
    var uy = (vy[i] + ay * h) * drag
    var sp = hypot(ux, uy)
    if (sp > 80) { ux = ux * 80 / sp; uy = uy * 80 / sp; sp = 80 }
    vx[i] = ux
    vy[i] = uy
    var x = x0 + ux * h
    var y = y0 + uy * h
    qx[i] = x
    qy[i] = y
    if (x < -25 || x > 125 || y < -25 || y > 125) { spawnFall(i); continue }
    var ag = age[i] + dt
    age[i] = ag
    var s = saturate((sp - 4) * 0.03)
    speedColour(s, min(1, ag * 2) * (1 - 0.25 * s))
    drawLine(x0 * 0.01, y0 * 0.01, x * 0.01, y * 0.01, lineW, 2)
  }
}

function vortex() {
  var flow = inflow * 0.14 * (0.75 + 0.25 * sin(t * 0.4))   // world units / s, breathing
  for (var i = 0; i < count; i++) {
    var r = qx[i]
    var a = qy[i]
    var x0 = cx + r * cos(a)
    var y0 = cy + r * sin(a)
    var q = 1 + r * 0.08
    a += 5 * vx[i] * dt / (q * sqrt(q))   // Keplerian-ish: the core spins fastest
    if (a > PI2) a -= PI2
    r -= flow * (0.4 + r * 0.012) * dt
    age[i] += dt
    if (r < 1.2 || r > 90 || (abs(inflow) <= 4 && age[i] > 6 + vy[i] * 6)) { vortexSpawn(i); continue }
    qx[i] = r
    qy[i] = a
    var u = saturate(r * 0.017)
    var b = min(1, age[i] * 1.5) * vy[i]
    oklch((0.95 - 0.4 * u) * b, 0.06 + 0.16 * min(1, u * 3), 0.2 - 0.48 * u)
    drawLine(x0 * 0.01, y0 * 0.01, (cx + r * cos(a)) * 0.01, (cy + r * sin(a)) * 0.01, lineW, 1)
  }
}

export function renderBulk() {
  fade(pow(0.5, dt / trail))
  if (mode == 0) fall()
  else vortex()
}
