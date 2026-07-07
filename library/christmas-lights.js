// name: Christmas Lights
// Clean-room reimplementation from a prose functional description of the
// community pattern "Christmas Lights"; original source never consulted.

// Static string-light look: blocks of a few pixels repeat a red / dim white /
// green triple. Every few seconds the colors instantly rotate one block
// position. Uses an explicit three-entry color list (h, s, v per stop)
// instead of the original's white-sentinel hue encoding, and all roles are
// defined from startup (the described uninitialized-roles bug is fixed).

var blockSize = 3        // pixels per block
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

//# min=0 max=1 step=0.09 default=0.18
export function sliderBlockSize(v) {
  blockSize = floor(1 + v * 11)    // 1..12 pixels
}

//# min=0 max=1 step=0.05 default=0.45
export function sliderInterval(v) {
  intervalMs = 1000 + v * 9000     // 1..10 s between rotations
}

export function beforeRender(delta) {
  accum += delta
  if (accum > intervalMs) {
    accum = 0
    phase = (phase + 1) % 3
  }
}

export function render(index) {
  var role = (floor(index / blockSize) + phase) % 3
  hsv(hues[role], sats[role], vals[role])
}
