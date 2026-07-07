// name: Christmas RG Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "Christmas RG Fade"; original source never consulted.

// Every pixel independently glows pure red or pure green and fades to
// black; at black it instantly relights (coin-flip color) at a random
// brightness. Staggered lifetimes keep the field shimmering, never pulsing.

// Exported so the two colors can be retargeted live (no UI controls).
export var hueRed = 0
export var hueGreen = 1 / 3

var FADE_SECONDS = 4 // full brightness takes several seconds to die

var bri = array(pixelCount)
var hue = array(pixelCount)

// Start every pixel mid-life so the strip lights instantly and unevenly.
var i
for (i = 0; i < pixelCount; i++) {
  hue[i] = random(1) < 0.5 ? hueRed : hueGreen
  bri[i] = random(1)
}

export function beforeRender(delta) {
  var step = delta / 1000 / FADE_SECONDS
  for (i = 0; i < pixelCount; i++) {
    bri[i] -= step
    if (bri[i] <= 0) {
      // Rebirth: fair coin for color, uniform random brightness.
      hue[i] = random(1) < 0.5 ? hueRed : hueGreen
      bri[i] = random(1)
    }
  }
}

export function render(index) {
  var v = bri[index]
  // Squaring stretches the low end: perceptually smoother fade, more time dim.
  hsv(hue[index], 1, v * v)
}
