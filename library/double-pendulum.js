// name: Double Pendulum
// tags: panel, nature
// Curated original (hand-written showcase of the Luxel language/builtins).
// The butterfly effect, live: a few double pendulums hang from one pivot,
// released a hair (0.0002 rad) apart. They swing as one (their additive
// ribbons sum to white), then chaos tears them apart into separate colours.
// Every Resync seconds they glide back together and are dropped again.
// Semi-implicit Euler with substeps; each tip keeps a ring of recent points
// drawn as fading drawLine ribbons, then rods (drawLine) and bobs (splat).

var MAXP = 5, NP = 48
var t1 = array(MAXP), t2 = array(MAXP), w1 = array(MAXP), w2 = array(MAXP)
var s1 = array(MAXP), s2 = array(MAXP)          // angles when a glide began
var rx = array(MAXP * NP), ry = array(MAXP * NP) // tip ribbons, ring per pendulum

var pend = 3
//# min=1 max=5 step=1 default=3
export function sliderPendulums(v) { pend = clamp(floor(v), 1, MAXP) }

// Seconds of ribbon behind each tip.
var trail = 2
//# min=0.5 max=6 step=0.1 default=2
export function sliderTrail(v) { trail = clamp(v, 0.5, 6) }

// Percent of swing speed lost per second.
var damping = 0.5
//# min=0 max=10 step=0.1 default=0.5
export function sliderDamping(v) { damping = clamp(v, 0, 10) }

// Seconds between re-synchronisations.
var resync = 60
//# min=10 max=180 step=1 default=60
export function sliderResync(v) { resync = clamp(v, 10, 180) }

var G = 18, GLIDE = 1.5
var clock = 0, cycle = 0, gliding = 0, a1 = 0, a2 = 0
var head = 0, sampT = 0, ax0 = 0.5, ay0 = 0.5, len = 0.24, sx = 1, sy = 1

function wrap(a) { return mod(a + PI, PI2) - PI }

function newStart() {           // a high, chaotic release, different every cycle
  a1 = (hash(cycle + 0.5) < 0.5 ? -1 : 1) * (1.9 + 0.8 * hash(cycle + 3.5))
  a2 = a1 + (hash(cycle + 7.5) - 0.5) * 2.4
  for (var p = 0; p < MAXP; p++) { s1[p] = t1[p]; s2[p] = t2[p] }
  gliding = GLIDE
}

var mx = 0, my = 0, tx = 0, ty = 0              // elbow and tip of the last tip() call
function tip(p) {
  mx = ax0 + sin(t1[p]) * len * sx
  my = ay0 + cos(t1[p]) * len * sy
  tx = mx + sin(t2[p]) * len * sx
  ty = my + cos(t2[p]) * len * sy
}

for (var q = 0; q < MAXP * NP; q++) { rx[q] = ax0; ry[q] = ay0 + 2 * len }
newStart()

export function beforeRender(delta) {
  var dt = min(delta, 60) / 1000, p, k
  clock += dt
  if (clock > resync) { clock = 0; cycle = (cycle + 1) % 1000; newStart() }
  if (gliding > 0) {             // ease every pendulum back to the shared start
    gliding = max(gliding - dt, 0)
    var e = easeInOutCubic(1 - gliding / GLIDE)
    for (p = 0; p < MAXP; p++) {
      t1[p] = s1[p] + wrap(a1 - s1[p]) * e
      t2[p] = s2[p] + wrap(a2 + p * 0.0002 - s2[p]) * e     // the hair of difference
      w1[p] = 0
      w2[p] = 0
    }
  } else {
    var n = clamp(ceil(dt / 0.004), 2, 15), h = dt / n
    var keep = pow(1 - damping / 100, h)
    for (p = 0; p < pend; p++) {
      var a = t1[p], b = t2[p], u = w1[p], v = w2[p]
      for (k = 0; k < n; k++) {  // equal masses and arms; the denominator stays in 2..4
        var d = a - b, sd = sin(d), cd = cos(d), den = 3 - cos(2 * d)
        var acc1 = (-3 * G * sin(a) - G * sin(a - 2 * b) - 2 * sd * (v * v + u * u * cd)) / den
        var acc2 = 2 * sd * (2 * u * u + 2 * G * cos(a) + v * v * cd) / den
        u = clamp((u + acc1 * h) * keep, -14, 14)
        v = clamp((v + acc2 * h) * keep, -14, 14)
        a = wrap(a + u * h)
        b = wrap(b + v * h)
      }
      t1[p] = a; t2[p] = b; w1[p] = u; w2[p] = v
    }
  }
  var gw = gridWidth(), gh = gridHeight()
  sx = gw > gh && gh > 0 ? gh / gw : 1         // keep it round on a wide panel
  sy = gh > gw && gw > 0 ? gw / gh : 1
  sampT += dt
  if (sampT >= trail / NP) {
    sampT = 0
    head = (head + 1) % NP
    for (p = 0; p < MAXP; p++) {
      tip(p)
      rx[p * NP + head] = tx
      ry[p * NP + head] = ty
    }
  }
}

export function renderBulk() {
  clear()
  var gw = gridWidth()
  var px = 1 / max((gw > 0 ? gw : 32) - 1, 1)
  var p, k
  for (p = 0; p < pend; p++) {                 // ribbons: newest bright, oldest gone
    var hue = p / pend + 0.02
    tip(p)
    var lx = tx, ly = ty, i = head
    for (k = 0; k < NP - 1; k++) {
      var f = 1 - k / (NP - 1)
      hsv(hue, 1, 0.75 * f * f)
      drawLine(lx, ly, rx[p * NP + i], ry[p * NP + i], px * (0.6 + 0.9 * f), 1)
      lx = rx[p * NP + i]
      ly = ry[p * NP + i]
      i = (i + NP - 1) % NP
    }
  }
  for (p = 0; p < pend; p++) {                 // rods and bobs over the ribbons
    var hue2 = p / pend + 0.02
    tip(p)
    hsv(hue2, 0.25, 0.35)
    drawLine(ax0, ay0, mx, my, px * 0.8, 2)
    drawLine(mx, my, tx, ty, px * 0.8, 2)
    hsv(hue2, 0.5, 0.8)
    splat(mx, my, px * 2.2, 2)
    hsv(hue2, 0.35, 1)
    splat(tx, ty, px * 2.8, 2)
  }
  hsv(0, 0, 0.8)
  splat(ax0, ay0, px * 1.6, 2)
}
