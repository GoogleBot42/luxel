// name: policeLights
// Clean-room reimplementation from a prose functional description of the
// community pattern "policeLights"; original source never consulted.
//
// Emergency-flasher blocks: alternating ~10-pixel blocks of red and
// blue-violet at full brightness, hard-swapping colors about five times a
// second. Both color states are initialized up front (fixing the
// original's first-swap quirk), and parity is taken on a floored quotient.

const BLOCK_SIZE = 10
const BLINK_MS = 200

var acc = 0
var hueA = 0      // red
var hueB = 0.7    // blue-violet

export function beforeRender(delta) {
  acc += delta
  if (acc > BLINK_MS) {
    acc = 0
    var t = hueA
    hueA = hueB
    hueB = t
  }
}

export function render(index) {
  var block = floor(index / BLOCK_SIZE)
  hsv(mod(block, 2) ? hueB : hueA, 1, 1)
}
