// name: 2 Purple Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "2 Purple Fade"; original source never consulted.

// Same idea as the yellow-fade sibling with only the hue changed: the
// entire strip glows one fully saturated violet and breathes in unison,
// about half a minute per full cycle.

const PURPLE_HUE = 0.78   // between blue and magenta, closer to violet

var brightness = 0

export function beforeRender(delta) {
  // slow sawtooth (~29.5 s) shaped into a smooth 0..1 pulse
  brightness = wave(time(0.45))
}

export function render(index) {
  hsv(PURPLE_HUE, 1, brightness)
}
