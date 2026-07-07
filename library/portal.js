// name: portal
// Clean-room reimplementation from a prose functional description of the
// community pattern "portal"; original source never consulted.

// Slow ripples: every couple of seconds a pulse is born near the strip's
// midpoint and spreads outward as a widening half-sine hump while its
// strength decays. Summed intensity is mapped through a banded "fire
// portal" gradient (thin red/orange/amber bands over ember-red gaps, then
// through black into a violet glow where pulses overlap or peak).
// Implements the *intended* design: full pool scan, so two pulses coexist.

var POOL = 4
var SPAWN_S = 2      // seconds between births
var LIFE_S = 4       // pulse lifetime (~2x spawn interval: pulses overlap)
var W0 = 0.1         // spatial width (fraction of strip) at birth
var W1 = 1.2         // width at death: wider than the whole strip

var alive = array(POOL)
var birth = array(POOL)
var posn = array(POOL)

var intensity = array(pixelCount)

// Piecewise-linear gradient stops: [input, r, g, b, ...]. Four thin warm
// bands over ember-red gaps in the lower half, black at mid-range, then a
// long ramp to violet/indigo across the upper half.
var STOPS = 12
var grad = array(STOPS * 4)
function stop(i, p, r, g, b) {
  grad[i * 4] = p
  grad[i * 4 + 1] = r
  grad[i * 4 + 2] = g
  grad[i * 4 + 3] = b
}
stop(0, 0.00, 0, 0, 0)
stop(1, 0.08, 1, 0, 0)           // red band
stop(2, 0.10, 0.25, 0.02, 0)     // ember gap
stop(3, 0.18, 1, 0.2, 0)         // red-orange band
stop(4, 0.20, 0.25, 0.02, 0)
stop(5, 0.28, 1, 0.45, 0)        // orange band
stop(6, 0.30, 0.25, 0.02, 0)
stop(7, 0.38, 1, 0.7, 0.08)      // amber band
stop(8, 0.42, 0.25, 0.02, 0)
stop(9, 0.50, 0, 0, 0)           // back to black at mid-range
stop(10, 1.00, 0.5, 0.12, 0.85)  // long ramp to violet/indigo
stop(11, 2.00, 0.5, 0.12, 0.85)  // clamp plateau for overlap overshoot

var clock = 0
var nextSpawn = 0
var col = array(3)

function gradient(x, out) {
  if (x <= 0) {
    out[0] = 0
    out[1] = 0
    out[2] = 0
    return out
  }
  for (var i = 1; i < STOPS; i++) {
    if (x <= grad[i * 4]) {
      var lo = grad[(i - 1) * 4]
      var t = (x - lo) / (grad[i * 4] - lo)
      out[0] = mix(grad[(i - 1) * 4 + 1], grad[i * 4 + 1], t)
      out[1] = mix(grad[(i - 1) * 4 + 2], grad[i * 4 + 2], t)
      out[2] = mix(grad[(i - 1) * 4 + 3], grad[i * 4 + 3], t)
      return out
    }
  }
  out[0] = grad[(STOPS - 1) * 4 + 1]
  out[1] = grad[(STOPS - 1) * 4 + 2]
  out[2] = grad[(STOPS - 1) * 4 + 3]
  return out
}

export function beforeRender(delta) {
  clock += delta / 1000
  if (clock > 3600) {   // hourly wrap keeps the accumulator small
    clock -= 3600
    nextSpawn -= 3600
    for (var i = 0; i < POOL; i++) birth[i] -= 3600
  }

  // Spawn: approximately normal around the midpoint (mean of 3 uniforms).
  if (clock >= nextSpawn) {
    for (var i = 0; i < POOL; i++) {      // scan the WHOLE pool
      if (!alive[i]) {
        alive[i] = 1
        birth[i] = clock
        posn[i] = (random(1) + random(1) + random(1)) / 3
        break
      }
    }
    nextSpawn = clock + SPAWN_S
  }

  arrayReplace(intensity, 0)

  // Update + accumulate every live pulse.
  for (var i = 0; i < POOL; i++) {
    if (!alive[i]) continue
    var age = clock - birth[i]
    if (age > LIFE_S) {
      alive[i] = 0
      continue
    }
    var u = age / LIFE_S
    var env = 1 - u                          // linear temporal decay
    var halfW = (W0 + (W1 - W0) * u) * pixelCount / 2
    var c = posn[i] * pixelCount
    var lo = max(0, ceil(c - halfW))
    var hi = min(pixelCount - 1, floor(c + halfW))
    for (var p = lo; p <= hi; p++) {
      // half-sine hump: zero at both edges, peak at the pulse center
      var s = (p - (c - halfW)) / (2 * halfW)
      intensity[p] += env * sin(PI * s)
    }
  }
}

export function render(index) {
  gradient(clamp(intensity[index], 0, 1.5), col)
  // squared channels: deepen dark bands, sharpen bright ones
  rgb(col[0] * col[0], col[1] * col[1], col[2] * col[2])
}
