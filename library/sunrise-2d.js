// name: Sunrise 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sunrise 2D"; original source never consulted.

// A warm disc rises from the bottom of a 2D matrix, pauses, then burns as
// an active sun: plasma-like granulation boils over the disc while flare
// particles erupt from the limb, loop under constant-magnitude gravity,
// and fall back, leaving cooling trails. Moving the slider fades to black
// and restarts the sunrise.
//
// Off-screen 16x16 canvas updated at a fixed ~10 Hz simulation tick; each
// cell packs hue and brightness into one number: integer part = hue in
// thousandths of the wheel, fractional part = brightness.

var W = 16, H = 16
var cells = array(W * H)
var mask = array(W * H)        // precomputed radial disc falloff
var CX = 7.5, CY = 7.5
var SUN_R = 5.4                // ~a third of the display width

// precompute the disc mask once
var i, xx, yy
for (yy = 0; yy < H; yy++) {
  for (xx = 0; xx < W; xx++) {
    var d = hypot(xx - CX, yy - CY)
    mask[yy * W + xx] = max(0, 1 - d / SUN_R)
  }
}

// flare particles: constant-magnitude pull toward a jittered center
var NP = 20
var px = array(NP), py = array(NP), pvx = array(NP), pvy = array(NP)
var phue = array(NP)
for (i = 0; i < NP; i++) {
  var a = random(PI2), rr = random(SUN_R * 0.8)
  px[i] = CX + cos(a) * rr
  py[i] = CY + sin(a) * rr
  phue[i] = random(18) - 6      // personal warm hue offset, thousandths
}

// stages: 0 sunrise, 1 pause, 2 active, 3 fadeout
var stage = 0
var riseOffset = H             // cells still to rise
var stageSec = 0
var simAccum = 0
var TICK = 0.1                 // ~10 sim updates per second
var runSec = 0                 // for the slow drifts
var driftA, driftB
var baseHue = 40               // flare base hue drifts very slowly

var sliderSeen = 0
export function sliderMakeTheSunRiseAgain(v) {
  //# min=0 max=1 step=0.01 default=0
  // value ignored; any movement (after the initial load call) restarts
  if (sliderSeen && stage != 3) { stage = 3; stageSec = 0 }
  sliderSeen = 1
}

function stampSun() {
  var off = floor(riseOffset)
  for (var y = 0; y < H; y++) {
    var my = y - off
    if (my < 0 || my >= H) continue
    for (var x = 0; x < W; x++) {
      var m = mask[my * W + x]
      if (m <= 0) continue
      // additive plasma: two drifting-direction waves + a ring wave
      var nx = x / W, ny = y / H
      var p = (sin((nx * driftA + ny * (3 - driftA)) * PI2) +
               triangle(frac(nx * (2 - driftB) + ny * driftB)) * 2 - 1 +
               sin(m * PI2 * 1.5)) / 3
      p = (p + 1) / 2
      p = p * p * p                       // cube for contrast
      var b = clamp(m * (0.45 + 0.55 * p), 0.02, 0.999)
      var hue = 15 + floor(p * 55)        // deep orange-red -> gold
      cells[y * W + x] = hue + b
    }
  }
}

function coolCanvas() {
  for (var k = 0; k < W * H; k++) {
    var v = cells[k]
    var b = frac(v)
    if (b > 0) cells[k] = floor(v) + max(0, b - 0.09)
  }
}

function moveParticles() {
  // jittered center of gravity keeps orbits from settling
  var gx = CX + random(1.6) - 0.8
  var gy = CY + random(1.6) - 0.8
  for (var k = 0; k < NP; k++) {
    var dx = gx - px[k], dy = gy - py[k]
    var d = hypot(dx, dy)
    if (d > 0.15) {
      // constant-magnitude attraction (the falloff cancels): wide arcs
      pvx[k] = clamp(pvx[k] + dx / d * 0.55, -2.4, 2.4)
      pvy[k] = clamp(pvy[k] + dy / d * 0.55, -2.4, 2.4)
    }
    px[k] += pvx[k]
    py[k] += pvy[k]
    // draw only beyond ~0.9 sun radii: flares erupt from the limb
    var cxk = floor(px[k]), cyk = floor(py[k])
    if (cxk < 0 || cxk >= W || cyk < 0 || cyk >= H) continue
    if (hypot(px[k] - CX, py[k] - CY) < SUN_R * 0.9) continue
    var idx = cyk * W + cxk
    cells[idx] = floor(baseHue + phue[k]) + max(frac(cells[idx]), 0.55)
  }
}

function simTick() {
  driftA = 1.5 + sin(runSec * PI2 / 47) * 1.2   // tens-of-seconds drifts
  driftB = 1.5 + sin(runSec * PI2 / 83) * 1.2
  baseHue = 40 + sin(runSec * PI2 / 240) * 12   // flares drift over minutes

  if (stage == 0) {
    riseOffset = max(0, riseOffset - 0.32)      // full rise ~5 s
    stampSun()
    coolCanvas()
    if (riseOffset <= 0) { stage = 1; stageSec = 0 }
  } else if (stage == 1) {
    stampSun()
    coolCanvas()
    if (stageSec > 2) { stage = 2; stageSec = 0 }
  } else if (stage == 2) {
    stampSun()
    coolCanvas()
    moveParticles()
  } else {
    coolCanvas()                                // fade to black
    if (stageSec > 1.5) {
      riseOffset = H
      stage = 0
      stageSec = 0
      for (var k = 0; k < NP; k++) {
        var a2 = random(PI2), r2 = random(SUN_R * 0.8)
        px[k] = CX + cos(a2) * r2
        py[k] = CY + sin(a2) * r2
        pvx[k] = 0
        pvy[k] = 0
      }
    }
  }
}

export function beforeRender(delta) {
  var dt = delta / 1000
  runSec += dt
  if (runSec > 3600) runSec -= 3600
  stageSec += dt
  simAccum += dt
  while (simAccum >= TICK) {
    simAccum -= TICK
    simTick()
  }
}

export function render2D(index, x, y) {
  var v = cells[floor(y * 15.99) * 16 + floor(x * 15.99)]
  var b = frac(v)
  hsv(floor(v) / 1000, 1, b * b)
}
