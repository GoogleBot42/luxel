// name: perlin fire wind
// Clean-room reimplementation from a prose functional description of the
// community pattern "perlin fire wind"; original source never consulted.

// Noise-driven fire on a 2D panel: a flame column concentrated at the
// horizontal center, streaming along the height axis, bent side to side by a
// slow wind. Palette-based (black -> deep red -> orange -> pale gold).

setPalette([
  0.00, 0, 0, 0,       // black
  0.20, 0.85, 0, 0,    // deep red
  0.55, 1, 0.25, 0,    // red-orange
  0.85, 1, 0.55, 0.08, // orange-gold
  1.00, 1, 0.9, 0.55   // pale warm gold
])

// Make the noise lattice tile on the z axis so the flicker loop is seamless.
var ZWRAP = 16
setPerlinWrap(256, 256, ZWRAP)

var w1, w2, zt, stream

export function beforeRender(delta) {
  w1 = time(0.05)         // ~3.3 s wind base
  w2 = time(0.085)        // ~5.6 s second wind base (kept incommensurate)
  zt = time(4) * ZWRAP    // ~4.4 min sweep of the noise field's z axis
  stream = time(1.5) * 8  // ~1.6 min steady vertical streaming
}

export function render2D(index, x, y) {
  // Recenter x on the panel middle and scale both axes up by ~2: coordinates
  // become a window into a larger noise space (x in -1..1, y in 0..2).
  x = (x - 0.5) * 2
  y = y * 2

  // Wind wobble: sinusoidal x offset driven by height plus the two time
  // bases, weighted strongest at the low-y edge so the column bends rather
  // than shifting rigidly.
  x += sin(y + w1 * PI2 + wave(w2) * 2) * 0.3 * (1 - y / 2)

  // Multi-octave turbulence (abs-value fractal sum), roughly doubled.
  var v = perlinTurbulence(x, y / 2 + stream, zt, 2, 0.5, 3) * 2

  // Triangle horizontal window: full at center, tapering to the sides.
  v = v * max(0, 1 - abs(x))

  // Linear ramp along the height axis: rooted-dark at one edge, brightest at
  // the other.
  v = v * y / 2

  // Clamp so the palette lookup never wraps from gold back to black; square
  // the brightness so dim red regions stay smoky.
  v = clamp(v, 0, 1)
  paint(v, v * v)
}
