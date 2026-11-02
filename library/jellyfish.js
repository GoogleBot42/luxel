// name: Jellyfish
// tags: panel, nature
// Curated original (hand-written showcase of the Luxel language/builtins).
// Translucent jellyfish pulse up through dark water: each bell snaps shut
// fast and relaxes slowly, every contraction a kick upward, while tentacles
// trail in a wave that travels down their length and plankton sparkle as a
// jelly drifts by. renderBulk: a water gradient, then per jelly a glow splat,
// a dome arc and margin of drawLines, four inner splats, oral arms and
// tentacle polylines; plankton are single splats.

var NJ = 4, NP = 20

var count = 3
//# min=1 max=4 step=1 default=3
export function sliderCount(v) { count = clamp(floor(v), 1, NJ) }

var pulseSec = 2.6
//# min=1 max=6 step=0.1 default=2.6
export function sliderPulseSeconds(v) { pulseSec = clamp(v, 1, 6) }

// Tentacle length as a percentage of the panel height.
var tentPct = 30
//# min=10 max=60 step=1 default=30
export function sliderTentacleLength(v) { tentPct = clamp(v, 10, 60) }

// Rotates the whole cyan-to-magenta palette, in degrees.
var hueShift = 0
//# min=0 max=359 step=1 default=0
export function sliderHueShift(v) { hueShift = clamp(v, 0, 359) / 360 }

var jx = array(NJ), jy = array(NJ), jr = array(NJ), jh = array(NJ)
var jph = array(NJ), jrate = array(NJ), jvy = array(NJ), jsw = array(NJ)
var jc = array(NJ), jbw = array(NJ), jbh = array(NJ)
var px = array(NP), py = array(NP), pz = array(NP)
var HUES = [0.5, 0.78, 0.9, 0.58]
var t = 0, pr = 0.02

function spawn(k, y) {
  jr[k] = 0.1 + random(0.07)
  jx[k] = 0.15 + random(0.7)
  jy[k] = y
  jh[k] = HUES[floor(random(4))] + random(0.06) - 0.03
  jph[k] = random(1)
  jrate[k] = 0.85 + random(0.3)
  jvy[k] = 0
  jsw[k] = random(PI2)
}
for (var k0 = 0; k0 < NJ; k0++) spawn(k0, 0.2 + k0 * 0.3 + random(0.15))
for (var p0 = 0; p0 < NP; p0++) { px[p0] = random(1); py[p0] = random(1); pz[p0] = random(1) }

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  t = mod(t + dt, 3600)
  pr = max(1.3 / max(gridWidth(), 24), 0.016)
  for (var k = 0; k < count; k++) {
    var p = jph[k] + dt / (pulseSec * jrate[k])
    if (p >= 1) p -= 1
    jph[k] = p
    // fast contract, slow relax; the contraction is the thrust
    var c = p < 0.22 ? easeOutQuad(p / 0.22) : 1 - easeInOutSine((p - 0.22) / 0.78)
    if (p < 0.22) jvy[k] -= jr[k] * 2 * dt / 0.22 / pulseSec
    jvy[k] = jvy[k] * exp(-dt * 1.6) + 0.006 * dt
    jy[k] += jvy[k] * dt
    jx[k] += 0.012 * sin(t * 0.4 + jsw[k]) * dt
    jc[k] = c
    jbw[k] = jr[k] * (1 - 0.3 * c)
    jbh[k] = jr[k] * (0.8 + 0.3 * c)
    if (jy[k] < -jr[k] - tentPct / 100) spawn(k, 1 + jr[k] * 0.4)
  }
  // plankton drift and wrap
  for (var i = 0; i < NP; i++) {
    py[i] = mod(py[i] + (0.004 + pz[i] * 0.006) * dt, 1)
    px[i] = mod(px[i] + 0.004 * sin(t * 0.3 + i) * dt, 1)
  }
}

function drawJelly(k) {
  var x = jx[k], y = jy[k], bw = jbw[k], bh = jbh[k], h = jh[k] + hueShift, c = jc[k]
  var ym = y + bh                           // the bell margin
  var lw = max(0.7 / max(gridHeight(), 24), 0.012)
  // tentacles: a wave travels down; a kick pulls them in and stretches them
  var n = 5, segs = 4, len = tentPct / 100 * (1 + 0.25 * c) / segs
  for (var i = 0; i < n; i++) {
    var ax = x + bw * (-0.8 + 1.6 * i / (n - 1)), ay = ym
    var ph = t * 2.2 + i * 1.7 + jsw[k]
    for (var j = 1; j <= segs; j++) {
      var f = j / segs
      var bx = x + (ax - x) * (1 - 0.35 * f * (0.5 + c)) + bw * 0.9 * f * sin(ph - j * 0.9)
      var by = ay + len * j
      hsv(h + 0.12 * f, 0.7, 0.55 * (1 - f * 0.75))
      drawLine(ax, ay, bx, by, lw, 1)
      ax = bx
      ay = by
    }
  }
  // oral arms: short frilled ribbons down the middle
  for (var a = -1; a <= 1; a += 2) {
    var ox = x + a * bw * 0.15, oy = ym
    for (var s = 1; s <= 3; s++) {
      var nx = x + a * bw * (0.15 + 0.12 * sin(t * 3 + s + a)), ny = ym + bh * 0.45 * s
      hsv(h + 0.35, 0.6, 0.5 - s * 0.1)
      drawLine(ox, oy, nx, ny, lw * 1.6, 1)
      ox = nx
      oy = ny
    }
  }
  // translucent body glow, then the dome arc and the bright margin
  hsv(h, 0.8, 0.3 + 0.15 * c)
  splat(x, ym - bh * 0.45, bw * 1.05, 1)
  hsv(h, 0.55, 0.85)
  var lx = x - bw, ly = ym
  for (var q = 1; q <= 8; q++) {
    var th = PI * q / 8
    var nx2 = x - bw * cos(th), ny2 = ym - bh * sin(th)
    drawLine(lx, ly, nx2, ny2, lw, 2)
    lx = nx2
    ly = ny2
  }
  hsv(h + 0.05, 0.4, 0.9)
  drawLine(x - bw, ym, x + bw, ym, lw * 1.2, 2)
  // four inner gonads in a ring
  hsv(h + 0.3, 0.7, 0.55)
  for (var g = 0; g < 4; g++) {
    var ga = g * PI / 2 + PI / 4
    splat(x + cos(ga) * bw * 0.35, ym - bh * 0.45 + sin(ga) * bh * 0.22, bw * 0.28, 1)
  }
}

export function renderBulk() {
  fillGradient(0.56 + hueShift, 1, 0.13, 0.64 + hueShift, 1, 0.025, 2)
  // a few faint shafts of light from the surface
  for (var r = 0; r < 2; r++) {
    var sx = 0.3 + r * 0.4 + 0.08 * sin(t * 0.25 + r * 2)
    hsv(0.55 + hueShift, 0.6, 0.07)
    drawLine(sx, 0, sx - 0.06, 1, 0.06, 1)
  }
  // plankton: dim specks that sparkle when a jelly passes close
  for (var i = 0; i < NP; i++) {
    var lit = 0
    for (var k = 0; k < count; k++) {
      var dx = px[i] - jx[k], dy = py[i] - jy[k] - jbh[k] * 1.5
      lit = max(lit, 1 - (dx * dx + dy * dy) / (jr[k] * jr[k] * 5))
    }
    var tw = 0.5 + 0.5 * sin(t * 9 + i * 2.3)
    hsv(0.5 + hueShift, 0.4 - 0.35 * lit, 0.18 + max(lit, 0) * tw)
    splat(px[i], py[i], pr, 2)
  }
  for (var k2 = 0; k2 < count; k2++) drawJelly(k2)
}
