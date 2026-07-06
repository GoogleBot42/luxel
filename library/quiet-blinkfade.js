// name: quiet blinkfade
// Clean-room reimplementation from a prose functional description of the
// community pattern "quiet blinkfade"; original source never consulted.

// Sparse purple twinkle: each pixel holds one scalar that is both its
// brightness (while positive) and its countdown-to-respawn (while negative).
// It decays linearly; on hitting the negative floor it respawns at a random
// modest brightness. Lit for up to ~1 s, dark for several seconds.

const HUE = 0.85          // purple/magenta
const CAP = 0.5           // max respawn brightness
const FLOOR = -3          // negative dwell floor (~6:1 dark-to-lit ratio)
const RATE = 0.5          // decay per second

var vals = array(pixelCount)

// Seed uniformly across the whole range so pixels are desynchronized from
// the first frame.
var _i
for (_i = 0; _i < pixelCount; _i++) {
  vals[_i] = FLOOR + random(CAP - FLOOR)
}

export function beforeRender(delta) {
  var step = RATE * delta / 1000
  var i
  for (i = 0; i < pixelCount; i++) {
    vals[i] -= step
    if (vals[i] <= FLOOR) vals[i] = random(CAP)  // respawn
  }
}

export function render(index) {
  var v = vals[index]
  // Positive check matters: squaring the negative dead-timer would relight.
  if (v > 0) hsv(HUE, 1, v * v)  // squared for a gentle ease-out
  else rgb(0, 0, 0)
}
