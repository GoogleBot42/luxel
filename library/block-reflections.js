// name: block reflections
// Clean-room reimplementation from a prose functional description of the
// community pattern "block reflections"; original source never consulted.

// Mirror-symmetric bands folding out from the strip midpoint: a signed
// center-distance ramp is zoomed by slow breathing waves, folded into
// repeating blocks with a signed modulo, and the per-block hue feeds back
// into the brightness formula so blocks blink through dark seams.

export function beforeRender(delta) {
  hueBase = sin(time(0.06) * PI2)    // ~3.9 s: drifting shared base hue
  linPhase = time(0.06)              // same speed, linear: size wobble + brightness bias
  zoomTri = triangle(time(0.5))      // ~33 s: dominant zoom (bands multiply/merge)
  zoomWobble = sin(time(0.15) * PI2) // ~10 s: secondary zoom wobble
}

export function render(index) {
  // Signed distance from the strip midpoint, about -0.5 .. +0.5.
  var d = (index - pixelCount / 2) / pixelCount

  // Zoom: roughly ten-fold triangle sweep plus a sinusoidal wobble of
  // about half that reach. Sets how many bands fit on the strip.
  var zoom = zoomTri * 10 + zoomWobble * 5

  // Block size hovers around a third of the hue circle, wobbling by a
  // modest fraction. The signed modulo folds the zoomed ramp into
  // repeating sawtooth blocks; the two halves fold in opposite
  // directions, giving the mirror effect for free.
  var blockSize = 0.33 + (triangle(linPhase) - 0.5) * 0.15
  var block = (d * zoom) % blockSize

  // All blocks share the drifting base color; each block spans a slice
  // of adjacent hues (hue wraps naturally through both signs).
  var h = hueBase + block

  // Brightness couples back to the per-block hue, so it breaks up
  // per-block; the wrap of frac() makes hard travelling seams and the
  // squaring deepens the dark phase.
  var v = frac(abs(h) + abs(blockSize) + linPhase)
  v = v * v

  hsv(h, 1, v)
}
