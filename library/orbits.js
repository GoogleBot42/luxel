// name: Orbits
// Clean-room reimplementation from the prose spec docs/pattern-specs/neutronorbit.md
// (the strip atom) — reimagined, not a replica; the panel modes are a curated original.
// Star system: planets on real (leapfrog, softened gravity) elliptical orbits,
// fast at perihelion, a moon, and a comet that slingshots round the sun with
// its tail pointing away from it. Atom: warm electrons on tilted orbits round
// a throbbing nucleus. Three-body: the figure-eight choreography. Trails are
// the persistent frame fading; a strip shows the same system edge-on.

var mode = 0
//# min=0 max=2 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 2) }

// Seconds for one lap of the reference orbit (the middle planet, an electron,
// or the figure-eight).
var orbitS = 10
//# min=3 max=30 step=0.5 default=10
export function sliderOrbitSeconds(v) { orbitS = clamp(v, 3, 30) }

// Trail half-life in seconds.
var trailS = 0.8
//# min=0.05 max=3 step=0.05 default=0.8
export function sliderTrailSeconds(v) { trailS = clamp(v, 0.05, 3) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

// world units: the panel's short side spans -50..50
var GM = 10659                     // a 30-unit orbit takes 10 sim seconds
var G3 = 21957                     // figure-eight masses (38x scale, 10 s lap)
var NB = 5
var bx = array(NB), by = array(NB), vx = array(NB), vy = array(NB), ax = array(NB), ay = array(NB)
var PH = [0.07, 0.55, 0.13, 0.8, 0.5]
var PS = [2.4, 3.4, 4.2, 3.2, 2]
var WARM = [0.06, 0.92, 0.01]
var built = -1, simT = 0, cometWait = 4, cometOn = 0, t = 0, keep = 0.9

function planet(i, a, e, ang) {
  var r = a * (1 + e), v = sqrt(GM * (1 - e) / r / a)
  bx[i] = cos(ang) * r
  by[i] = sin(ang) * r
  vx[i] = -sin(ang) * v
  vy[i] = cos(ang) * v
}

function build() {
  built = mode
  simT = 0
  cometOn = 0
  cometWait = 3
  if (mode == 0) {
    planet(0, 13, 0.18, 0.4)
    planet(1, 21, 0.38, 2.6)
    planet(2, 30, 0.12, 4.4)
    planet(3, 38, 0.28, 1.2)
  } else if (mode == 2) {
    // Chenciner-Montgomery figure-eight, positions x38, time x1.581
    bx[0] = 36.86; by[0] = -9.237; bx[1] = -36.86; by[1] = 9.237; bx[2] = 0; by[2] = 0
    vx[2] = -22.41; vy[2] = -20.79; vx[0] = 11.21; vy[0] = 10.39; vx[1] = 11.21; vy[1] = 10.39
  }
  accel()
}

function accel() {
  var i
  if (mode == 0) {
    for (i = 0; i < (cometOn ? 5 : 4); i++) {
      var r2 = bx[i] * bx[i] + by[i] * by[i] + 4
      var f = GM / r2 / sqrt(r2)
      ax[i] = -bx[i] * f
      ay[i] = -by[i] * f
    }
  } else {
    for (i = 0; i < 3; i++) { ax[i] = 0; ay[i] = 0 }
    for (i = 0; i < 3; i++) {
      for (var j = i + 1; j < 3; j++) {
        var dx = bx[j] - bx[i], dy = by[j] - by[i], d2 = dx * dx + dy * dy + 1
        var g = G3 / d2 / sqrt(d2)
        ax[i] += dx * g
        ay[i] += dy * g
        ax[j] -= dx * g
        ay[j] -= dy * g
      }
    }
  }
}

function spawnComet() {
  var a = random(PI2), b = (random(1) < 0.5 ? -1 : 1) * (5 + random(7)), v = 21 + random(4)
  var cx = cos(a), cy = sin(a)
  bx[4] = cx * 72 - cy * b
  by[4] = cy * 72 + cx * b
  vx[4] = -cx * v
  vy[4] = -cy * v
  cometOn = 1
  accel()
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  t = mod(t + dt, 3600)
  keep = pow(0.5, dt / trailS)
  if (built != mode) build()
  if (mode == 1) return
  var sim = dt * 10 / orbitS
  var ns = clamp(ceil(sim / 0.008), 1, 16), h = sim / ns
  var nb = mode == 0 ? (cometOn ? 5 : 4) : 3
  for (var s = 0; s < ns; s++) {
    for (var i = 0; i < nb; i++) {
      vx[i] += ax[i] * h / 2
      vy[i] += ay[i] * h / 2
      bx[i] += vx[i] * h
      by[i] += vy[i] * h
    }
    accel()
    for (i = 0; i < nb; i++) {
      vx[i] += ax[i] * h / 2
      vy[i] += ay[i] * h / 2
    }
  }
  if (mode == 2) {
    simT += sim
    if (simT >= 10) build()         // one lap: snap back onto the exact orbit
  } else {
    var r = hypot(bx[4], by[4])
    if (cometOn && r > 85 && bx[4] * vx[4] + by[4] * vy[4] > 0) { cometOn = 0; cometWait = 6 + random(8) }
    if (!cometOn) {
      cometWait -= dt
      if (cometWait <= 0) spawnComet()
    }
  }
}

// atom electron k: tilted ellipse, returns depth (-1 back .. 1 front) and sets ex/ey
var ex = 0, ey = 0
function electron(k) {
  var ph = PI2 * (t / orbitS * 1.6 + k / 3), psi = k * PI / 3 + t * 0.05
  var a = 44, b = 15, u = cos(ph) * a, w = sin(ph) * b
  ex = u * cos(psi) - w * sin(psi)
  ey = u * sin(psi) + w * cos(psi)
  return sin(ph)
}

var sx = 0.01, sy = 0.01, px = 0.02

function panel() {
  fade(keep)
  var gw = max(gridWidth(), 1), gh = max(gridHeight(), 1)
  var short = min(gw, gh)
  sx = short / gw / 100
  sy = short / gh / 100
  px = 1 / max(gw, gh)
  var i
  // a dim, faintly twinkling star field behind everything
  for (i = 0; i < 40; i++) {
    hsv(0.6, 0.25, 0.07 + 0.06 * wave(t * (0.2 + hash(i + 7)) + hash(i)))
    setPixel(gridIndex(hash(i * 3 + 1) * gw, hash(i * 3 + 2) * gh))
  }
  if (mode == 1) {
    var pulse = 0.75 + 0.25 * wave(t * 1.7)
    for (i = 0; i < 3; i++) if (electron(i) < 0) dot(i, 0.55)
    hsv(0.07, 0.7, 0.45 * pulse)
    splat(0.5, 0.5, 10 * sx, 0)
    for (i = 0; i < 7; i++) {
      var ang = i * 2.4 + t * 0.7, rr = i == 0 ? 0 : 4.6 + 0.5 * sin(t * 9 + i)
      if (i % 2) hsv(0.0, 0.85, pulse)
      else hsv(0.6, 0.35, 0.85 * pulse)
      splat(0.5 + cos(ang) * rr * sx, 0.5 + sin(ang) * rr * sy, max(4 * sx, 1.5 * px), 2)
    }
    for (i = 0; i < 3; i++) if (electron(i) >= 0) dot(i, 1)
    return
  }
  if (mode == 2) {
    for (i = 0; i < 3; i++) {
      hsv(0.55 + i * 0.33, 0.6, 1)
      splat(0.5 + bx[i] * sx, 0.5 + by[i] * sy, max(4.5 * sx, 1.8 * px), 2)
      hsv(0, 0, 1)
      splat(0.5 + bx[i] * sx, 0.5 + by[i] * sy, max(1.6 * sx, 0.9 * px), 2)
    }
    return
  }
  // the sun
  var fl = 1 + 0.06 * sin(t * 3.1) + 0.04 * sin(t * 7.3)
  // replace first so the add-mode core never accumulates across frames
  hsv(0.04, 0.95, 0.8)
  splat(0.5, 0.5, 12 * fl * sx, 0)
  hsv(0.09, 0.7, 1)
  splat(0.5, 0.5, 8 * fl * sx, 1)
  hsv(0.13, 0.2, 0.8)
  splat(0.5, 0.5, 4 * sx, 1)
  // planets, and a moon round the third
  for (i = 0; i < 4; i++) {
    hsv(PH[i], 0.7, 1)
    splat(0.5 + bx[i] * sx, 0.5 + by[i] * sy, max(PS[i] * sx, 1.2 * px), 2)
  }
  var ma = t * 4.2
  hsv(0.6, 0.1, 0.8)
  splat(0.5 + (bx[2] + cos(ma) * 5.5) * sx, 0.5 + (by[2] + sin(ma) * 5.5) * sy, max(1 * sx, 0.8 * px), 2)
  // the comet: tail streams straight away from the sun, longer when close
  if (cometOn) {
    var r = max(hypot(bx[4], by[4]), 1), ux = bx[4] / r, uy = by[4] / r
    var len = clamp(260 / r, 3, 28), cx = 0.5 + bx[4] * sx, cy = 0.5 + by[4] * sy
    hsv(0.55, 0.5, 0.45)
    drawLine(cx, cy, cx + ux * len * sx, cy + uy * len * sy, max(2.2 * sx, 1.1 * px), 1)
    hsv(0.5, 0.25, 0.8)
    drawLine(cx, cy, cx + ux * len * 0.6 * sx, cy + uy * len * 0.6 * sy, max(1 * sx, 0.7 * px), 2)
    hsv(0, 0, 1)
    splat(cx, cy, max(1.6 * sx, 1 * px), 2)
  }
}

function dot(k, v) {
  var x = 0.5 + ex * sx, y = 0.5 + ey * sy
  hsv(WARM[k], 0.8, v)
  splat(x, y, max(3.4 * sx, 1.5 * px), 2)
  hsv(WARM[k] + 0.05, 0.3, v)
  splat(x, y, max(1.4 * sx, 0.8 * px), 2)
}

// strip: the same system seen edge-on; depth decides who hides behind the sun
function seg(x, w) {
  var n = pixelCount, c = (0.5 + x / 112) * n
  fillRange(floor(c - w / 2), floor(c - w / 2) + max(round(w), 1))
}

function strip() {
  fade(keep)
  var n = pixelCount, core = max(n / 60, 2), i, d
  var pulse = 0.8 + 0.2 * wave(t * 1.7)
  if (mode == 1) {
    for (i = 0; i < 3; i++) {
      d = electron(i)
      if (d < 0 && abs(ex) < 6) continue
      hsv(WARM[i], 0.85, 0.55 + 0.45 * d)
      seg(ex, max(n / 50, 2))
    }
    hsv(0.1, 0.25, pulse)
    seg(sin(t * 2) * 0.8, core * (1 + 0.15 * sin(t * 12)))
    return
  }
  var nb = mode == 2 ? 3 : 4
  if (mode == 0) {
    hsv(0.08, 0.7, pulse)
    seg(0, core * 1.6)
  }
  for (i = 0; i < nb; i++) {
    if (mode == 0 && by[i] > 0 && abs(bx[i]) < 7) continue
    if (mode == 0) hsv(PH[i], 0.7, 1)
    else hsv(0.55 + i * 0.33, 0.6, 1)
    seg(bx[i], max(n / 70, 2))
  }
  if (mode == 0 && cometOn) {
    hsv(0.5, 0.3, 1)
    seg(bx[4], max(n / 90, 1))
  }
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
