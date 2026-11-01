// name: Plucked String
// Curated original (hand-written showcase of the Luxel language/builtins).
// The strip is a guitar string. A pluck pulls a triangular kink that splits,
// races to the fixed ends, reflects and rings down into shimmering standing
// waves: the 1D wave equation on two capped arrays, stepped with native
// stencil2D/arrayAdd passes (a little viscosity damps the high harmonics
// first). Warm light for up, cool for down. On a panel the string is drawn
// as a vibrating line between two bridges. Click the preview to pluck it.

var rate = 12
//# min=2 max=60 step=1 default=12
export function sliderPlucksPerMinute(v) { rate = clamp(v, 2, 60) }

// Seconds for the ringing to fall to half.
var ringS = 1.5
//# min=0.3 max=6 step=0.1 default=1.5
export function sliderRingSeconds(v) { ringS = clamp(v, 0.3, 6) }

// Seconds for a kink to run the length of the string (higher = slacker).
var crossS = 1.2
//# min=0.2 max=4 step=0.1 default=1.2
export function sliderCrossSeconds(v) { crossS = clamp(v, 0.2, 4) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var MAXN = clamp(pixelCount, 8, 1024)
var u = array(MAXN), up = array(MAXN), lap = array(MAXN), tmp = array(MAXN)
var ev = array(4)
var n = 2, wait = 1.2
// displacement is simulated x100: at 16.16 a long string's gentle curvature
// would otherwise round to zero and freeze it in place
var K = 100

function pluck(pos, amp) {
  var p = clamp(round(pos * (n - 1)), 1, n - 2)
  for (var i = 1; i < n - 1; i++) {
    var s = i <= p ? i / p : (n - 1 - i) / (n - 1 - p)
    u[i] += amp * s * K
    up[i] += amp * s * K
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  var want = has2DMap() ? min(max(gridWidth(), 8), 128) : min(pixelCount, MAXN)
  if (want != n) {
    n = max(want, 3)
    feedback(u, 0)
    feedback(up, 0)
  }
  wait -= dt
  if (wait <= 0) {
    wait = 60 / rate * (0.6 + random(0.8))
    pluck(0.12 + random(0.76), (random(1) < 0.5 ? -1 : 1) * (0.55 + random(0.45)))
  }
  while (readEvent(ev)) pluck(ev[1], random(1) < 0.5 ? -0.9 : 0.9)

  // substeps keep the Courant number (with the viscosity) stable
  var cells = n * dt / crossS
  var steps = clamp(ceil(cells / 0.75), 1, 16)
  var c = min(cells / steps, 0.75), c2 = c * c
  var damp = pow(0.5, dt / steps / ringS)
  for (var k = 0; k < steps; k++) {
    // tmp = u + 0.1 (u - up): the Laplacian of it adds viscosity
    feedback(tmp, 0)
    arrayAdd(tmp, u)
    arraySub(tmp, up)
    arrayScale(tmp, 0.1)
    arrayAdd(tmp, u)
    feedback(lap, 0)
    stencil2D(lap, tmp, n, 1, -4, 1, 0)
    arrayScale(lap, c2)
    // next = u + damp (u - up) + c^2 lap (damping the velocity only),
    // written over up, then swapped
    arraySub(up, u)
    arrayScale(up, -damp)
    arrayAdd(up, u)
    arrayAdd(up, lap)
    up[0] = 0
    up[n - 1] = 0
    var sw = u
    u = up
    up = sw
  }
}

// warm for up, cool for down; kinks (sharp bends) burn white-hot
function tint(d, kink, floorV) {
  var m = min(abs(d) * 1.5, 1), k = min(kink, 1)
  var v = min(floorV + (0.5 - floorV) * sqrt(m) + 0.8 * k, 1)
  if (d >= 0) hsv(0.02 + 0.09 * m, 1 - 0.4 * m * m - 0.5 * k, v)
  else hsv(0.62 - 0.1 * m, 1 - 0.4 * m * m - 0.5 * k, v)
}

function bend(i) { return abs(u[max(i - 1, 0)] + u[min(i + 1, n - 1)] - 2 * u[i]) * 2.5 }

function strip() {
  var pc = pixelCount
  if (pc <= n) {
    for (var i = 0; i < pc; i++) {
      tint(u[i] / K, bend(i), 0.04)
      setPixel(i)
    }
    return
  }
  for (var k = 0; k < n; k++) {
    tint(u[k] / K, bend(k), 0.04)
    fillRange(round(k * pc / n), round((k + 1) * pc / n))
  }
}

function panel() {
  fade(0.55)
  var gw = max(gridWidth(), 8), gh = max(gridHeight(), 8)
  var lw = 0.9 / gh, amp = 0.36, x0 = 1.5 / gw, x1 = 1 - 1.5 / gw
  // the bridges
  hsv(0.1, 0.6, 0.35)
  fillRect(0, 0.42, 1 / gw, 0.58)
  fillRect(1 - 1 / gw, 0.42, 1, 0.58)
  var px = x0, py = 0.5 - u[0] / K * amp
  for (var i = 1; i < n; i++) {
    var x = x0 + (x1 - x0) * i / (n - 1), y = 0.5 - u[i] / K * amp
    var d = (u[i] + u[i - 1]) / (2 * K)
    // a wide soft halo under the bright core of the string
    tint(d, 0, 0.05)
    drawLine(px, py, x, y, lw * 3.5 * (0.4 + min(abs(d) * 2, 0.6)), 2)
    tint(d, bend(i) * 0.1, 0.14)
    drawLine(px, py, x, y, lw, 2)
    px = x
    py = y
  }
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
