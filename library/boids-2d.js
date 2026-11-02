// name: Boids
// tags: panel, nature
// Clean-room reimplementation from the prose spec docs/pattern-specs/time-flies-2d.md — reimagined, not a replica.
// A real flock at native resolution: Reynolds' separation, alignment and
// cohesion over a coarse 8x8 neighbour grid (a linked list per cell), so each
// bird looks only at its neighbours. Each bird is a drawLine streak along its
// velocity, coloured by its heading, so a turning flock sweeps the
// spectrum; fade() keeps the trails. Swarm 1 swaps the flock for jittering
// flies that buzz around a drifting centre with flapping wings.

var MAXN = 80, G = 8
var px = array(MAXN), py = array(MAXN), vx = array(MAXN), vy = array(MAXN)
var nxt = array(MAXN), cell = array(G * G)
for (var k = 0; k < MAXN; k++) {
  px[k] = 0.15 + 0.7 * hash(k + 0.5)
  py[k] = 0.15 + 0.7 * hash(k + 9.5)
  var a = hash(k + 17.5) * PI2
  vx[k] = cos(a) * 0.3
  vy[k] = sin(a) * 0.3
}

var count = 32
//# min=5 max=80 step=1 default=32
export function sliderCount(v) { count = clamp(floor(v), 5, MAXN) }

// Panel widths per second.
var speed = 0.3
//# min=0.05 max=1 step=0.05 default=0.3
export function sliderSpeed(v) { speed = clamp(v, 0.05, 1) }

// Seconds for a trail to fade out.
var trail = 0.6
//# min=0 max=3 step=0.05 default=0.6
export function sliderTrail(v) { trail = clamp(v, 0, 3) }

// Personal space, in percent of the neighbour radius.
var sepPct = 50
//# min=0 max=100 step=1 default=50
export function sliderSeparation(v) { sepPct = clamp(v, 0, 100) }

// 0 flock, 1 jittering flies
var swarm = 0
//# min=0 max=1 step=1 default=0
export function sliderSwarm(v) { swarm = clamp(floor(v), 0, 1) }

var R = 0.11, wt = random(100), fadeK = 0, hueOff = 0

export function beforeRender(delta) {
  var dt = min(delta, 50) / 1000
  wt = mod(wt + dt, 1000)
  hueOff = time(0.6)
  fadeK = trail < 0.02 ? 0 : pow(0.05, dt / trail)
  var s = speed, rs = R * sepPct / 100 * 0.7, i, j, c
  for (c = 0; c < G * G; c++) cell[c] = -1
  for (i = 0; i < count; i++) {
    c = clamp(floor(px[i] * G), 0, G - 1) + G * clamp(floor(py[i] * G), 0, G - 1)
    nxt[i] = cell[c]
    cell[c] = i
  }
  var scx = 0.5 + 0.25 * sin(wt * 0.7), scy = 0.5 + 0.2 * sin(wt * 0.53 + 1)
  for (i = 0; i < count; i++) {
    var x = px[i], y = py[i], n = 0, ax = 0, ay = 0
    var mx = 0, my = 0, mvx = 0, mvy = 0
    var gx = floor(x * G), gy = floor(y * G)
    for (var cy = max(gy - 1, 0); cy <= min(gy + 1, G - 1); cy++) {
      for (var cx = max(gx - 1, 0); cx <= min(gx + 1, G - 1); cx++) {
        j = cell[cx + G * cy]
        while (j >= 0) {
          var dx = x - px[j], dy = y - py[j], d2 = dx * dx + dy * dy
          if (j != i && d2 < R * R) {
            n++
            mx += px[j]
            my += py[j]
            mvx += vx[j]
            mvy += vy[j]
            if (d2 < rs * rs) {                 // separation: push off, harder when close
              var d = max(sqrt(d2), 0.003), f = (rs - d) / (rs * d) * 20 * s
              ax += dx * f
              ay += dy * f
            }
          }
          j = nxt[j]
        }
      }
    }
    var lo = 0.6, hi = 1.3
    if (swarm == 0) {
      if (n > 0) {
        ax += (mvx / n - vx[i]) * 2 + (mx / n - x) * 4 * s
        ay += (mvy / n - vy[i]) * 2 + (my / n - y) * 4 * s
      }
      ax += simplex2(i * 3.7, wt * 0.4) * 1.2 * s
      ay += simplex2(i * 3.7 + 40, wt * 0.4) * 1.2 * s
    } else {                                    // flies: twitchy, held near a drifting centre
      ax += simplex2(i * 7.3, wt * 4) * 14 * s + (scx - x) * 6 * s
      ay += simplex2(i * 7.3 + 50, wt * 4) * 14 * s + (scy - y) * 6 * s
      lo = 0.2
      hi = 1.8
    }
    var m = 0.12                                // soft walls
    if (x < m) ax += (m - x) * 60 * s
    if (x > 1 - m) ax -= (x - 1 + m) * 60 * s
    if (y < m) ay += (m - y) * 60 * s
    if (y > 1 - m) ay -= (y - 1 + m) * 60 * s
    vx[i] += ax * dt
    vy[i] += ay * dt
    var sp = max(hypot(vx[i], vy[i]), 0.001), k2 = clamp(sp, lo * s, hi * s) / sp
    vx[i] *= k2
    vy[i] *= k2
  }
  for (i = 0; i < count; i++) {
    px[i] = clamp(px[i] + vx[i] * dt, 0, 1)
    py[i] = clamp(py[i] + vy[i] * dt, 0, 1)
  }
}

export function renderBulk() {
  if (fadeK > 0) fade(fadeK)
  else clear()
  var gw = gridWidth()
  var p = 1 / max((gw > 0 ? gw : 32) - 1, 1)  // one pixel in panel units
  var L = 4.5 * p / speed                       // streak: about 4.5 px long
  for (var i = 0; i < count; i++) {
    var x = px[i], y = py[i]
    if (swarm == 0) {
      var h = atan2(vy[i], vx[i]) / PI2 + hueOff
      hsv(h, 0.9, 0.8)
      drawLine(x, y, x - vx[i] * L, y - vy[i] * L, p * 1.2, 2)
      hsv(h, 0.4, 1)
      splat(x, y, p * 1.3, 2)
    } else {
      hsv(i / count, 0.8, 1)
      splat(x, y, p * (1.2 + 1.4 * triangle(wt * 7 + hash(i + 0.5))), 1)   // flapping wings
    }
  }
}
