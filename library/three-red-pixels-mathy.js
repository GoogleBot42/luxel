// name: Three Red Pixels (mathy)
// Clean-room reimplementation from a prose functional description of the
// community pattern "Three Red Pixels (mathy)"; original source never
// consulted.

// Teaching example: a solid bright blue strip with three evenly spaced
// single red pixels crawling steadily along and wrapping. The dots are
// not tracked separately — one moving position is mirrored by taking
// pixel offsets modulo a third of the strip. Delta-scaled movement keeps
// the speed identical at any frame rate.

var speed = 10    // pixels per second
var numDots = 3
var spacing = floor(pixelCount / numDots)

export var pos = 0  // fractional head position, exported for inspection

export function beforeRender(delta) {
  // pixels/second * seconds elapsed: frame-rate independent motion
  pos = (pos + speed * delta / 1000) % pixelCount
}

export function render(index) {
  // offset behind the moving position; add a strip length first so the
  // value never goes negative, then fold by the dot spacing
  var offset = (pixelCount + index - pos) % spacing
  if (floor(offset) == 0) {
    hsv(0, 1, 1)      // red dot (or one of its spaced images)
  } else {
    hsv(2 / 3, 1, 1)  // blue background
  }
}
