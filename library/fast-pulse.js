// name: fast pulse
// Clean-room reimplementation from a prose functional description of the
// community pattern "fast pulse"; original source never consulted.

// A single narrow pulse with a white-hot core and rainbow fringes sweeps
// back and forth along the strip, sine-driven so it races through the
// middle and lingers at the turnarounds.

export function beforeRender(delta) {
  // Master clock: ~6.5 s sawtooth. Doubles as the frame hue and the
  // phase driver for the pulse position.
  t1 = time(0.1)
}

export function render(index) {
  // Sine-shaped offset spanning two full wraps of the strip per cycle,
  // plus this pixel's fractional position, wrapped to the unit interval.
  var phase = frac(wave(t1) * 2 + index / pixelCount)

  // Triangle wave peaks at one spot along the strip; a fifth power
  // sharpens the broad triangle into a narrow pulse with dim tails.
  var v = triangle(phase)
  v = v * v * v * v * v

  // The very peak of the pulse desaturates to white; shoulders stay
  // fully saturated (colored fringe).
  var s = v > 0.9 ? 0 : 1

  hsv(t1, s, v)
}
