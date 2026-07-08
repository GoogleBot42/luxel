// name: Ice Floes 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Ice Floes 2D"; original source never consulted.

// A top-down animated Voronoi diagram over a few slowly drifting control
// points ("floes"). Floe interiors glow icy blue-white and fade to deep blue
// at the edges; thin dark-blue seams mark the cell boundaries ("cracks").
// All floes drift upstream at slightly different rates with gentle sideways
// wander, so the boundaries continuously shear, merge and split.

var NPOINTS = 4
var px = array(NPOINTS)
var py = array(NPOINTS)
var vx = array(NPOINTS)   // per-tick horizontal drift (negative = upstream)
var vy = array(NPOINTS)   // per-tick vertical wander

var speed = 1             // set by slider (squared)
var TICK = 1 / 15         // seconds per simulation step
var acc = 0               // tick accumulator (s)

// 16x16 virtual canvas (row-major), one channel each
var W = 16
var hC = array(W * W)
var sC = array(W * W)
var vC = array(W * W)

function initPoints() {
  for (var i = 0; i < NPOINTS; i++) {
    px[i] = random(1)
    py[i] = random(1)
    vx[i] = -(0.012 + random(0.012))            // upstream, narrow band
    vy[i] = (random(1) - 0.5) * 2 * 0.006       // small, centered on zero
  }
}
initPoints()

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  speed = v * v * 4       // frozen .. ~4x default flow
}

function tick() {
  for (var i = 0; i < NPOINTS; i++) {
    px[i] = mod(px[i] + vx[i] * speed, 1)       // wrap horizontally (toroidal)
    py[i] += vy[i]
    if (py[i] < 0)  { py[i] = 0; vy[i] = -vy[i] }   // bounce off both banks
    if (py[i] > 1)  { py[i] = 1; vy[i] = -vy[i] }
  }
}

function toroidal(a, b) {
  var d = abs(a - b)
  if (d > 0.5) d = 1 - d
  return d
}

export function beforeRender(delta) {
  acc += delta / 1000
  while (acc >= TICK) { tick(); acc -= TICK }

  var tol = 0.06          // crack tolerance
  for (var row = 0; row < W; row++) {
    var gy = (row + 0.5) / W
    for (var col = 0; col < W; col++) {
      var gx = (col + 0.5) / W

      // two smallest toroidal distances
      var d1 = 99
      var d2 = 99
      for (var i = 0; i < NPOINTS; i++) {
        var dx = toroidal(gx, px[i])
        var dy = toroidal(gy, py[i])
        var d = hypot(dx, dy)
        if (d < d1) { d2 = d1; d1 = d }
        else if (d < d2) { d2 = d }
      }

      var crack = (d2 - d1) < tol
      var bri = 1 - d1
      bri = bri * bri * bri                       // steep falloff -> glassy centers

      var idx = row * W + col
      if (crack) {
        hC[idx] = 0.66                            // deep pure blue seam
        sC[idx] = 1
        vC[idx] = bri
      } else {
        hC[idx] = clamp(0.55 + d1 * 0.3, 0.55, 0.66)   // cyan-blue, bluer with distance
        sC[idx] = clamp(1.1 - bri, 0, 1)               // bright centers -> icy white
        vC[idx] = bri
      }
    }
  }
}

export function render2D(index, x, y) {
  var idx = floor(y * 15.99) * W + floor(x * 15.99)
  hsv(hC[idx], sC[idx], vC[idx])
}
