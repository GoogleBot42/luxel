// name: 80s kid show
// Clean-room reimplementation from a prose functional description of the
// community pattern "80s kid show"; original source never consulted.

// Brightly colored geometric shapes (circle, square, triangle, hexagon,
// six-pointed star) drift and bounce around the panel like a retro
// screensaver, each slowly spinning and breathing in size. Rendered per
// pixel with 2D signed distance functions; first hit wins, so array
// order is z-order, and pairs occasionally swap to vary stacking.
//
// Defaults are FILLED and FLAT — solid poster-color shapes, the way the
// Saturday-morning title card reads. The Filled and Flat toggles switch to
// thin outlines / distance-shaded neon glow respectively.

const POOL = 18

var px = array(POOL)
var py = array(POOL)
var vx = array(POOL)
var vy = array(POOL)
var hueA = array(POOL)
var shapeA = array(POOL)
var rotOff = array(POOL)
var sizeMul = array(POOL)
var brPeriod = array(POOL)
var brPhase = array(POOL)
var curSize = array(POOL)
var csA = array(POOL)
var snA = array(POOL)

// control state (defaults)
var numActive = 6
var shapeSel = 5      // 0..4 fixed shape, 5 = random mix
var nomSize = .13
var speedMul = .5
var filledOn = 1      // solid shapes by default (outline mode is the toggle)
var lineW = .045
var cutoffF = 1.6
var spinOn = 0
var flatOn = 1        // flat poster color by default (glow shading is the toggle)

var gPhase = 0
var gc = 1
var gs = 0

function initShapes() {
  var h0 = random(1)
  for (var i = 0; i < POOL; i++) {
    px[i] = random(1)
    py[i] = random(1)
    vx[i] = (random(1) - .5) * .3
    vy[i] = (random(1) - .5) * .3
    hueA[i] = h0 + i * .45
    shapeA[i] = shapeSel >= 5 ? floor(random(5)) : shapeSel
    rotOff[i] = random(PI2)
    sizeMul[i] = 1 + random(2)
    brPeriod[i] = 1 + random(55)   // seconds: ~1 s up to ~1 min
    brPhase[i] = random(1)
    curSize[i] = nomSize
    csA[i] = 1
    snA[i] = 0
  }
}
initShapes()

//# min=0 max=1 step=0.01 default=0.25
export function sliderNumberOfFloaters(v) {
  numActive = floor(2 + v * (POOL - 2) + .5)
}

//# min=0 max=1 step=0.01 default=1
export function sliderShapeType(v) {
  shapeSel = floor(v * 5.99)   // 0..4 = one shape, 5 = random mix
  initShapes()
}

//# min=0 max=1 step=0.01 default=0.4
export function sliderSize(v) { nomSize = .03 + v * .3 }

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) { speedMul = v }

// Solid fill vs thin outline. Filled is the default look.
//# default=1
export function toggleFilled(v) { filledOn = v }

//# min=0 max=1 step=0.01 default=0.3
export function sliderLineWidth(v) { lineW = .01 + v * .12 }

//# min=0 max=1 step=0.01 default=0.4
export function sliderCutoff(v) { cutoffF = 1 + v * 1.5 }

export function toggleSpin(v) { spinOn = v }

// Flat full-brightness color vs distance-shaded neon glow. Flat is the
// default look; turn this off for the soft-edged glow rendering.
//# default=1
export function toggleFlat(v) { flatOn = v }

// Swap two objects wholesale. This MUST move the per-frame derived cache
// (current size and the cached sin/cos) along with the state it was derived
// from: the swap happens after beforeRender has already filled the cache, so
// leaving curSize/csA/snA behind draws each shape at the other's position
// with the wrong size and rotation for exactly one frame — the single-frame
// flicker this pattern used to show every few seconds.
function swapObjects(a, b) {
  var t
  t = curSize[a]; curSize[a] = curSize[b]; curSize[b] = t
  t = csA[a]; csA[a] = csA[b]; csA[b] = t
  t = snA[a]; snA[a] = snA[b]; snA[b] = t
  t = px[a]; px[a] = px[b]; px[b] = t
  t = py[a]; py[a] = py[b]; py[b] = t
  t = vx[a]; vx[a] = vx[b]; vx[b] = t
  t = vy[a]; vy[a] = vy[b]; vy[b] = t
  t = hueA[a]; hueA[a] = hueA[b]; hueA[b] = t
  t = shapeA[a]; shapeA[a] = shapeA[b]; shapeA[b] = t
  t = rotOff[a]; rotOff[a] = rotOff[b]; rotOff[b] = t
  t = sizeMul[a]; sizeMul[a] = sizeMul[b]; sizeMul[b] = t
  t = brPeriod[a]; brPeriod[a] = brPeriod[b]; brPeriod[b] = t
  t = brPhase[a]; brPhase[a] = brPhase[b]; brPhase[b] = t
}

// signed distance to shape kind k, local coords, radius r
function shapeDist(k, x, y, r) {
  var ax
  var ay
  var m
  if (k == 0) {                       // circle
    return hypot(x, y) - r
  }
  if (k == 1) {                       // square
    ax = abs(x) - r
    ay = abs(y) - r
    return hypot(max(ax, 0), max(ay, 0)) + min(max(ax, ay), 0)
  }
  if (k == 2) {                       // equilateral triangle
    var kk = 1.7320508
    ax = abs(x) - r
    ay = y + r / kk
    if (ax + kk * ay > 0) {
      m = (ax - kk * ay) / 2
      ay = (-kk * ax - ay) / 2
      ax = m
    }
    ax -= clamp(ax, -2 * r, 0)
    return -hypot(ax, ay) * sign(ay)
  }
  if (k == 3) {                       // hexagon
    ax = abs(x)
    ay = abs(y)
    m = 2 * min(-.8660254 * ax + .5 * ay, 0)
    ax -= m * -.8660254
    ay -= m * .5
    ax -= clamp(ax, -.5773503 * r, .5773503 * r)
    ay -= r
    return hypot(ax, ay) * sign(ay)
  }
  // hexagram (six-pointed star)
  var rr = r * .6
  ax = abs(x)
  ay = abs(y)
  m = 2 * min(-.5 * ax + .8660254 * ay, 0)
  ax -= m * -.5
  ay -= m * .8660254
  m = 2 * min(.8660254 * ax + -.5 * ay, 0)
  ax -= m * .8660254
  ay -= m * -.5
  ax -= clamp(ax, .5773503 * rr, 1.7320508 * rr)
  ay -= rr
  return hypot(ax, ay) * sign(ay)
}

export function beforeRender(delta) {
  var dt = delta / 1000
  gPhase = time(.12) * PI2        // full scene turn every ~7.9 s
  gc = cos(-gPhase)
  gs = sin(-gPhase)
  var sp = speedMul * 2
  for (var i = 0; i < numActive; i++) {
    // move & bounce
    px[i] += vx[i] * sp * dt
    py[i] += vy[i] * sp * dt
    if (px[i] < 0) { px[i] = 0; vx[i] = -vx[i] }
    if (px[i] > 1) { px[i] = 1; vx[i] = -vx[i] }
    if (py[i] < 0) { py[i] = 0; vy[i] = -vy[i] }
    if (py[i] > 1) { py[i] = 1; vy[i] = -vy[i] }
    // spin: cache sin/cos of global phase + per-object offset
    var a = gPhase + rotOff[i]
    csA[i] = cos(a)
    snA[i] = sin(a)
    // breathe: triangle wave between half and 1.5x nominal
    brPhase[i] += dt / brPeriod[i]
    if (brPhase[i] > 1) brPhase[i] -= 1
    curSize[i] = sizeMul[i] * nomSize * (.5 + triangle(brPhase[i]))
  }
  // occasional z-order shuffle: swap two random active objects wholesale.
  // Rate is per SECOND (about one swap every three seconds, what the pattern
  // is meant to do), not per frame — a flat per-frame chance fired several
  // times a second on a fast rig, which is the other half of what read as
  // random popping.
  if (numActive > 1 && random(1) < dt / 3) {
    var a1 = floor(random(numActive))
    var b1 = floor(random(numActive))
    if (a1 != b1) swapObjects(a1, b1)
  }
}

export function render2D(index, x, y) {
  if (spinOn) {   // rotate the whole scene about the panel center
    var rx = x - .5
    var ry = y - .5
    x = .5 + rx * gc - ry * gs
    y = .5 + rx * gs + ry * gc
  }
  var v = 0
  var h = 0
  var s = 1
  for (var i = 0; i < numActive; i++) {
    var r = curSize[i]
    var dx = x - px[i]
    var dy = y - py[i]
    var bb = r * cutoffF
    if (abs(dx) > bb || abs(dy) > bb) continue   // cheap bbox reject
    var lx = dx * csA[i] + dy * snA[i]
    var ly = -dx * snA[i] + dy * csA[i]
    var d = shapeDist(shapeA[i], lx, ly, r)
    var e = filledOn ? d : abs(d)
    if (e < lineW) {                             // first hit wins
      h = hueA[i]
      if (flatOn) {
        v = 1
        s = 1
      } else {
        v = saturate(1 - e / lineW)              // shade by distance
        s = saturate(abs(d) * 3 / lineW)         // bleach the exact edge
      }
      break
    }
  }
  hsv(h, s, v * v)
}
