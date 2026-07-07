// name: spin cycle
// Clean-room reimplementation from a prose functional description of the
// community pattern "spin cycle"; original source never consulted.

// About five sharp bright bands race along the strip on a several-second
// loop. Hues are folded into a half-wheel window whose position rotates
// steadily around the full wheel, while the hue striping density breathes
// between ~5 and ~10 repetitions.

var t1

export function beforeRender(delta) {
  t1 = time(0.065) // ~4.3 s cycle
}

export function render(index) {
  var p = index / pixelCount

  // Breathing repetition count (~5..10) plus a scrolling offset, folded into
  // a half-wheel window that itself rotates once per cycle.
  var h = p * (5 + 5 * wave(t1)) + wave(t1) * 2
  h = h % 0.5 + t1

  // ~5 triangular bands translating along the strip several times per cycle;
  // cubed for narrow punchy bars with dark gaps.
  var v = triangle(frac(p * 5 + t1 * 10))
  hsv(h, 1, v * v * v)
}
