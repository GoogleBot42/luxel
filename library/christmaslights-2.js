// name: ChristmasLights
// Clean-room reimplementation from a prose functional description of the
// community pattern "ChristmasLights"; original source never consulted.

// The strip is cut into equal contiguous blocks (~20 px). Blocks cycle
// through a repeating three-colour sequence -- red, dimmed white, green --
// like chunky holiday lights. Every ~2 s the assignment rotates one slot,
// so the colours march block-by-block in slow discrete jumps. No fading.
// (Original header says "red/blue" but its wrapped hue actually shows
// green; we implement what it displays: red / white / green.)

const BLOCK = 20        // pixels per block
const STEP_MS = 2000    // rotate every ~2 s

var accum = 0
var phase = 0           // which colour sits in the first slot

export function beforeRender(delta) {
  accum += delta
  if (accum >= STEP_MS) {
    accum -= STEP_MS
    phase = (phase + 1) % 3
  }
}

export function render(index) {
  var block = floor(index / BLOCK) % 3
  var slot = (block + phase) % 3
  if (slot == 0) hsv(0, 1, 1)            // red
  else if (slot == 1) hsv(0, 0, 0.45)    // dimmed white
  else hsv(0.333, 1, 1)                  // green
}
