// name: block reflections
// Clean-room reimplementation from a prose functional description of the
// community pattern "block reflections"; original source never consulted.

// Chunky banded blocks of color, mirror-symmetric about the strip center,
// like reflections in facing mirrors. A signed center-distance ramp is
// zoomed by a slow triangle wave (bands multiply and merge) and folded by
// a signed modulo into repeating block segments. All blocks share a
// drifting base hue; feeding the position-dependent hue back into the
// brightness formula makes brightness break up per block, so blocks blink
// through dark seams instead of dimming smoothly.

export function beforeRender(delta) {
  hueAngle = time(0.07) * PI2   // base-hue phase, ~4.6 s, used via sine
  linPhase = time(0.07)         // same-speed linear phase: size wobble + brightness bias
  zoomT = time(0.5)             // dominant zoom, ~33 s triangle breathing
  wobAngle = time(0.16) * PI2   // secondary zoom wobble, ~10 s sine
}

export function render(index) {
  // Signed distance from the strip midpoint, about -0.5 .. +0.5.
  var d = (index - pixelCount / 2) / pixelCount

  // Zoom: slow triangle up to roughly ten-fold, plus a sinusoidal
  // wobble of about half that reach.
  var zoom = triangle(zoomT) * 10 + sin(wobAngle) * 5

  // Time-varying block size around a third of the hue circle.
  var blockSize = 0.33 + (triangle(linPhase) - 0.5) * 0.12

  // Signed modulo folds the zoomed ramp into blocks; the two halves of
  // the strip fold in opposite directions — the mirror effect.
  var block = (d * zoom) % blockSize

  // All blocks share a drifting base hue; each spans adjacent hues.
  var h = sin(hueAngle) + block

  // Per-block brightness ramp; squaring deepens the dark seams.
  var v = frac(abs(h) + abs(blockSize) + linPhase)
  v = v * v

  hsv(h, 1, v)
}
