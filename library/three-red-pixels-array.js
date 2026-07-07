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

  arrayReplace(hues, HUE_BG)
  var spacing = pixelCount / 3
  for (var i = 0; i < 3; i++) {
    hues[mod(floor(head + i * spacing), pixelCount)] = HUE_DOT
  }
}

export function render(index) {
  hsv(hues[index], 1, 1)
}
