// name: rainbow fonts
// Clean-room reimplementation from a prose functional description of the
// community pattern "rainbow fonts"; original source never consulted.

// A smoothly animated rainbow, mirror-symmetric about the strip midpoint.
// A folded distance-from-center ramp is passed through a sine-shaped wave,
// phase-shifted, and sine-folded again — so the hue bands compress and
// expand as the phase slides. Fixed modest brightness.

export function beforeRender(delta) {
  phase = time(0.1)   // ~6.5 s cycle, relaxed pace
}

export function render(index) {
  // 1 at the strip midpoint, falling linearly to 0 at both ends
  var mid = (pixelCount - 1) / 2
  var c = 1 - abs(index - mid) / mid
  // double sine-fold: ramp -> wave -> add phase -> wave -> hue
  var h = wave(wave(c) + phase)
  hsv(h, 1, 0.35)
}
