// name: Three Red Pixels (array)
// Clean-room reimplementation from a prose functional description of the
// community pattern "Three Red Pixels (array)"; original source never consulted.

var HUE_BG = 0.667   // saturated blue background
var HUE_DOT = 0      // saturated red dots
var SPEED = 10       // pixels per second

var hues = array(pixelCount)
var head = 0         // fractional head position, in pixels

export function beforeRender(delta) {
  head = mod(head + SPEED * delta / 1000, pixelCount)

  // Paint the background. (arrayReplace splats its value arguments starting
  // at index 0 — it is NOT an array fill, so it has to be a loop.)
  for (var i = 0; i < pixelCount; i++) hues[i] = HUE_BG

  var spacing = pixelCount / 3
  for (var d = 0; d < 3; d++) {
    hues[mod(floor(head + d * spacing), pixelCount)] = HUE_DOT
  }
}

export function render(index) {
  hsv(hues[index], 1, 1)
}
