// name: Christmas RG Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "Christmas RG Fade"; original source never consulted.

// Every pixel independently glows pure red or pure green and fades
// linearly to black; on reaching black it instantly relights (coin-flip
// color) at a random brightness. Staggered lifetimes keep the strip a
// gently shimmering red/green sparkle that never pulses in unison.

// hue targets exported for external adjustment (no UI controls)
export var hueRed = 0
export var hueGreen = 1 / 3

var FADE_SECONDS = 5  // a full-brightness pixel takes this long to die

var bri = array(pixelCount)
var hue = array(pixelCount)

export function beforeRender(delta) {
  var fall = delta / 1000 / FADE_SECONDS
  var i
  for (i = 0; i < pixelCount; i++) {
    bri[i] -= fall
    if (bri[i] <= 0) {
      // rebirth: fair coin for the color, uniform random brightness
      hue[i] = random(1) < 0.5 ? hueRed : hueGreen
      bri[i] = random(1)
    }
  }
}

export function render(index) {
  var v = bri[index]
  // squaring stretches the low end: perceptually smoother fade, and
  // pixels spend more of their life dim
  hsv(hue[index], 1, v * v)
}
