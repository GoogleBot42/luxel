// name: Bouncy Boxes
// Clean-room reimplementation from a prose functional description of the
// community pattern "Bouncy Boxes"; original source never consulted.

// Four solid soft-edged squares glide around a 2D surface at constant speed,
// wrapping horizontally (cylinder seam-safe: all horizontal math uses the
// shortest signed distance) and bouncing off top/bottom. Squares collide as
// rigid bodies — pushed apart along the axis of least penetration, elastic
// velocity exchange — and every frame each velocity is renormalized to a
// fixed speed so the scene never slows down. Each square carries a rainbow
// hue a quarter-wheel from its siblings, rotating over ~10 s, with a radial
// hue gradient from its center. Optional "digital glitch" garnish: hashed
// per-tick sparkles, a short horizontal streak, and up to three tearing
// bands — rows whose sample position is shifted sideways so squares passing
// through appear sliced. All glitch randomness is a stateless hash of a
// coarse tick counter, so glitches hold still between re-rolls.
// Original was hardcoded to a 32x8 serpentine cylinder; this version uses
// the supplied normalized 2D coordinates (the spec's "obvious fix") with a
// 16-row virtual grid for tear/sparkle quantization.

var ROWS = 16                // virtual rows for tears / sparkles / streaks
var COLS = 16
var N = 4                    // squares
var S = 0.3                  // square side, normalized units
var SPEED = 0.22             // constant speed, units/sec (crosses in a few s)
var SOFT = 1 / 16            // ~one pixel of edge falloff
var HALFDIAG = S * 0.7071

// ---- controls --------------------------------------------------------------
var sparkleRate = 0          // default off
var sparkleBright = 0        // default off
var sparkleSat = 1
var tearChance = 0.15        // default: rare
var tearRate = 0.5           // 0..1 -> up to ~10 re-rolls per second
var tearBright = 0.5

//# min=0 max=1 step=0.01 default=0
export function sliderSparkleRate(v) { sparkleRate = v * 0.25 }
//# min=0 max=1 step=0.01 default=0
export function sliderSparkleBrightness(v) { sparkleBright = v }
//# min=0 max=1 step=0.01 default=1
export function sliderSparkleSaturation(v) { sparkleSat = v }
//# min=0 max=1 step=0.01 default=0.15
export function sliderTearChance(v) { tearChance = v }
//# min=0 max=1 step=0.01 default=0.5
export function sliderTearRate(v) { tearRate = v }
//# min=0 max=1 step=0.01 default=0.5
export function sliderTearBrightness(v) { tearBright = v }

// ---- square state -----------------------------------------------------------
var px = array(N)            // top-left corner, x wraps in [0,1)
var py = array(N)
var vx = array(N)
var vy = array(N)
var hue = array(N)

var k
for (k = 0; k < N; k++) {
  px[k] = k / N                          // spread evenly around the cylinder
  py[k] = mod(0.1 + k * 0.17, 1 - S)     // varied heights
  var a = 0.5 + k * 1.7                  // varied diagonal directions
  vx[k] = SPEED * cos(a)
  vy[k] = SPEED * sin(a)
}

// tear band records (recomputed from a hashed tick, so they hold still)
var tearRow = array(3)
var tearShift = array(3)
var tearHue = array(3)
var tearOn = array(3)
var sparkTick = 0
var accumMs = 0

function shortDist(d) { return mod(d + 0.5, 1) - 0.5 }   // seam-safe signed dx

export function beforeRender(delta) {
  var dt = min(delta, 40) / 1000        // clamp so physics stays stable
  accumMs += delta
  if (accumMs > 30000) accumMs -= 30000  // keep well inside 16.16 range

  // 1. integrate, wrap, bounce
  var i, j
  for (i = 0; i < N; i++) {
    px[i] = mod(px[i] + vx[i] * dt, 1)
    py[i] += vy[i] * dt
    if (py[i] < 0) { py[i] = -py[i]; vy[i] = abs(vy[i]) }
    if (py[i] > 1 - S) { py[i] = 2 * (1 - S) - py[i]; vy[i] = -abs(vy[i]) }
  }

  // 2. rigid collisions: several relaxation passes over all pairs
  var pass
  for (pass = 0; pass < 3; pass++) {
    for (i = 0; i < N - 1; i++) {
      for (j = i + 1; j < N; j++) {
        var dx = shortDist((px[j] + S / 2) - (px[i] + S / 2))
        var dy = (py[j] + S / 2) - (py[i] + S / 2)
        var penX = S - abs(dx)
        var penY = S - abs(dy)
        if (penX <= 0 || penY <= 0) continue
        if (penX < penY) {               // resolve along x
          var sx = dx >= 0 ? 1 : -1
          var push = (penX - 0.002) * 0.45
          px[i] = mod(px[i] - sx * push, 1)
          px[j] = mod(px[j] + sx * push, 1)
          if ((vx[j] - vx[i]) * sx < 0) {  // approaching: swap x velocities
            var tmp = vx[i]; vx[i] = vx[j]; vx[j] = tmp
          }
        } else {                         // resolve along y
          var sy = dy >= 0 ? 1 : -1
          var pushY = (penY - 0.002) * 0.45
          py[i] = clamp(py[i] - sy * pushY, 0, 1 - S)
          py[j] = clamp(py[j] + sy * pushY, 0, 1 - S)
          if ((vy[j] - vy[i]) * sy < 0) {
            var tmp2 = vy[i]; vy[i] = vy[j]; vy[j] = tmp2
          }
        }
      }
    }
  }

  // 3. renormalize speed: collisions redirect, liveliness never decays
  for (i = 0; i < N; i++) {
    var sp = hypot(vx[i], vy[i])
    if (sp > 0.001) {
      vx[i] = vx[i] * SPEED / sp
      vy[i] = vy[i] * SPEED / sp
    } else {
      vx[i] = SPEED * 0.7; vy[i] = SPEED * 0.7
    }
  }

  // 4. hues: one slow rainbow phase, squares offset by quarter wheels
  var base = time(0.15)                  // ~9.8 s full rotation
  for (i = 0; i < N; i++) hue[i] = base + i * 0.25

  // 5. glitch state, all hashed from coarse tick counters
  sparkTick = floor(accumMs / 50)        // sparkles re-roll 20x/sec
  var tearMs = 1000 / (0.5 + tearRate * 9.5)
  var tick = floor(accumMs / tearMs)
  for (i = 0; i < 3; i++) {
    tearOn[i] = hash2(tick, i * 4 + 1) < tearChance
    tearRow[i] = floor(hash2(tick, i * 4 + 2) * ROWS)
    tearShift[i] = (hash2(tick, i * 4 + 3) - 0.5) * 0.66  // up to ~1/3 around
    tearHue[i] = hash2(tick, i * 4 + 4)
  }
}

export function render2D(index, x, y) {
  var row = floor(y * 15.99)
  var sx = x
  var inTear = 0
  var tHue = 0
  var b
  for (b = 0; b < 3; b++) {
    if (tearOn[b] && tearRow[b] == row) {
      sx = mod(sx + tearShift[b], 1)     // lie about the sample position only
      inTear = 1
      tHue = tearHue[b]
    }
  }

  // soft-box coverage of each square at the (possibly shifted) sample point
  var best = 0, bestK = 0, total = 0, bestOx = 0, bestOy = 0
  var i
  for (i = 0; i < N; i++) {
    var ox = shortDist(sx - (px[i] + S / 2))
    var oy = y - (py[i] + S / 2)
    var c = min(S / 2 - abs(ox), S / 2 - abs(oy)) / SOFT
    c = clamp(c, 0, 1)
    total += c
    if (c > best) { best = c; bestK = i; bestOx = ox; bestOy = oy }
  }

  if (best > 0) {
    var d = hypot(bestOx, bestOy) / HALFDIAG          // radial hue gradient
    var v = min(total, 1) + (inTear ? 0.25 : 0)
    hsv(hue[bestK] + d * 0.55, 1, min(v, 1))
    return
  }
  if (inTear) {
    hsv(tHue, 0.15, tearBright)          // pale near-white torn band
    return
  }
  // sparkles + one short streak, per-tick hashed so they hold still
  if (sparkleBright > 0) {
    var cell = row * COLS + floor(x * 15.99)
    if (hash2(sparkTick, cell) < sparkleRate) {
      hsv(hash2(sparkTick, cell + 300), sparkleSat, sparkleBright)
      return
    }
    var sRow = floor(hash2(sparkTick, 777) * ROWS)
    var sCol = hash2(sparkTick, 888)
    if (row == sRow && abs(shortDist(x - sCol)) < 1.5 / COLS) {
      hsv(hash2(sparkTick, 999), sparkleSat, sparkleBright * 0.6)
      return
    }
  }
  rgb(0, 0, 0)
}

// 1D fallback: this pattern needs a 2D map
export function render(index) { rgb(0, 0, 0) }
