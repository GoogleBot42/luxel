// name: 8 Red Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "8 Red Fade"; original source never consulted.

// The whole display breathes a single uniform red: brightness follows a
// triangle-smoothed wave of a slow clock, one full fade every ~30 s.

var brightness = 0

export function beforeRender(delta) {
  brightness = triangle(time(0.45))   // ~29.5 s per up-and-down fade
}

export function render(index) {
  hsv(0, 1, brightness)               // pure red, only value animates
}
