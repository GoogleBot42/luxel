// name: bouncing balls - hsv
// Clean-room reimplementation from a prose functional description of the
// community pattern "bouncing balls - hsv"; original source never consulted.

// A handful of rainbow-hued single-pixel balls dropped under gravity: each
// falls, rebounds a little lower each bounce, and relaunches to full height
// once its bounces die out. Per-ball restitution differs slightly so the
// initially-synchronized drops desynchronize over time. Deterministic.

const NUM_BALLS = 8
const GRAVITY = -10                 // normalized strip heights per s^2
var v0 = sqrt(-2 * GRAVITY)         // launch velocity so a full rebound reaches the top
const RELAUNCH_BELOW = 0.4          // rebound velocity floor before relaunch

// Direction / symmetry mode:
// 0 = bounce from strip start, 1 = from strip end,
// 2 = from both ends toward the middle, 3 = from the middle outward
const MODE = 0

var clock = 0                       // seconds
var strikeTime = array(NUM_BALLS)   // clock at last ground strike
var reboundVel = array(NUM_BALLS)
var restitution = array(NUM_BALLS)
var hues = array(NUM_BALLS)

var hueBuf = array(pixelCount)
var briBuf = array(pixelCount)

var _i
for (_i = 0; _i < NUM_BALLS; _i++) {
  reboundVel[_i] = v0
  strikeTime[_i] = 0
  // near 0.9, decreasing slightly with ball number (inverse-square scaling)
  restitution[_i] = 0.9 - _i / (NUM_BALLS * NUM_BALLS)
  hues[_i] = _i / NUM_BALLS
}

function stamp(p, hue) {
  if (MODE == 0) {
    hueBuf[p] = hue
    briBuf[p] = 1
  } else if (MODE == 1) {
    hueBuf[pixelCount - 1 - p] = hue
    briBuf[pixelCount - 1 - p] = 1
  } else if (MODE == 2) {
    hueBuf[p] = hue
    briBuf[p] = 1
    hueBuf[pixelCount - 1 - p] = hue
    briBuf[pixelCount - 1 - p] = 1
  } else {
    var mid = floor(pixelCount / 2)
    var up = mid + p
    var down = mid - 1 - p
    if (up < pixelCount) {
      hueBuf[up] = hue
      briBuf[up] = 1
    }
    if (down >= 0) {
      hueBuf[down] = hue
      briBuf[down] = 1
    }
  }
}

export function beforeRender(delta) {
  clock += delta / 1000
  arrayReplace(briBuf, 0)

  var usable = pixelCount
  if (MODE >= 2) usable = floor(pixelCount / 2)

  var b
  for (b = 0; b < NUM_BALLS; b++) {
    var t = clock - strikeTime[b]
    var h = 0.5 * GRAVITY * t * t + reboundVel[b] * t
    if (h < 0) {
      h = 0
      reboundVel[b] = reboundVel[b] * restitution[b]
      strikeTime[b] = clock
      if (reboundVel[b] < RELAUNCH_BELOW) reboundVel[b] = v0  // relaunch
    }
    var p = clamp(floor(h * (usable - 1)), 0, usable - 1)
    stamp(p, hues[b])
  }
}

export function render(index) {
  hsv(hueBuf[index], 1, briBuf[index])
}
