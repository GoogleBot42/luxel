// name: Bouncer3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Bouncer3D"; original source never consulted.

// Small colored balls bouncing around the unit square/cube, screensaver
// style. Hot white cores fringe out to each ball's saturated hue via a
// cheap squared-Manhattan falloff inside a box test. Requires a mapped
// 2D or 3D display; there is no 1D renderer in the original.

var MAXBALLS = 20
var bx = array(MAXBALLS)
var by = array(MAXBALLS)
var bz = array(MAXBALLS)
var vx = array(MAXBALLS)
var vy = array(MAXBALLS)
var vz = array(MAXBALLS)
var bHue = array(MAXBALLS)

var count = 5
var rad2 = 0.1          // 2D ball radius
var rad3 = 0.4          // 3D radius ~4x larger for the sparser hit volume
var maxSpeed = 0.012    // units per frame (per frame, not time-scaled)

function reshuffle() {
  for (var i = 0; i < MAXBALLS; i++) {
    bx[i] = random(1)
    by[i] = random(1)
    bz[i] = random(1)
    // Faithful quirk: velocity components drawn from 0..max only, so all
    // balls initially drift toward the same corner until first bounces.
    vx[i] = random(maxSpeed)
    vy[i] = random(maxSpeed)
    vz[i] = random(maxSpeed)
    bHue[i] = random(1)
  }
}

//# min=0 max=1 step=0.05 default=0.25
export function sliderBallCount(v) {
  count = 1 + floor(v * (MAXBALLS - 1) + 0.001)
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderBallSize(v) {
  rad2 = max(0.002, v * 0.2)
  rad3 = rad2 * 4
}

// Faithful side effect preserved: touching the speed slider
// re-randomizes every ball (positions, velocities, hues).
//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  maxSpeed = 0.002 + v * 0.022
  reshuffle()
}

reshuffle()

export function beforeRender(delta) {
  for (var i = 0; i < count; i++) {
    bx[i] += vx[i]
    by[i] += vy[i]
    bz[i] += vz[i]
    // Axis checks in order; only the first wall hit per ball per frame is
    // resolved (corner hits settle over successive frames).
    if (bx[i] < 0) { bx[i] = 0; vx[i] = -vx[i] }
    else if (bx[i] > 1) { bx[i] = 1; vx[i] = -vx[i] }
    else if (by[i] < 0) { by[i] = 0; vy[i] = -vy[i] }
    else if (by[i] > 1) { by[i] = 1; vy[i] = -vy[i] }
    else if (bz[i] < 0) { bz[i] = 0; vz[i] = -vz[i] }
    else if (bz[i] > 1) { bz[i] = 1; vz[i] = -vz[i] }
  }
}

export function render2D(index, x, y) {
  for (var i = 0; i < count; i++) {
    var dx = abs(x - bx[i])
    if (dx > rad2) continue          // cheap per-axis box rejection
    var dy = abs(y - by[i])
    if (dy > rad2) continue
    var m = (dx + dy) / rad2         // Manhattan distance, normalized
    m = m * m                        // squared falloff parameter
    // White-hot center: saturation ramps up several-fold so full color is
    // reached well inside the radius. First matching ball wins.
    hsv(bHue[i], min(1, m * 4), max(0, 1 - m))
    return
  }
  rgb(0, 0, 0)
}

export function render3D(index, x, y, z) {
  for (var i = 0; i < count; i++) {
    var dx = abs(x - bx[i])
    if (dx > rad3) continue
    var dy = abs(y - by[i])
    if (dy > rad3) continue
    var dz = abs(z - bz[i])
    if (dz > rad3) continue
    var m = (dx + dy + dz) / rad3
    m = m * m
    hsv(bHue[i], min(1, m * 4), max(0, 1 - m))
    return
  }
  rgb(0, 0, 0)
}
