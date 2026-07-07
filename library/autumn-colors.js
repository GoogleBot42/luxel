// name: Autumn Colors
// Clean-room reimplementation from a prose functional description of the
// community pattern "Autumn Colors"; original source never consulted.

// Four autumn-leaf hues, exported so they can be retargeted live from the
// variable watcher (e.g. Christmas or Halloween variants).
export var hueRed = 0
export var hueBrown = 0.035
export var hueOrange = 0.075
export var hueYellow = 0.13

var hues = array(pixelCount)
var levels = array(pixelCount)
var tickAccum = 0

// Weighted hue choice: red ~30%, brown ~30%, orange ~37%, yellow ~3%
function pickHue() {
  var r = random(1)
  if (r < 0.3) return hueRed
  if (r < 0.6) return hueBrown
  if (r < 0.97) return hueOrange
  return hueYellow
}

export function beforeRender(delta) {
  tickAccum += delta
  if (tickAccum < 30) return          // throttle updates to ~30 ms ticks
  var elapsed = tickAccum
  tickAccum = 0
  for (var i = 0; i < pixelCount; i++) {
    // decay by the actual elapsed time this tick; full fade ~5 s
    levels[i] -= elapsed / 5000
    if (levels[i] <= 0) {
      hues[i] = pickHue()
      levels[i] = random(1)           // pop back at a random brightness
    }
  }
}

export function render(index) {
  var b = levels[index]
  hsv(hues[index], 1, b * b)          // squared value deepens the dim end
}
