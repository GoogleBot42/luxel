// name: blink fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "blink fade"; original source never consulted.

// Twinkle field: each pixel pops on at a random brightness, decays
// linearly to black over a few seconds, then instantly re-ignites at a
// fresh random level with a hue drawn from a slowly drifting palette.
// Random restart heights keep the pixels permanently desynchronized.

var levels = array(pixelCount)   // per-pixel current brightness
var hues = array(pixelCount)     // per-pixel hue, frozen at ignition

const FADE_MS = 3500             // full brightness -> black in ~3.5 s
const BAND = 0.2                 // positional gradient spans ~1/5 of wheel

export function beforeRender(delta) {
  var drift = time(0.08)         // palette circles the wheel in ~5 s
  var decay = delta / FADE_MS    // frame-rate independent linear decay
  for (var i = 0; i < pixelCount; i++) {
    levels[i] -= decay
    if (levels[i] <= 0) {
      // re-ignite: random restart height, hue = drifting base plus a
      // symmetric triangle-shaped positional offset (strip ends match)
      levels[i] = random(1)
      hues[i] = drift + triangle(i / pixelCount) * BAND
    }
  }
}

export function render(index) {
  var v = levels[index]
  // squaring the linear level reads as a natural fade (cheap gamma)
  hsv(hues[index], 1, v * v)
}
