// name: spin cycle
// Clean-room reimplementation from a prose functional description of the
// community pattern "spin cycle"; original source never consulted.

// About five sharp bright bands race along the strip on a several-second
// loop. Each band is painted from a compressed half-wheel slice of the
// rainbow; the slice rotates around the wheel while the hue-striping
// density breathes between ~5 and ~10 repetitions across the strip.

var t1 = 0

export function beforeRender(delta) {
  t1 = time(0.06)   // ~4 s cycle
}

export function render(index) {
  var p = index / pixelCount

  // hue: breathing repetition count + scrolling offset, folded into a
  // half-wheel window that itself rotates once per cycle
  var reps = 5 + 5 * wave(t1)
  var h = p * reps + 2 * wave(t1)
  h = h % 0.5 + t1

  // brightness: ~5 triangular bands translating along the strip, cubed
  // for narrow punchy bars with dark gaps
  var b = triangle(frac(p * 5 + t1 * 10))
  hsv(h, 1, b * b * b)
}
