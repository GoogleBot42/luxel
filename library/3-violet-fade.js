// name: 3 Violet Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "3 Violet Fade"; original source never consulted.

// The whole strip is one fully saturated violet that breathes in unison —
// a smooth bright-dark-bright cycle taking about half a minute. Pixel
// index is ignored, so it looks identical on any length or mapping.

const VIOLET_HUE = 0.8   // magenta-leaning purple

var brightness = 0

export function beforeRender(delta) {
  // ~29.5 s sawtooth shaped into a smooth 0..1..0 wave
  brightness = wave(time(0.45))
}

export function render(index) {
  hsv(VIOLET_HUE, 1, brightness)
}
