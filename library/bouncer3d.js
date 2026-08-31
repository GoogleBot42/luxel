// name: Bouncer3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Bouncer3D"; original source never consulted.
//
// A handful of colored balls bounce elastically around the unit square/cube.
// Ball centers are white-hot, fringing to the ball's saturated hue, over
// black. Manhattan-distance falloff inside a box test keeps it cheap.
// Requires a mapped 2D or 3D display (no 1D renderer in the original).

var MAXBALLS = 20
var bx = array(MAXBALLS)
var by = array(MAXBALLS)
var bz = array(MAXBALLS)
var vx = array(MAXBALLS)
var vy = array(MAXBALLS)
var vz = array(MAXBALLS)
var bh = array(MAXBALLS)

var numBalls = 5
var maxSpeed = 0.015    // per ~60 fps frame
var r2 = 0.08           // 2D radius
var r3 = 0.32           // 3D radius: ~4x the 2D radius (sparser hit volume)

function reshuffle() {
  for (var i = 0; i < MAXBALLS; i++) {
    bx[i] = random(1)
    by[i] = random(1)
    bz[i] = random(1)
    // Quirk preserved from the original: velocity components are drawn
    // from 0..max only, so freshly shuffled balls all drift toward the
    // same corner until their first bounces decorrelate them.
    vx[i] = random(maxSpeed)
    vy[i] = random(maxSpeed)
    vz[i] = random(maxSpeed)
    bh[i] = random(1)
  }
}

reshuffle()

// Whole balls, in balls. The directive makes the UI send real units, so the
// handler takes v as the count itself (no 0..1 rescaling).
//# min=1 max=20 step=1 default=5
export function sliderBallCount(v) {
  numBalls = clamp(floor(v), 1, MAXBALLS)
}

//# min=0 max=1 step=0.01 default=0.4
export function sliderBallSize(v) {
  r2 = 0.01 + v * 0.19   // up to ~a fifth of the display width
  r3 = r2 * 4
}

//# min=0 max=1 step=0.01 default=0.4
export function sliderSpeed(v) {
  maxSpeed = 0.003 + v * 0.03
  // Preserved side effect from the original: moving the speed slider
  // re-randomizes every ball (positions, velocities, hues).
  reshuffle()
}

export function beforeRender(delta) {
  // Scaled by elapsed time (16.667 ms = one 60 fps frame) so speed does
  // not depend on frame rate — the "better port" option in the spec.
  var step = delta / 16.667
  for (var i = 0; i < numBalls; i++) {
    bx[i] += vx[i] * step
    by[i] += vy[i] * step
    bz[i] += vz[i] * step
    // Check axes in order, stop after the first wall hit per ball per
    // frame; corner hits resolve over successive frames.
    if (bx[i] < 0) { bx[i] = 0; vx[i] = -vx[i] }
    else if (bx[i] > 1) { bx[i] = 1; vx[i] = -vx[i] }
    else if (by[i] < 0) { by[i] = 0; vy[i] = -vy[i] }
    else if (by[i] > 1) { by[i] = 1; vy[i] = -vy[i] }
    else if (bz[i] < 0) { bz[i] = 0; vz[i] = -vz[i] }
    else if (bz[i] > 1) { bz[i] = 1; vz[i] = -vz[i] }
  }
}

export function render2D(index, x, y) {
  for (var i = 0; i < numBalls; i++) {
    var dx = abs(x - bx[i])
    if (dx > r2) continue           // cheap box rejection, axis by axis
    var dy = abs(y - by[i])
    if (dy > r2) continue
    var d = (dx + dy) / r2          // Manhattan distance, normalized
    d = d * d                       // squared falloff parameter
    // White-hot core, saturation ramps up several-fold toward the edge
    hsv(bh[i], min(1, d * 4), 1 - d)
    return                          // first matching ball wins
  }
  rgb(0, 0, 0)
}

export function render3D(index, x, y, z) {
  for (var i = 0; i < numBalls; i++) {
    var dx = abs(x - bx[i])
    if (dx > r3) continue
    var dy = abs(y - by[i])
    if (dy > r3) continue
    var dz = abs(z - bz[i])
    if (dz > r3) continue
    var d = (dx + dy + dz) / r3
    d = d * d
    hsv(bh[i], min(1, d * 4), 1 - d)
    return
  }
  rgb(0, 0, 0)
}
