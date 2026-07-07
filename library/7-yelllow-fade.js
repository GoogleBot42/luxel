// name: 7 Yelllow Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "7 Yelllow Fade"; original source never consulted.

// The whole strip breathes a single warm golden yellow in unison:
// one slow sawtooth timebase, shaped by wave() into a smooth 0..1
// sinusoidal pulse. Roughly half a minute per full dim-bright-dim cycle.

const YELLOW_HUE = 0.13   // warm yellow, slightly toward golden

var brightness = 0

export function beforeRender(delta) {
  // time(0.45) -> ~29.5 s period; wave() shapes it 0..1 smoothly
  brightness = wave(time(0.45))
}

export function render(index) {
  // uniform: every pixel identical, index ignored
  hsv(YELLOW_HUE, 1, brightness)
}
