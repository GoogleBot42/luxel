// name: 5 Teal Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "5 Teal Fade"; original source never consulted.

// The whole strip is one fixed teal color, breathing in unison: brightness
// follows a smooth sine-shaped fade, roughly half a minute per full cycle.
// No spatial variation, no state, no controls.

const TEAL_HUE = 0.5      // cyan/teal on the hue wheel

var brightness = 0

export function beforeRender(delta) {
  // slow sawtooth (~29.5 s) shaped into a smooth 0..1 pulse
  brightness = wave(time(0.45))
}

export function render(index) {
  hsv(TEAL_HUE, 1, brightness)
}
