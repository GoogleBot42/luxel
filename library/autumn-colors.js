// name: Autumn Colors
// Clean-room reimplementation from a prose functional description of the
// community pattern "Autumn Colors"; original source never consulted.

// Every pixel independently glows in an autumn-leaf color, slowly fades
// out, then pops back at a new weighted-random hue and random brightness.
// The four hues are exported so they can be retargeted live (Christmas,
// Halloween, ...) without touching the selection logic.

export var hueRed = 0
export var hueBrown = 0.04
export var hueOrange = 0.075
export var hueYellow = 0.14

const TICK_MS = 40        // update pass throttled to ~25 Hz
const FADE_SECONDS = 5    // full-brightness fade-out time

var hues = array(pixelCount)
var levels = array(pixelCount)
var accumMs = 0

export function beforeRender(delta) {
  accumMs += delta
  if (accumMs < TICK_MS) return
  var fade = accumMs / (FADE_SECONDS * 1000)  // decay by elapsed time per tick
  accumMs = 0

  for (var i = 0; i < pixelCount; i++) {
    levels[i] -= fade
    if (levels[i] <= 0) {
      // Weighted hue choice: red ~30%, brown ~30%, orange ~37%, yellow ~3%
      var r = random(1)
      if (r < 0.3) hues[i] = hueRed
      else if (r < 0.6) hues[i] = hueBrown
      else if (r < 0.97) hues[i] = hueOrange
      else hues[i] = hueYellow
      levels[i] = random(1)   // restart at a random brightness
    }
  }
}

export function render(index) {
  var v = levels[index]
  hsv(hues[index], 1, v * v)  // squared: deepens the dim end of the fade
}
