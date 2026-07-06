// name: rainbow fonts 2
// Clean-room reimplementation from a prose functional description of the
// community pattern "rainbow fonts 2"; original source never consulted.

// Concentric rainbow bands mirrored around the strip's middle, rippling as
// hues cycle. The mirror point sways slowly side to side by about a tenth
// of the strip. Full saturation, deliberately dim (~1/5 brightness).

const BRIGHTNESS = 0.2
const SWAY_AMPLITUDE = 0.1  // fraction of the strip

export function beforeRender(delta) {
  huePhase = time(0.08)                       // hue ripple, ~5 s per cycle
  sway = sin(time(0.16) * PI2) * SWAY_AMPLITUDE  // ~10 s per back-and-forth
}

export function render(index) {
  var p = index / pixelCount
  // Symmetric ramp: 1 at the (swaying) center, falling toward both ends
  var ramp = 1 - abs(p - (0.5 + sway))
  // Double sine-fold turns the ramp into mirrored bands that compress
  // and expand nonlinearly as the phase advances
  var h = wave(wave(ramp) + huePhase + sway)
  hsv(h, 1, BRIGHTNESS)
}
