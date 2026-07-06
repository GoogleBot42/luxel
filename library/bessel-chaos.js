// name: Bessel Chaos
// Clean-room reimplementation from a prose functional description of the
// community pattern "Bessel Chaos"; original source never consulted.

// Deterministic "chaos": a cubic/quartic spatial chirp inside two sines,
// driven by several slow oscillators with incommensurate periods. Broad
// breathing blobs near a wandering focal point compress into fine
// shimmering ripples away from it. No randomness anywhere.

var phase, colorDiv, pan
var shape = 0.8

// Pan-motion shaping: low = smooth sweep, high = dwell at the ends and
// snap across the middle.
//# min=0 max=1 step=0.01 default=0.8
export function sliderTransitionSpeed(v) {
  shape = v
}

export function beforeRender(delta) {
  // Phase driver: ~11 s sine sweeping across roughly two full turns.
  phase = sin(time(0.17) * PI2) * PI2

  // Color-spread divisor: breathes over ~23 s between 3 and 9.
  colorDiv = 3 + 6 * wave(time(0.35))

  // Focal-point pan: ~15 s sine, shaped by a mirrored odd-root ease.
  // pow of a negative base with a fractional exponent misbehaves, so the
  // ease is applied to the magnitude and the sign is restored afterward.
  var s = sin(time(0.23) * PI2)
  var e = pow(20, -shape)  // shape 0 -> exponent 1 (linear), 1 -> 0.05
  pan = sign(s) * pow(abs(s), e) * 0.4
}

export function render(index) {
  // Signed coordinate centered on the wandering focal point.
  var d = (index / pixelCount - 0.5 - pan) * 3

  // Nonlinear squeeze: spatial frequency explodes away from the focus.
  var squeeze = d * d * d * 10

  var w1 = sin(squeeze * d + 2 * phase)  // quartic chirp, co-drifting
  var w2 = sin(squeeze - phase)          // cubic chirp, counter-drifting

  // Average then cube: sharpens crests; negative values clamp to black
  // in hsv(), which carves out the dark background for free.
  var v = (w1 + w2) / 2
  v = v * v * v

  // Hues breathe outward from cyan; spread width pulses with colorDiv.
  hsv(0.5 + w2 / colorDiv, 1, v)
}
