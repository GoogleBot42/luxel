// name: Bouncy Boxes
// Clean-room reimplementation from a prose functional description of the
// community pattern "Bouncy Boxes"; original source never consulted.
//
// Four solid squares glide over a cylindrical matrix: wrapping horizontally,
// bouncing off top and bottom, colliding elastically (impulse swap + per-frame
// speed renormalization, so they never overlap and never slow down). Rainbow
// hues rotate slowly, offset a quarter wheel per square, with a radial hue
// gradient inside each square. Optional "digital glitch" garnish: hashed
// sparkles, a short streak, and up to three frozen tearing bands that shift
// a whole row's sample position sideways.
//
// Simulated on a 16x16 virtual canvas (wrapping horizontally = the cylinder);
// render2D samples the canvas, so any mapped layout works.

var W = 16
var H = 16
var CELLS = 256
var S = 4                  // square side, in cells (half the original height)
var NB = 4                 // number of squares
var TARGET_SPEED = 6       // cells per second, renormalized every frame

// canvas
var hc = array(CELLS)
var sc = array(CELLS)
var vc = array(CELLS)

// square state: top-left corner (x wraps), velocity
var bx = array(NB)
var by = array(NB)
var bvx = array(NB)
var bvy = array(NB)
bx[0] = 1;  by[0] = 1;  bvx[0] = 4.2;  bvy[0] = 4.2
bx[1] = 5;  by[1] = 9;  bvx[1] = -4.2; bvy[1] = 4.2
bx[2] = 9;  by[2] = 4;  bvx[2] = 4.2;  bvy[2] = -4.2
bx[3] = 13; by[3] = 11; bvx[3] = -3;   bvy[3] = 5

// glitch state
var sparkAcc = 0, sparkTick = 0
var tearAcc = 0, tearTick = 0
var tearRow = array(3)
var tearShift = array(3)
var tearOn = array(3)
var tearHueArr = array(3)
var hueBase = 0

// ---- controls -----------------------------------------------------------------
var sparkRate = 0
export function sliderSparkleRate(v) {
  //# min=0 max=1 step=0.01 default=0
  sparkRate = v * 0.3
}
var sparkBright = 0
export function sliderSparkleBrightness(v) {
  //# min=0 max=1 step=0.01 default=0
  sparkBright = v
}
var sparkSat = 1
export function sliderSparkleSaturation(v) {
  //# min=0 max=1 step=0.01 default=1
  sparkSat = v
}
var tearChance = 0.1
export function sliderTearChance(v) {
  //# min=0 max=1 step=0.01 default=0.1
  tearChance = v
}
var tearHz = 3
export function sliderTearRate(v) {
  //# min=0 max=1 step=0.01 default=0.3
  tearHz = 0.2 + v * 9.8            // up to about ten re-rolls per second
}
var tearBright = 0.5
export function sliderTearBrightness(v) {
  //# min=0 max=1 step=0.01 default=0.5
  tearBright = v
}

// shortest signed horizontal distance around the cylinder
function wrapDist(d) {
  return mod(d + W / 2, W) - W / 2
}

function bounceY(k) {
  if (by[k] < 0) {
    by[k] = -by[k]
    bvy[k] = abs(bvy[k])
  }
  if (by[k] > H - S) {
    by[k] = 2 * (H - S) - by[k]
    bvy[k] = -abs(bvy[k])
  }
  by[k] = clamp(by[k], 0, H - S)
}

export function beforeRender(delta) {
  var i, j, k, pass, tmp
  var dt = min(delta, 50) / 1000    // clamp delta so physics stays stable

  // integrate: wrap horizontally, reflect off top/bottom
  for (k = 0; k < NB; k++) {
    bx[k] = mod(bx[k] + bvx[k] * dt, W)
    by[k] += bvy[k] * dt
    bounceY(k)
  }

  // rigid collisions, several passes over all six unordered pairs
  for (pass = 0; pass < 3; pass++) {
    for (i = 0; i < NB - 1; i++) {
      for (j = i + 1; j < NB; j++) {
        var dx = wrapDist(bx[j] - bx[i])   // center delta == corner delta
        var dy = by[j] - by[i]
        var ox = S - abs(dx)
        var oy = S - abs(dy)
        if (ox > 0 && oy > 0) {
          if (ox < oy) {
            // resolve along x: split the penetration, minus a tiny slop
            var sgx = dx >= 0 ? 1 : -1
            var push = max(ox * 0.45 - 0.01, 0)
            bx[i] = mod(bx[i] - sgx * push, W)
            bx[j] = mod(bx[j] + sgx * push, W)
            // approaching along the normal? elastic swap of that component
            if ((bvx[j] - bvx[i]) * sgx < 0) {
              tmp = bvx[i]; bvx[i] = bvx[j]; bvx[j] = tmp
            }
          } else {
            var sgy = dy >= 0 ? 1 : -1
            var pushy = max(oy * 0.45 - 0.01, 0)
            by[i] -= sgy * pushy
            by[j] += sgy * pushy
            bounceY(i)
            bounceY(j)
            if ((bvy[j] - bvy[i]) * sgy < 0) {
              tmp = bvy[i]; bvy[i] = bvy[j]; bvy[j] = tmp
            }
          }
        }
      }
    }
  }

  // renormalize speed: collisions redirect, liveliness never decays
  for (k = 0; k < NB; k++) {
    var sp = hypot(bvx[k], bvy[k])
    if (sp > 0.01) {
      bvx[k] *= TARGET_SPEED / sp
      bvy[k] *= TARGET_SPEED / sp
    } else {
      bvx[k] = TARGET_SPEED * 0.7
      bvy[k] = TARGET_SPEED * 0.7
    }
  }

  // slow global rainbow, ~10 s per revolution
  hueBase = time(0.15)

  // glitch ticks: hashed, so everything holds still between re-rolls
  sparkAcc += delta
  if (sparkAcc > 50) {              // sparkles refresh ~20x/s
    sparkAcc = mod(sparkAcc, 50)
    sparkTick = (sparkTick + 1) % 997
  }
  tearAcc += delta
  if (tearAcc > 1000 / tearHz) {
    tearAcc = mod(tearAcc, 1000 / tearHz)
    tearTick = (tearTick + 1) % 991
  }
  var b
  for (b = 0; b < 3; b++) {
    var seed = tearTick * 3 + b
    tearRow[b] = floor(hash2(seed, 17) * H)
    tearShift[b] = (hash2(seed, 29) - 0.5) * W * 2 / 3
    tearOn[b] = hash2(seed, 43) < tearChance
    tearHueArr[b] = hash2(seed, 61)
  }

  paintCanvas()
}

var HALF_DIAG = S * SQRT2 / 2

function paintCanvas() {
  var cx, cy, k, b
  var streakRow = floor(hash2(sparkTick, 7) * H)
  var streakCol = hash2(sparkTick, 11) * W
  for (cy = 0; cy < H; cy++) {
    for (cx = 0; cx < W; cx++) {
      var idx = cy * W + cx
      var sx = cx + 0.5             // sample at the cell center
      var sy = cy + 0.5

      // tearing shifts the SAMPLE position only, slicing whatever passes by
      var inTear = 0
      var tHue = 0
      for (b = 0; b < 3; b++) {
        if (tearOn[b] && tearRow[b] == cy) {
          sx = mod(sx + tearShift[b], W)
          inTear = 1
          tHue = tearHueArr[b]
        }
      }

      // soft-box coverage of each square (~one pixel of edge falloff)
      var bestCov = 0
      var bestK = 0
      var bestD = 0
      var totCov = 0
      for (k = 0; k < NB; k++) {
        var dx = wrapDist(sx - (bx[k] + S / 2))
        var dy = sy - (by[k] + S / 2)
        var cov = min(
          clamp(S / 2 - abs(dx) + 0.5, 0, 1),
          clamp(S / 2 - abs(dy) + 0.5, 0, 1)
        )
        if (cov > 0) {
          totCov += cov
          if (cov > bestCov) {
            bestCov = cov
            bestK = k
            bestD = hypot(dx, dy)
          }
        }
      }

      var h = 0, s = 1, v = 0
      if (bestCov > 0) {
        // radial two-tone gradient out from the square's center
        h = hueBase + bestK * 0.25 + bestD / HALF_DIAG * 0.55
        v = min(totCov + inTear * 0.2, 1)
        s = 1
      } else if (inTear) {
        h = tHue                    // pale, nearly white torn row
        s = 0.12
        v = tearBright
      } else {
        // hashed confetti sparkles + one short horizontal streak
        if (hash2(idx, sparkTick) < sparkRate) {
          h = hash2(idx + 500, sparkTick)
          s = sparkSat
          v = sparkBright
        }
        if (cy == streakRow && abs(wrapDist(cx - streakCol)) < 1.5) {
          h = hash2(sparkTick, 13)
          s = sparkSat
          v = max(v, sparkBright * 0.6)
        }
      }
      hc[idx] = h
      sc[idx] = s
      vc[idx] = v
    }
  }
}

export function render2D(index, x, y) {
  var idx = floor(y * 15.99) * 16 + floor(x * 15.99)
  hsv(hc[idx], sc[idx], vc[idx])
}

// 1D fallback exists but just outputs black (matches the original's behavior)
export function render(index) {
  rgb(0, 0, 0)
}
