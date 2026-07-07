// name: Bouncing RGB Balls - 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Bouncing RGB Balls - 2D"; original source never
// consulted.

// Three soft glowing discs — one red, one green, one blue — drift around
// the plane in slow Lissajous bounces, each axis at its own randomly
// chosen (once, at startup) period, while each disc slowly breathes its
// radius over tens of seconds. One color channel per ball means overlaps
// mix additively for free: yellow, magenta, cyan, near-white.

const SPEED = 1          // uniform scale on all six drift rates
const BALLS = 3

// per-axis motion periods in seconds, rolled once at startup (1..8 s)
var periodX = array(BALLS)
var periodY = array(BALLS)
// radius breathing periods: deliberately incommensurate, tens of seconds
var periodR = array(BALLS)
periodR[0] = 23
periodR[1] = 31
periodR[2] = 41

var k
for (k = 0; k < BALLS; k++) {
  periodX[k] = (1 + random(7)) / SPEED
  periodY[k] = (1 + random(7)) / SPEED
}

// per-frame ball state
var cx = array(BALLS)
var cy = array(BALLS)
var radius = array(BALLS)

export function beforeRender(delta) {
  var i
  for (i = 0; i < BALLS; i++) {
    // smooth 0..1..0 glide between the display edges ("bouncing")
    cx[i] = wave(time(periodX[i] / 65.536))
    cy[i] = wave(time(periodY[i] / 65.536))
    // base ~1/3 of the width, breathing up toward ~1/2
    radius[i] = 0.33 + 0.15 * wave(time(periodR[i] / 65.536))
  }
}

export function render2D(index, x, y) {
  var r = 0, g = 0, b = 0
  var d

  // red ball: linear falloff, bright center to zero at the rim
  d = hypot(x - cx[0], y - cy[0])
  if (d < radius[0]) r = 1 - d / radius[0]

  // green ball: linear falloff
  d = hypot(x - cx[1], y - cy[1])
  if (d < radius[1]) g = 1 - d / radius[1]

  // blue ball: 1 - (d/r)^3 — fuller plateau core, fast drop at the rim
  d = hypot(x - cx[2], y - cy[2])
  if (d < radius[2]) {
    d = d / radius[2]
    b = 1 - d * d * d
  }

  rgb(r, g, b)
}

// 1D fallback: the strip is the horizontal line through mid-height
export function render(index) {
  render2D(index, index / pixelCount, 0.5)
}
