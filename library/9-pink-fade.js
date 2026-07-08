// name: 9 Pink Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "9 Pink Fade"; original source never consulted.
// Whole strip a single hot pink hue whose brightness triangle-fades in and
// out through black on a very slow (~30 s) cycle. Pixel index is ignored.

// Hue just below the top of the wheel -> red/magenta boundary (hot pink-red).
var hue = 0.94
var brightness = 0

export function beforeRender(delta) {
  // One slow sawtooth (~30 s: time interval ~0.46 -> 0.46 * 65.536 s).
  var slow = time(0.46)
  // Shape it with a triangle wave so it fades up and back down through black.
  brightness = triangle(slow)
}

export function render(index) {
  hsv(hue, 1, brightness)
}
