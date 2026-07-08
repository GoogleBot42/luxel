// name: An Intro to Pixelblaze Code
// Clean-room reimplementation from a prose functional description of the
// community pattern "An Intro to Pixelblaze Code"; original source never
// consulted. The original file is mostly tutorial prose; the only active
// code is a wiring-test: a red, then green, then blue single-pixel dot
// chasing along the strip in fixed formation, sweeping end to end every
// ~5 seconds. Stateless, purely per-pixel.

var GAP = 3   // pixel lag between consecutive dots

export function beforeRender(delta) {
  // sawtooth master clock: one full sweep every ~4.9 s
  lead = time(0.075) * pixelCount
}

export function render(index) {
  // each channel lights when the pixel sits within ~1 px of its lagged dot
  var r = abs(index - lead) < 1 ? 1 : 0
  var g = abs(index - (lead - GAP)) < 1 ? 1 : 0
  var b = abs(index - (lead - 2 * GAP)) < 1 ? 1 : 0
  rgb(r, g, b)
}
