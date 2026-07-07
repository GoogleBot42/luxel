// name: rainbow
// Clean-room reimplementation from a prose functional description of the
// community pattern "rainbow"; original source never consulted.

// One full hue cycle spread across the strip, scrolling smoothly.
// time(0.08) gives a ~5.2 s period for the full revolution.

export function beforeRender(delta) {
  offset = time(0.08)
}

export function render(index) {
  hsv(offset + index / pixelCount, 1, 1)
}
