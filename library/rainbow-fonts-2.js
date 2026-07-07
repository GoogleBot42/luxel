// name: rainbow fonts 2
// Clean-room reimplementation from a prose functional description of the
// community pattern "rainbow fonts 2"; original source never consulted.

var huePhase = 0
var swayFrac = 0

export function beforeRender(delta) {
  huePhase = time(0.06)                       // hue ripple, ~4 s per cycle
  // slow sinusoidal sway, ~10 s per back-and-forth, amplitude ~1/10 strip
  swayFrac = (wave(time(0.15)) - 0.5) * 0.2
}

export function render(index) {
  var pos = index / pixelCount
  var center = 0.5 + swayFrac
  // symmetric ramp: 1 at the (swaying) center, falling to 0 at the ends
  var ramp = max(0, 1 - abs(pos - center) * 2)
  // fold the ramp through two sine-shaped waves; the second adds the time
  // phase and the sway offset, turning one ramp into rippling mirrored bands
  var h = wave(wave(ramp) + huePhase + swayFrac)
  hsv(h, 1, 0.2)                              // full saturation, deliberately dim
}
