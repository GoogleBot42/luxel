// name: 2D Bouncing Additive Primaries
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D Bouncing Additive Primaries"; original source never
// consulted.

// Soft disks — pure red, green, blue, repeating — ricochet off the panel
// edges. Overlaps simply add, so the secondaries and white emerge from mixing
// alone. Motion is scaled by the frame delta (the spec's recommended fix for
// the original's frame-rate-dependent stepping), tuned to the same lively feel.
//
// Every control carries a //# directive, so the UI sends REAL units (balls,
// panel widths, a speed multiplier) and the handlers use them as-is.

var MAXBALLS = 12
var px = array(MAXBALLS)
var py = array(MAXBALLS)
var vx = array(MAXBALLS)   // panels per second at speed = 1
var vy = array(MAXBALLS)
var chan = array(MAXBALLS) // 0 = red, 1 = green, 2 = blue

var numBalls = 3
var radius = 0.5
var speedK = 1

function scatter() {
  // stratified start: disk N somewhere in the Nth third, per axis, so the
  // first three disks never spawn on top of each other; further disks reuse
  // the same thirds with fresh offsets
  for (var i = 0; i < MAXBALLS; i++) {
    var lane = mod(i, 3)
    px[i] = (lane + random(1)) / 3
    py[i] = (lane + random(1)) / 3
    // diagonal-biased velocities: moderate baseline plus symmetric jitter
    vx[i] = 5 + (random(2) - 1) * 3.3
    vy[i] = vx[i] + (random(2) - 1) * 3.3
    chan[i] = lane
  }
}

scatter()

// Whole balls, in balls. Colours cycle red/green/blue as the count grows.
//# min=1 max=12 step=1 default=3
export function sliderBallCount(v) {
  numBalls = clamp(floor(v), 1, MAXBALLS)
}

// Disk radius as a fraction of the panel width.
//# min=0.05 max=1 step=0.01 default=0.5
export function sliderBallRadius(v) {
  radius = max(0.01, v)
}

// Speed multiplier: 1 = the reference pace, 0 freezes the disks.
//# min=0 max=3 step=0.05 default=1
export function sliderBallSpeed(v) {
  speedK = max(0, v)
}

// Re-throw every disk from a fresh stratified start.
export function triggerScatter(v) {
  scatter()
}

export function beforeRender(delta) {
  var dt = delta / 1000
  for (var i = 0; i < numBalls; i++) {
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
  for (var i = 0; i < numBalls; i++) {
    var d = dist(x, y, px[i], py[i])
    if (d < radius) {
      var s = 1 - d / radius   // quadratic falloff: glowing spot, not a disc
      s = s * s
      var c = chan[i]
      if (c == 0) r += s
      else if (c == 1) g += s
      else b += s
    }
  }
  rgb(r, g, b)   // overlaps sum; the engine clamps anything over full
}
