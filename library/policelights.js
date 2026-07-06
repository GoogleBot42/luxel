// name: policeLights
// Clean-room reimplementation from a prose functional description of the
// community pattern "policeLights"; original source never consulted.

// Emergency-vehicle flashers: the strip is split into fixed-size blocks,
// alternating blocks show red and blue-violet at full blast, and the two
// colors swap places a few times per second with hard cuts.

var BLOCK_SIZE = 10     // pixels per block
var BLINK_MS = 200      // swap interval, milliseconds

var hueA = 0            // red (top of the hue wheel)
var hueB = 0.7          // blue-violet, ~7/10 around the wheel
var elapsed = 0

export function beforeRender(delta) {
  elapsed += delta
  if (elapsed > BLINK_MS) {
    elapsed = 0
    var t = hueA
    hueA = hueB
    hueB = t
  }
}

export function render(index) {
  // Parity of the block this pixel sits in picks which hue it shows.
  var block = floor(index / BLOCK_SIZE)
  if (mod(block, 2) < 1) {
    hsv(hueA, 1, 1)
  } else {
    hsv(hueB, 1, 1)
  }
}
