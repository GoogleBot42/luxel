// name: 2 Purple Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "2 Purple Fade"; original source never consulted.

// The whole strip glows one fully saturated purple/violet and breathes in
// unison: brightness swells sinusoidally from black to full and back,
// taking on the order of half a minute per full cycle.

var PURPLE_HUE = 0.77   // between blue and magenta, closer to violet
var brightness = 0

export function beforeRender(delta) {
  // time(0.45) -> sawtooth with ~29.5 s period; wave() shapes it into a
  // smooth 0..1 sinusoidal pulse.
  brightness = wave(time(0.45))
}

export function render(index) {
  hsv(PURPLE_HUE, 1, brightness)
}
