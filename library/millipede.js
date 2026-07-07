// name: millipede
// Clean-room reimplementation from a prose functional description of the
// community pattern "millipede"; original source never consulted.

// Two free-running clocks: t1 ~3.3 s, t2 ~6.6 s (drives the band travel).
var t1, t2

export function beforeRender(delta) {
  t1 = time(0.05)
  t2 = time(0.1)
}

export function render(index) {
  var p = index / pixelCount

  // Scrolling segmented ramp: bands travel one strip length per t2 cycle;
  // scaling by 5 and wrapping at one-half makes repeating half-spectrum
  // segments.
  var seg = ((p + t2) * 5) % 0.5

  // Add a static end-to-end gradient and a slow sinusoidal wobble of the
  // faster clock that shifts every hue together.
  var h = seg + p + triangle(t1) * 0.5

  // Brightness rides on the hue value itself (plus the slower clock's
  // phase) so the ripples stay locked to the color bands; squaring deepens
  // the troughs and sharpens the crests.
  var v = wave(h + t2)
  v = v * v

  hsv(h, 1, v)
}
