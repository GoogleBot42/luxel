// name: 7 Yelllow Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "7 Yelllow Fade"; original source never consulted.

// The whole strip glows one warm golden yellow and breathes in unison:
// brightness swells sinusoidally from black to full and back, taking on
// the order of half a minute per full cycle. Hue/saturation never change.

var YELLOW_HUE = 0.13   // warm yellow, slightly toward golden
var brightness = 0

export function beforeRender(delta) {
  // time(0.45) -> sawtooth with ~29.5 s period; wave() shapes it into a
  // smooth 0..1 sinusoidal pulse.
  brightness = wave(time(0.45))
}

export function render(index) {
  hsv(YELLOW_HUE, 1, brightness)
}
