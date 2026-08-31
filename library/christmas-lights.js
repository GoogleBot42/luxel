// name: Christmas Lights
// Clean-room reimplementation from a prose functional description of the
// community pattern "Christmas Lights"; original source never consulted.

// Static string-light look: blocks of a few pixels repeat a red / dim white /
// green triple. Every few seconds the colors instantly rotate one block
// position. Uses an explicit three-entry color list (h, s, v per stop)
// instead of the original's white-sentinel hue encoding, and all roles are
// defined from startup (the described uninitialized-roles bug is fixed).

var blockSize = 3        // lit pixels per block
var gap = 0              // dark pixels between blocks ("wire" between bulbs)
var intervalMs = 5000    // time between rotations
var accum = 0
var phase = 0            // 0..2, which rotation of the triple is active

// Color stops: red, soft dim white, green
var hues = array(3)
var sats = array(3)
var vals = array(3)
hues[0] = 0;     sats[0] = 1; vals[0] = 1     // red
hues[1] = 0;     sats[1] = 0; vals[1] = 0.4   // dim white
hues[2] = 0.333; sats[2] = 1; vals[2] = 1     // green

// Lit pixels per bulb.
//# min=1 max=12 step=1 default=3
export function sliderBlockSize(v) {
  blockSize = max(1, floor(v))
}

// Dark pixels left between bulbs, like unlit wire on a real string.
//# min=0 max=8 step=1 default=0
export function sliderGap(v) {
  gap = max(0, floor(v))
}

// Seconds each coloring holds before the colors rotate one bulb along.
//# min=0.5 max=30 step=0.5 default=5
export function sliderIntervalSeconds(v) {
  intervalMs = max(0.1, v) * 1000
}

//# min=0 max=360 step=5 default=0
export function sliderColorAHue(v) { hues[0] = v / 360 }

//# min=0 max=360 step=5 default=120
export function sliderColorBHue(v) { hues[2] = v / 360 }

export function beforeRender(delta) {
  accum += delta
  if (accum > intervalMs) {
    accum = 0
    phase = (phase + 1) % 3
  }
}

export function render(index) {
  var span = blockSize + gap
  var block = floor(index / span)
  var local = index - block * span
  if (local >= blockSize) {
    hsv(0, 0, 0)                   // unlit wire between bulbs
  } else {
    var role = (block + phase) % 3
    hsv(hues[role], sats[role], vals[role])
  }
}
