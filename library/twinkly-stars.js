// name: twinkly stars
// Clean-room reimplementation from a prose functional description of the
// community pattern "twinkly stars"; original source never consulted.

// Steady full-brightness blue strip; random pixels snap to white and ease
// back to blue over about a second. Time-based (delta-driven) rather than
// frame-count based, so twinkle speed is frame-rate independent.

var BLUE_HUE = 0.667      // pure blue
var RECOVERY = 1          // seconds to fade from white back to blue
var TWINKLES_PER_SEC = 0.6  // per-pixel twinkle rate (~sparse, scattered)

// Seconds since each pixel last twinkled; start "long ago" (fully blue).
var since = array(pixelCount)
var i
for (i = 0; i < pixelCount; i++) since[i] = RECOVERY + 1

var dt = 0
var chance = 0

export function beforeRender(delta) {
  dt = delta / 1000
  // Probability a given pixel starts a twinkle this frame
  // (~1-in-100 per pixel per frame at 60 fps).
  chance = TWINKLES_PER_SEC * dt
}

export function render(index) {
  var t = since[index]
  if (t < RECOVERY) {
    // Mid-recovery: saturation climbs linearly from 0 (white) to 1 (blue).
    hsv(BLUE_HUE, t / RECOVERY, 1)
    since[index] = t + dt
  } else {
    if (random(1) < chance) {
      // Twinkle: snap to pure white and restart the recovery ramp.
      since[index] = 0
      hsv(BLUE_HUE, 0, 1)
    } else {
      hsv(BLUE_HUE, 1, 1)
    }
  }
}
