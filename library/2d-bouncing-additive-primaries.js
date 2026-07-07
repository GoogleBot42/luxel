// name: 2D Bouncing Additive Primaries
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D Bouncing Additive Primaries"; original source never
// consulted.

// Three soft disks — pure red, green, blue — ricochet off the panel edges.
// Overlaps simply add, so the secondaries and white emerge from mixing alone.
// Motion is scaled by the frame delta (the spec's recommended fix for the
// original's frame-rate-dependent stepping), tuned to the same lively feel.

var NUM = 3
var px = array(NUM)
var py = array(NUM)
var vx = array(NUM)   // panels per second at speed = 1
var vy = array(NUM)

var radius = 0.5
var speedK = 1

// stratified start: disk N somewhere in the Nth third, per axis, so the
// disks never spawn on top of each other
for (i = 0; i < NUM; i++) {
  px[i] = (i + random(1)) / 3
  py[i] = (i + random(1)) / 3
  // diagonal-biased velocities: moderate baseline plus symmetric jitter
  vx[i] = 5 + (random(2) - 1) * 3.3
  vy[i] = vx[i] + (random(2) - 1) * 3.3
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderBallRadius(v) {
  radius = 0.02 + 0.98 * v
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderBallSpeed(v) {
  speedK = 2 * v   // zero freezes; default 0.5 -> 1x baseline
}

export function beforeRender(delta) {
  var dt = delta / 1000
  for (var i = 0; i < NUM; i++) {
    px[i] += vx[i] * speedK * dt
    py[i] += vy[i] * speedK * dt
    // hard reflective bounces off all four walls
    if (px[i] > 1) { px[i] = 1; vx[i] = -vx[i] }
    if (px[i] < 0) { px[i] = 0; vx[i] = -vx[i] }
    if (py[i] > 1) { py[i] = 1; vy[i] = -vy[i] }
    if (py[i] < 0) { py[i] = 0; vy[i] = -vy[i] }
  }
}

export function render2D(index, x, y) {
  var r = 0
  var g = 0
  var b = 0
  for (var i = 0; i < NUM; i++) {
    var d = dist(x, y, px[i], py[i])
    if (d < radius) {
      var s = 1 - d / radius   // quadratic falloff: glowing spot, not a disc
      s = s * s
      if (i == 0) r += s
      else if (i == 1) g += s
      else b += s
    }
  }
  rgb(r, g, b)   // overlaps sum; the engine clamps anything over full
}
