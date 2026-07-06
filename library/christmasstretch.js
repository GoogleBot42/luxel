// name: ChristmasStretch
// Clean-room reimplementation from a prose functional description of the
// community pattern "ChristmasStretch"; original source never consulted.

// Holiday bands of red / green / dim white that slide along the strip while
// one band shrinks and another grows over each ~2 s cycle; at the cycle
// boundary the three colors rotate roles.

const CYCLE_MS = 2000

// Derive the block width from the strip so the look scales; the original
// hardcoded a couple dozen pixels per block.
var blockSize = floor(pixelCount / 9)
if (blockSize < 1) blockSize = 1

var elapsed = 0
var phase = 0

// roles[slot] = which color occupies that block position (0 red, 1 green, 2 dim white)
var roles = array(3)
roles[0] = 0
roles[1] = 1
roles[2] = 2

export function beforeRender(delta) {
  elapsed += delta
  if (elapsed >= CYCLE_MS) {
    elapsed -= CYCLE_MS
    // rotate which color occupies each block role
    var tmp = roles[0]
    roles[0] = roles[1]
    roles[1] = roles[2]
    roles[2] = tmp
  }
  phase = elapsed / CYCLE_MS
}

function paintRole(r) {
  if (r == 0) hsv(0, 1, 1)          // saturated red
  else if (r == 1) hsv(0.333, 1, 1) // saturated green
  else hsv(0, 0, 0.4)               // dim white accent
}

export function render(index) {
  // Adding phase drifts the banding one block width per cycle; subtracting
  // phase from the thresholds shrinks the first band and grows the last.
  var b = (index / blockSize + phase) % 3
  if (b < 1 - phase) paintRole(roles[0])
  else if (b < 2 - phase) paintRole(roles[1])
  else paintRole(roles[2])
}
