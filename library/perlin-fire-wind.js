// name: perlin fire wind
// Clean-room reimplementation from a prose functional description of the
// community pattern "perlin fire wind"; original source never consulted.

// Noise-driven fire on a 2D panel, the whole flame column swaying as if a
// breeze is bending it. Turbulence-style perlin tongues stream along the
// height axis; the flame sits in a column at the horizontal center and
// tapers to the sides. The noise z axis rides a several-minute sawtooth
// across a wrapping lattice, so the flicker never visibly loops.
// (The original carried three dead alternate noise modes and a vestigial
// mode readout; only the always-selected turbulence path is implemented.)

// classic fire ramp: black -> deep red -> red/orange -> pale warm gold
setPalette([
  0.00, 0,    0,    0,
  0.20, 0.65, 0,    0,
  0.55, 1,    0.25, 0,
  0.85, 1,    0.6,  0.08,
  1.00, 1,    0.92, 0.55
])

// wrap the noise lattice so the sawtooth-driven coordinates loop seamlessly
var Z_WRAP = 4
var Y_WRAP = 8
setPerlinWrap(8, Y_WRAP, Z_WRAP)

var t1 = 0
var t2 = 0
var streamY = 0
var slowZ = 0

export function beforeRender(delta) {
  t1 = time(0.03)               // ~2 s   } the two wind-sway bases
  t2 = time(0.05)               // ~3.3 s }
  streamY = time(1.5) * Y_WRAP  // ~98 s: noise streams along the height axis
  slowZ = time(4) * Z_WRAP      // ~262 s: full seamless flicker loop
}

export function render2D(index, x, y) {
  // recenter x on the panel middle and scale both axes ~2x
  var cx = (x - 0.5) * 2
  var cy = y * 2

  // wind: sinusoidal x wobble, strongest at the low-y edge (the flame tips)
  // so the column bends rather than shifting rigidly
  var bend = 1 - y * 0.7
  var wx = cx + sin(cy * 2 + t1 * PI2 + 3 * wave(t2)) * 0.25 * bend

  // multi-octave turbulence, height compressed by half + streaming offset
  // (scaled ~2.5x: Luxel's turbulence amplitude runs lower than the ~2x the
  // original assumed, so this lands the same visual intensity)
  var v = 2.5 * perlinTurbulence(wx, cy * 0.5 + streamY, slowZ, 2, 0.55, 4)

  // triangular horizontal window: full at center, tapering to the sides
  v *= max(0, 1 - abs(cx))

  // linear ramp along the height axis: rooted-dark tips, brightest base
  v *= y

  v = min(v, 1)      // never wrap the palette from gold back to black
  paint(v, v * v)    // squared brightness keeps the dim red end smoky
}
