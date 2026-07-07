// name: ChristmasStretch
// Clean-room reimplementation from a prose functional description of the
// community pattern "ChristmasStretch"; original source never consulted.

// Bands of red, green, and dim white creep along the strip. Over each
// ~2 s cycle the whole banding drifts one band-width toward the start
// while the first band shrinks and the last widens (the "stretch").
// On wrap the three colors rotate roles.

const CYCLE = 2000       // ms per slide/stretch cycle
const BANDS = 9          // color bands across the strip (3 of each color)

var elapsed = 0
var rotation = 0         // which color currently holds the first role
var phase = 0

export function beforeRender(delta) {
  elapsed += delta
  if (elapsed >= CYCLE) {
    elapsed -= CYCLE
    rotation = (rotation + 1) % 3
  }
  phase = elapsed / CYCLE
}

export function render(index) {
  // band coordinate reduced mod 3, drifting with the phase
  var b = mod(index / pixelCount * BANDS + phase, 3)

  // thresholds shrink with phase: first band narrows, last band grows
  var role
  if (b < 1 - phase) role = 0
  else if (b < 2 - phase) role = 1
  else role = 2

  var c = (role + rotation) % 3
  if (c == 0) hsv(0, 1, 1)         // saturated red
  else if (c == 1) hsv(1 / 3, 1, 1) // saturated green
  else hsv(0, 0, 0.4)              // dim white accent
}
