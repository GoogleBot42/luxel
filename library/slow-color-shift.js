// name: slow color shift
// Clean-room reimplementation from a prose functional description of the
// community pattern "slow color shift"; original source never consulted.

// Soft islands of color, roughly a dozen LEDs wide, separated by dark
// valleys. The blobs slosh back and forth (~10 s cycle) while the whole
// palette drifts around the hue wheel, with a gentle quarter-wheel hue
// gradient spread along the strip. Stateless: two clocks, per-pixel math.

var phaseA   // sloshing clock, full turn ~10 s
var hueBase  // hue lap, ~6.5 s

export function beforeRender(delta) {
  phaseA = time(0.15) * PI2   // ~9.8 s per full turn
  hueBase = time(0.1)         // ~6.5 s per hue lap
}

export function render(index) {
  // Standing wave against the raw pixel index (fixed blob size in LEDs,
  // spatial wavelength ~12 pixels), phase-swept back and forth by clock A.
  var s = sin(index * 0.52 + 3 * sin(phaseA))

  // Fourth-power sharpening: plain sine -> distinct blobs with wide gaps.
  var v = (s + 1) / 2
  v = v * v
  v = v * v

  // Hue: drifting base + small wobble from the same wave + a quarter of
  // the wheel spread end to end.
  var h = hueBase + 0.2 * s * 0.2 + index / (4 * pixelCount)
  hsv(h, 1, v)
}
