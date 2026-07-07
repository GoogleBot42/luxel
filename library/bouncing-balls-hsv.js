// name: bouncing balls - hsv
// Clean-room reimplementation from a prose functional description of the
// community pattern "bouncing balls - hsv"; original source never consulted.

// A handful of single-pixel balls bounce under gravity on a black
// background. Each ball has a fixed rainbow hue and a slightly different
// coefficient of restitution, so they start in sync and drift into chaos.
// When a ball's bounces nearly die out it relaunches at full height.
// Fully deterministic — no randomness.

const numBalls = 8

// direction mode (edit-the-source constant, as in the original):
// 0 = bounce from strip start, 1 = from strip end,
// 2 = both ends toward the middle, 3 = middle toward both ends
const mode = 0

// physics in normalized height units (0 = ground, 1 = top of strip)
const G = 8              // gravity, height-units / s^2
const V0 = 4             // sqrt(2 * G): full-energy rebound reaches height 1

var lastStrike = array(numBalls)  // clock at last ground hit
var vel = array(numBalls)         // rebound velocity at last hit
var rest = array(numBalls)        // per-ball coefficient of restitution

var hues = array(pixelCount)
var vals = array(pixelCount)

var clock = 0
var half = floor(pixelCount / 2)
var usable = (mode < 2) ? pixelCount : half

var i
for (i = 0; i < numBalls; i++) {
  vel[i] = V0
  // decrement scales inversely with the square of the ball count
  rest[i] = 0.9 - i / (numBalls * numBalls)
}

export function beforeRender(delta) {
  clock += delta / 1000
  arrayReplace(vals, 0)

  for (var i = 0; i < numBalls; i++) {
    var t = clock - lastStrike[i]
    var h = vel[i] * t - 0.5 * G * t * t
    if (h < 0) {
      h = 0
      vel[i] = vel[i] * rest[i]
      lastStrike[i] = clock
      // relaunch when the bounce train has nearly died out
      if (vel[i] < 0.4) vel[i] = V0
    }
    var px = floor(h * (usable - 1))
    hues[px] = i / numBalls
    vals[px] = 1
  }
}

export function render(index) {
  var bi = index
  if (mode == 1) {
    bi = pixelCount - 1 - index
  } else if (mode == 2) {
    if (index >= half) bi = pixelCount - 1 - index
  } else if (mode == 3) {
    if (index >= half) bi = index - half
    else bi = half - 1 - index
  }
  if (bi >= usable) bi = usable - 1
  hsv(hues[bi], 1, vals[bi])
}
