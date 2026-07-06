// name: slow color shift
// Clean-room reimplementation from a prose functional description of the
// community pattern "slow color shift"; original source never consulted.

// Soft islands of color (~a dozen pixels wide) with dark valleys between
// them. The blobs slosh back and forth on a ~10 s cycle rather than
// scrolling, while the palette drifts around the hue wheel and a gentle
// quarter-wheel hue gradient spans the strip. Stateless: two free-running
// clocks read each frame, everything else is per-pixel math.

var phase                     // Clock A: full turn in ~10 s
var hueBase                   // Clock B: hue lap over several seconds

export function beforeRender(delta) {
  phase = time(0.15) * PI2    // ~9.8 s per revolution
  hueBase = time(0.1)         // ~6.5 s per hue lap
}

export function render(index) {
  // Spatial wavelength ~a dozen LEDs (raw index, so blob SIZE is fixed
  // in LEDs regardless of strip length), phase-modulated by a few units
  // of sin(Clock A) so the standing wave sweeps back and forth.
  var s = sin(index * 0.5 + 3 * sin(phase))

  // Map to 0..1 and sharpen with a 4th power: distinct bright blobs
  // separated by wide dark gaps.
  var v = (s + 1) / 2
  v = v * v
  v = v * v

  // Hue: drifting base, a small wobble from the same sinusoid, and a
  // quarter-wheel spread across the strip.
  var h = hueBase + s * 0.05 + index / (pixelCount * 4)
  hsv(h, 1, v)
}
