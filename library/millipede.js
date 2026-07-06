// name: millipede
// Clean-room reimplementation from a prose functional description of the
// community pattern "millipede"; original source never consulted.

// A segmented rainbow crawls along the strip while brightness ripples,
// locked to the color bands, travel through it — like millipede legs.

var tFast, tSlow

export function beforeRender(delta) {
  tFast = time(0.055)  // ~3.6 s cycle: hue wobble
  tSlow = time(0.11)   // ~7.2 s cycle: band scroll + brightness ripple
}

export function render(index) {
  var p = index / pixelCount

  // Scrolling segmented ramp: bands travel one strip length per slow cycle,
  // scaled up by 5 and wrapped at one-half to make half-spectrum segments.
  var h = ((p + tSlow) * 5) % 0.5

  // Static end-to-end gradient plus a slow sinusoidal hue wobble.
  h += p + wave(tFast) * 0.25

  // Brightness is a wave of the hue value itself (plus the slow phase),
  // squared — so the dark/bright ripples ride along with the color bands.
  var v = wave(h + tSlow)
  v = v * v

  hsv(h, 1, v)
}
