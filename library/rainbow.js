// name: rainbow
// Clean-room reimplementation from a prose functional description of the
// community pattern "rainbow"; original source never consulted.

// The canonical scrolling rainbow: one full hue cycle across the strip,
// drifting smoothly with a sawtooth offset (~5 s per revolution).

var offset = 0

export function beforeRender(delta) {
  offset = time(0.08)
}

export function render(index) {
  hsv(offset + index / pixelCount, 1, 1)
}
