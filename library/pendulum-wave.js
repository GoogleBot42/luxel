// name: Pendulum Wave
// Curated original (hand-written showcase of the Luxel language/builtins).
// The physics-classroom classic: a row of pendulums whose periods are tuned so
// each completes one more swing per cycle than its neighbour. The row drifts
// from unison into travelling snakes, apparent chaos and back to unison, and
// the clock wraps exactly at realignment. Panel: one bob per column, seen from
// above, with a short fade trail. Strip: the strip is cut into lanes, each
// lane one pendulum's bob (or its swing as colour when lanes are tiny).

var cycleS = 60
//# min=20 max=120 step=5 default=60
export function sliderCycleSeconds(v) { cycleS = clamp(v, 20, 120) }

var groups = 18
//# min=8 max=48 step=1 default=18
export function sliderPendulums(v) { groups = clamp(floor(v), 8, 48) }

// 0 spectrum, 1 ice, 2 ember
var pal = 0
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { pal = clamp(floor(v), 0, 2) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var u = 0, base = 36, keep = 0.8, drift = 0

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  u = mod(u + dt / cycleS, 1)
  base = round(cycleS * 0.4)          // slowest pendulum ~0.4 swings/s
  keep = pow(0.5, delta / 60)         // trail half-life 60 ms
  drift = mod(drift + dt / 40, 1)
}

// brush for pendulum k of n at swing d (-1..1), brightness b
function tint(k, n, d, b) {
  var f = k / n, h = drift + f * 0.85, c = 0.16, l = 0.72 + 0.1 * d
  if (pal == 1) { h = 0.52 + f * 0.3; c = 0.13; l = 0.7 + 0.15 * d }
  else if (pal == 2) { h = 0.08 + f * 0.14; c = 0.17; l = 0.7 + 0.12 * d }
  oklch(l * b, c * b, h)
}

function swing(k) { return sin(PI2 * frac((base + k) * u)) }

var lastY = array(48)

function panel() {
  fade(keep)
  var gw = gridWidth()
  if (gw < 1) gw = 32
  var n = min(groups, floor(gw / 2))
  var r = 1.05 / n, amp = 0.48 - r * 0.6, hw = 0.3 / gw
  for (var k = 0; k < n; k++) {
    var d = swing(k)
    var x = (k + 0.5) / n, y = 0.5 + amp * d
    // a faint thread from the pivot line, a motion smear, then the bob
    tint(k, n, d, 0.3)
    fillRect(x - hw, 0.5, x + hw, y)
    tint(k, n, d, 0.5)
    drawLine(x, lastY[k] == 0 ? y : lastY[k], x, y, r * 0.55, 2)
    tint(k, n, d, 1)
    splat(x, y, r, 2)
    oklch(0.97, 0.03, 0)
    splat(x - r * 0.2, y - r * 0.25, r * 0.3, 2)
    lastY[k] = y
  }
}

function strip() {
  var n = pixelCount, g = min(groups, max(floor(n / 2), 1)), L = n / g
  if (L < 5) {
    // tiny lanes: each lane glows with its pendulum's swing
    clear()
    for (var k = 0; k < g; k++) {
      var d = swing(k), b = 0.5 + 0.5 * d
      tint(k, g, d, 0.12 + 0.88 * b * b)
      fillRange(round(k * L), round((k + 1) * L))
    }
    return
  }
  fade(keep)
  for (var k = 0; k < g; k++) {
    var d = swing(k)
    var p = k * L + 0.5 + (0.5 + 0.44 * d) * (L - 2)
    tint(k, g, d, 1)
    fillRange(floor(p), floor(p) + 2)
  }
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
