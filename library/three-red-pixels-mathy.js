// name: Three Red Pixels (mathy)
// Clean-room reimplementation from a prose functional description of the
// community pattern "Three Red Pixels (mathy)"; original source never
// consulted.

// Teaching example: frame-rate-independent motion + modular arithmetic.
// Three evenly spaced red dots crawl over a solid blue strip, ~10 px/s.
// One fractional position drives all three dots: pixel offsets are taken
// modulo a third of the strip, so a single comparison finds every dot.

var speed = 10 // pixels per second, identical at any frame rate
var dots = 3
var spacing = floor(pixelCount / dots)

export var pos = 0 // exported for external inspection/adjustment

export function beforeRender(delta) {
  pos = (pos + speed * delta / 1000) % pixelCount
}

export function render(index) {
  // Add a strip length before subtracting so the offset never goes negative.
  var offset = (index - pos + pixelCount) % spacing
  if (floor(offset) == 0) {
    hsv(0, 1, 1)     // red dot (within one pixel-width of an image of pos)
  } else {
    hsv(2 / 3, 1, 1) // blue background
  }
}
