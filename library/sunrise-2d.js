// name: Sunrise 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sunrise 2D"; original source never consulted.

// A warm sun disc rises from the bottom of a 2D matrix over several
// seconds, pauses, then "activates": its surface boils with plasma-like
// granulation while a couple dozen flare particles erupt off the limb,
// loop under constant-magnitude gravity toward a jittered center, and fall
// back, leaving fading trails. Moving the slider fades to black and
// restarts the sunrise.
//
// Simulated on a 16x16 virtual canvas at a fixed ~10 Hz tick, independent
// of LED frame rate; render2D just samples the canvas. Each cell packs
// hue and brightness into one number: integer part = hue in thousandths
// of the wheel, fractional part = brightness (the original's key trick).

var W = 16
var H = 16
var buf = array(W * H)

// --- precomputed sun disc mask: linear falloff, full center -> zero rim ---
var SUN_R = 5.3                     // ~a third of the display width, cells
var CX = 7.5
var CY = 7.5
var mask = array(W * H)
for (i = 0; i < W * H; i++) {
  var d = hypot(i % W - CX, floor(i / W) - CY)
  mask[i] = d < SUN_R ? 1 - d / SUN_R : 0
}

// --- flare particles ---
var NP = 22
var px = array(NP), py = array(NP), pvx = array(NP), pvy = array(NP)
var phue = array(NP)                // personal warm hue offset, thousandths
for (i = 0; i < NP; i++) {
  var a = random(PI2)
  var rr = random(SUN_R * 0.8)
  px[i] = CX + cos(a) * rr
  py[i] = CY + sin(a) * rr
  phue[i] = random(45)              // reds through oranges
}

var PULL = 0.10                     // constant-magnitude attraction
var VMAX = 1.6                      // per-axis "speed of light", cells/tick
var COOL = 0.11                     // brightness lost per tick (~1 s fade)
var BASE_HUE = 18                   // deep orange-red, thousandths of wheel

// --- stage machine ---
var STAGE_RISE = 0
var STAGE_PAUSE = 1
var STAGE_ACTIVE = 2
var STAGE_FADE = 3
var stage = STAGE_RISE
var RISE_START = 14                 // cells below final position
var riseOfs = RISE_START
var stageT = 0                      // seconds in current stage
var booted = 0

var msAcc = 0
var TICK_MS = 100                   // ~10 simulation updates per second
var tSec = 0                        // for the slow drifts

export function sliderRestartSunrise(v) {
  //# min=0 max=1 step=0.1 default=0
  // Value ignored: any movement fades out and restarts the sunrise.
  if (booted) {
    stage = STAGE_FADE
    stageT = 0
  }
}

function stampSun() {
  // Drifting plasma coefficients: periods of tens of seconds to a minute+.
  var c1 = 3 + 2 * sin(time(0.55) * PI2)     // ~36 s
  var c2 = 4 + 2.5 * sin(time(1.1) * PI2)    // ~72 s
  var ofs = floor(riseOfs)
  for (var i = 0; i < W * H; i++) {
    var m = mask[i]
    if (m <= 0) continue
    var x = i % W
    var y = floor(i / W) + ofs
    if (y >= H) continue
    // Additive plasma: sine of one x/y combo, triangle of another, sine
    // of the mask itself; average and cube for contrast.
    var nx = x / W, ny = y / H
    var p = (wave(nx * c1 + ny * (5 - c1) + tSec * 0.13)
           + triangle(frac(nx * c2 + ny * 2.5 - tSec * 0.09))
           + wave(m * 0.8)) / 3
    p = p * p * p
    // Mask-weighted brightness mix keeps the center brightest; hue shifts
    // toward gold where the granulation is hot.
    var b = clamp(m * (0.35 + 0.85 * p), 0, 0.98)
    buf[y * W + x] = BASE_HUE + floor(p * 55) + b
  }
}

function coolCanvas() {
  for (var i = 0; i < W * H; i++) {
    var v = buf[i]
    var b = frac(v)
    if (b > 0) {
      b -= COOL
      buf[i] = b > 0 ? trunc(v) + b : 0
    }
  }
}

function runParticles() {
  // Jittered center of gravity "stirs" the system so orbits never settle.
  var gx = CX + random(1.2) - 0.6
  var gy = CY + random(1.2) - 0.6
  var hueDrift = time(2) * 30       // base flare hue creeps over minutes
  for (var i = 0; i < NP; i++) {
    var dx = gx - px[i]
    var dy = gy - py[i]
    var d = hypot(dx, dy)
    if (d > 0.2) {                  // tiny inner cutoff
      // Constant-magnitude pull regardless of distance — this is what
      // makes the wide looping arcs.
      pvx[i] = clamp(pvx[i] + dx / d * PULL, -VMAX, VMAX)
      pvy[i] = clamp(pvy[i] + dy / d * PULL, -VMAX, VMAX)
    }
    px[i] += pvx[i]
    py[i] += pvy[i]
    // Draw only beyond ~9/10 of the sun radius (flares erupt from the
    // limb, never crawl across the face) and only when on-screen;
    // off-screen particles keep simulating and get pulled back.
    var rr = hypot(px[i] - CX, py[i] - CY)
    if (rr < SUN_R * 0.9) continue
    var cx = floor(px[i])
    var cy = floor(py[i])
    if (cx < 0 || cx >= W || cy < 0 || cy >= H) continue
    var cell = cy * W + cx
    var b = frac(buf[cell])
    buf[cell] = BASE_HUE + phue[i] + hueDrift + max(b, 0.55)
  }
}

function tick(dt) {
  stageT += dt
  if (stage == STAGE_RISE) {
    stampSun()
    coolCanvas()
    riseOfs = RISE_START * (1 - stageT / 5)   // full rise in ~5 s
    if (riseOfs <= 0) {
      riseOfs = 0
      stage = STAGE_PAUSE
      stageT = 0
    }
  } else if (stage == STAGE_PAUSE) {
    stampSun()
    coolCanvas()
    if (stageT > 2) { stage = STAGE_ACTIVE; stageT = 0 }
  } else if (stage == STAGE_ACTIVE) {
    stampSun()
    coolCanvas()
    runParticles()
  } else {                                    // STAGE_FADE
    coolCanvas()                              // only cool: image dies away
    if (stageT > 1.5) {
      stage = STAGE_RISE
      stageT = 0
      riseOfs = RISE_START
      for (var i = 0; i < NP; i++) { pvx[i] = 0; pvy[i] = 0 }
    }
  }
}

export function beforeRender(delta) {
  booted = 1
  tSec += delta / 1000
  if (tSec > 3600) tSec -= 3600
  msAcc += delta
  var guard = 0
  while (msAcc >= TICK_MS && guard < 5) {     // catch up, bounded
    msAcc -= TICK_MS
    tick(TICK_MS / 1000)
    guard++
  }
  if (guard == 5) msAcc = 0
}

export function render2D(index, x, y) {
  var v = buf[floor(y * 15.99) * 16 + floor(x * 15.99)]
  hsv(trunc(v) / 1000, 1, frac(v))
}
