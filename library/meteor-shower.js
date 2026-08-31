// name: Meteor Shower
// Clean-room reimplementation from a prose functional description of the
// community pattern "Meteor Shower"; original source never consulted.

// Meteors streak along the strip with white-hot pastel heads and tails that
// dim while *gaining* saturation, fading to black. A per-pixel HSV ring
// buffer holds the trail history: one cell is written per animation step and
// render reads the buffer rotated by the write head, so the whole trail
// scrolls without ever shifting array contents. A delta accumulator gates
// steps to a fixed rate (~50/s) regardless of frame rate. Meteor hues follow
// a slow (~30 s) clock, low-pass filtered by averaging with the previous
// cell's hue so consecutive meteors are color neighbors.

var STEP_MS = 20        // one animation step every 20 ms (~50 px/s)
var DECAY = 0.88        // tail brightness multiplier per step
var SAT_GROW = 1.25     // tail saturation multiplier per step (clamps at 1)
var SPAWN_P = 0.067     // ~1 in 15 chance per step of an early new head

var HUE_INTERVAL = 0.45 // time() interval for the hue clock (~30 s)
var HUE_STEP = 0.004    // hue nudge per pixel along a trail
var REVERSE = 0         // 1 = meteors travel the other way along the strip

// ---- controls ------------------------------------------------------------
// Meteor speed in pixels per second.
//# min=5 max=200 step=5 default=50
export function sliderSpeed(v) {
  STEP_MS = 1000 / clamp(v, 5, 200)
}

// Trail length in pixels: how far behind the head the trail takes to decay
// to the 2% floor where a new meteor may be spawned.
//# min=3 max=60 step=1 default=31
export function sliderTailLength(v) {
  DECAY = pow(0.02, 1 / clamp(floor(v), 3, 60))
}

// Average gap between meteor heads, in pixels.
//# min=2 max=60 step=1 default=15
export function sliderMeteorGap(v) {
  SPAWN_P = 1 / clamp(floor(v), 2, 60)
}

// Seconds for the shower's color to walk the whole spectrum.
//# min=2 max=120 step=1 default=30
export function sliderHueCycle(v) {
  HUE_INTERVAL = clamp(v, 2, 120) / 65.536
}

// Send the meteors the other way along the strip.
//# default=0
export function toggleReverse(v) {
  REVERSE = v
}

var hBuf = array(pixelCount)
var sBuf = array(pixelCount)
var vBuf = array(pixelCount)
var head = 0
var accum = 0

export function beforeRender(delta) {
  hueClock = time(HUE_INTERVAL)      // full rainbow drift in ~30 s
  accum += delta
  while (accum >= STEP_MS) {
    accum -= STEP_MS
    step()
  }
}

function step() {
  var prev = head
  head = (head + 1) % pixelCount
  if (vBuf[prev] < 0.02 || random(1) < SPAWN_P) {
    // New meteor head: bright, pastel, hue between the last hue and the
    // slow clock so successive meteors stay in the same neighborhood.
    hBuf[head] = (mod(hBuf[prev], 1) + hueClock) / 2
    sBuf[head] = 0.5
    vBuf[head] = 1
  } else {
    // Tail continuation: dim, nudge hue, saturate toward vivid.
    hBuf[head] = hBuf[prev] - HUE_STEP
    sBuf[head] = min(1, sBuf[prev] * SAT_GROW)
    vBuf[head] = vBuf[prev] * DECAY
  }
}

export function render(index) {
  var i = index
  if (REVERSE) i = pixelCount - 1 - index
  i = (i + head) % pixelCount
  hsv(hBuf[i], sBuf[i], vBuf[i])
}
