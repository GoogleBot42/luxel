// name: Wander Edges
// tags: both, ambient
// Clean-room reimplementation from the prose spec docs/pattern-specs/wanderedges.md
// — reimagined, not a replica.
// Soft pulses swell, drift and fade, summed into one field read through a
// colour ramp that peaks at MID intensity and is black again at full: a
// swelling pulse hollows into a bright rim, and overlapping pulses merge into
// figure-eights and split apart. Strip: a capped 1-D field; panel: radial
// pulses on a <= 32x32 canvas, so the rims become wandering contour rings
// (bilinear fillCanvas). One 1024-cell array either way.

var rimH = 0.33        // green
var rimS = 1
var life = 6           // seconds a pulse lives
var spawnRate = 1.5    // pulses per second
var foot = 12          // pulse footprint, % of the strip (or panel)

// Rim colour (hue + saturation; brightness comes from the field).
export function hsvPickerRim(h, s, v) { rimH = h; rimS = s }
//# min=1 max=20 step=0.5 default=6
export function sliderLifetimeSeconds(v) { life = clamp(v, 1, 20) }
//# min=0.2 max=10 step=0.1 default=1.5
export function sliderSpawnPerSecond(v) { spawnRate = clamp(v, 0.2, 10) }
// Pulse width as a percentage of the strip (on a panel, its radius is twice this % of the width).
//# min=2 max=40 step=1 default=12
export function sliderFootprintPercent(v) { foot = clamp(v, 2, 40) }

var CAP = 1024
var field = array(CAP)
var MAXP = 32
var px = array(MAXP)
var py = array(MAXP)
var vx = array(MAXP)
var vy = array(MAXP)
var age = array(MAXP)
var plife = array(MAXP)   // 0 = free slot
var acc = 0
var dt = 0
var seeded = 0

function spawn(a) {
  for (var i = 0; i < MAXP; i++) {
    if (plife[i] == 0) {
      px[i] = random(1)
      py[i] = random(1)
      var ang = random(PI2)
      var sp = (0.3 + random(0.7)) * foot / 100 / life   // drift: under one footprint per life
      vx[i] = cos(ang) * sp
      vy[i] = sin(ang) * sp
      plife[i] = life * (0.8 + random(0.4))
      age[i] = a * plife[i]
      return
    }
  }
}

export function beforeRender(delta) {
  dt = min(delta, 200) / 1000
  if (!seeded) {
    seeded = 1
    for (var k = 0; k < min(spawnRate * life * 0.8, MAXP); k++) spawn(random(1))
  }
  acc += spawnRate * dt
  while (acc >= 1) { acc -= 1; spawn(0) }
  for (var i = 0; i < MAXP; i++) {
    if (plife[i] == 0) continue
    age[i] += dt
    if (age[i] >= plife[i]) { plife[i] = 0; continue }
    px[i] += vx[i] * dt
    py[i] += vy[i] * dt
    if (px[i] < 0 || px[i] > 1) vx[i] = -vx[i]    // bounce off the ends
    if (py[i] < 0 || py[i] > 1) vy[i] = -vy[i]
  }
}

// The mid-peaked ramp: black at the ends, full at 1/3 (band = half-width).
function ramp(n, band) {
  for (var c = 0; c < n; c++) {
    var d = abs(min(field[c], 2 / 3) * 1.5 - 0.5)
    var g = d >= band ? 0 : 1 - d / band
    field[c] = g * g
  }
}

function strip() {
  var n = min(pixelCount, CAP)
  var half = foot / 200 * n
  var inv = 1 / half
  feedback(field, 0)
  for (var i = 0; i < MAXP; i++) {
    if (plife[i] == 0) continue
    var env = sin(PI * age[i] / plife[i])
    var cx = px[i] * n
    var c1 = min(ceil(cx + half), n - 1)
    for (var c = max(floor(cx - half), 0); c <= c1; c++) {
      var s = 1 - abs(c + 0.5 - cx) * inv
      if (s > 0) field[c] += env * s
    }
  }
  ramp(n, 0.35)
  if (n == pixelCount) { fillHSV(rimH, rimS, field); return }
  var w = pixelCount / n          // a long strip: one run per field cell
  for (var c2 = 0; c2 < n; c2++) {
    hsv(rimH, rimS, field[c2])
    fillRange(floor(c2 * w), floor((c2 + 1) * w))
  }
}

function panel() {
  var gw = gridWidth()
  var gh = gridHeight()
  var W = gw > 0 ? min(gw, 32) : 32
  var H = gh > 0 ? min(gh, 32) : 32
  var R = foot / 50 * W
  var inv = 1 / (R * R)
  feedback(field, 0)
  for (var i = 0; i < MAXP; i++) {
    if (plife[i] == 0) continue
    var env = sin(PI * age[i] / plife[i])
    var cx = px[i] * W
    var cy = py[i] * H
    var x0 = max(floor(cx - R), 0)
    var x1 = min(ceil(cx + R), W - 1)
    var y1 = min(ceil(cy + R), H - 1)
    for (var y = max(floor(cy - R), 0); y <= y1; y++) {
      var dy = y + 0.5 - cy
      var row = y * W
      for (var x = x0; x <= x1; x++) {
        var dx = x + 0.5 - cx
        var s = 1 - (dx * dx + dy * dy) * inv
        if (s > 0) field[row + x] += env * s
      }
    }
  }
  blur2D(field, W, H, 1)           // smooth the field so the rims stay unbroken
  ramp(W * H, 0.45)               // wider band: a coarse canvas needs thicker rims
  fillCanvas(rimH, rimS, field, W, H, 1)
}

export function render(index) {}  // keeps a strip mapless; renderBulk paints

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
