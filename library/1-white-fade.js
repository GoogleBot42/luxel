// name: 1 White Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "1 White Fade"; original source never consulted.

// The whole strip breathes in plain white: a slow time ramp shaped by
// wave() gives a smooth 0..1..0 sinusoidal fade, one breath every ~33 s.

var brightness = 0

export function beforeRender(delta) {
  brightness = wave(time(0.5))   // period 0.5 * 65.536 s ~ half a minute
}

export function render(index) {
  // zero saturation = white; hue is irrelevant
  hsv(0, 0, brightness)
}
