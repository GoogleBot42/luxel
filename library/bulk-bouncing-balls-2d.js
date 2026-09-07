// name: Bulk Bouncing Balls 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// The entity loop, inverted. A per-pixel version of this asks every pixel
// "how far are you from each of the N balls?" — N hypot() calls per pixel,
// the most expensive shape in the whole library. With the whole-frame entry
// the balls draw THEMSELVES: `clear()`, then one `splat()` per ball (mode 1,
// additive, so overlaps glow) plus a `drawLine()` capsule from the previous
// position for a motion-blur tail. Cost is per BALL, not per pixel.
//
// Coordinates are the normalized 0..1 mapped coordinates `render2D` would
// see, so this works on any map; a pattern that only names coordinate ops
// like these gets the same default square grid a render2D pattern does.

const MAX = 8

var balls = 5
var speed = 1
var size = 0.14

var bx = array(MAX)
var by = array(MAX)
var vx = array(MAX)
var vy = array(MAX)
var ox = array(MAX)      // previous position — the tail's other end
var oy = array(MAX)
var hue = array(MAX)

var i
for (i = 0; i < MAX; i++) {
  bx[i] = 0.15 + 0.7 * hash(i + 0.5)
  by[i] = 0.15 + 0.7 * hash(i + 9.5)
  vx[i] = 0.25 + 0.35 * hash(i + 21.5)
  vy[i] = 0.25 + 0.35 * hash(i + 33.5)
  if (hash(i + 45.5) < 0.5) vx[i] = -vx[i]
  if (hash(i + 57.5) < 0.5) vy[i] = -vy[i]
  ox[i] = bx[i]
  oy[i] = by[i]
  hue[i] = i / MAX
}

export function beforeRender(delta) {
  var dt = delta * 0.001 * speed
  var b
  for (b = 0; b < balls; b++) {
    ox[b] = bx[b]
    oy[b] = by[b]
    bx[b] = bx[b] + vx[b] * dt
    by[b] = by[b] + vy[b] * dt
    // reflect off the unit square, keeping the overshoot
    if (bx[b] < 0) { bx[b] = -bx[b]; vx[b] = -vx[b] }
    if (bx[b] > 1) { bx[b] = 2 - bx[b]; vx[b] = -vx[b] }
    if (by[b] < 0) { by[b] = -by[b]; vy[b] = -vy[b] }
    if (by[b] > 1) { by[b] = 2 - by[b]; vy[b] = -vy[b] }
  }
}

export function renderFrame() {
  clear()                        // the frame persists, so start it fresh
  var b
  for (b = 0; b < balls; b++) {
    hsv(hue[b], 0.9, 0.35)
    drawLine(ox[b], oy[b], bx[b], by[b], size * 0.6, 1)
    hsv(hue[b], 0.9, 1)
    splat(bx[b], by[b], size, 1)          // mode 1 = additive
  }
}

//# min=1 max=8 step=1 default=5
export function inputNumberBalls(v) { balls = clamp(floor(v), 1, MAX) }

//# min=0.1 max=3 step=0.1 default=1
export function sliderSpeed(v) { speed = v }

//# min=0.04 max=0.3 step=0.01 default=0.14
export function sliderSize(v) { size = v }
