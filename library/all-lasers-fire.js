// name: All Lasers Fire
// Clean-room reimplementation from a prose functional description of the
// community pattern "All Lasers Fire"; original source never consulted.

// Volleys of laser beams/sparks radiating up from the bottom, morphing between
// order and chaos via tangent poles. Three copies of one scalar field — one
// per RGB channel, each with a tiny spatial and temporal offset — give white
// cores with rainbow chromatic-aberration fringes.

export var speed = 0.5
export var blastScale = 1

var interval = 0.02

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  speed = v
  // Inverted and cubed: the right end is dramatically faster; a tiny floor
  // keeps it from ever fully stopping.
  var inv = 1 - v
  interval = 0.003 + inv * inv * inv * 0.09
}

//# min=0 max=1 step=0.01 default=0.1
export function sliderBlastScale(v) {
  blastScale = 0.1 + v * 10
}

var ang, chaos, env0, env1, env2

export function beforeRender(delta) {
  var t = time(interval)
  ang = t * PI2
  // Chaos re-arms at each cycle start and decays linearly to zero, so every
  // cycle relaxes into pure order before firing again.
  chaos = (1 - t) * 0.3
  // Per-channel envelopes: blast scale plus a smooth oscillation of the time
  // base, each channel's phase nudged slightly later.
  env0 = blastScale + wave(t)
  env1 = blastScale + wave(t + 0.02)
  env2 = blastScale + wave(t + 0.04)
}

// One additive channel of the field.
function laser(x, y, d, env, phase) {
  // Tangent poles are the chaos/order oscillator: the scale factor sweeps
  // from one giant tile (a single beam) through orderly grids to explosive
  // chaos, and different radii hit poles at different times.
  var s = d * env * tan(d * chaos - ang + phase)
  var fx = frac(x / s)
  var fy = frac(y / s)
  // Point-light falloff toward a spot slightly past the tile center; features
  // near the origin blaze hotter.
  var v = 0.03 / hypot(fx - 0.62, fy - 0.62) / d
  // Cube for hard contrast: dim regions crushed, bright cores kept.
  return v * v * v
}

export function render2D(index, x, y) {
  // Flip vertical so beams radiate from the bottom (adjust to taste for the
  // mounting orientation).
  y = 1 - y
  var d = hypot(x, y)

  var r = laser(x, y, d, env0, 0)
  // Nudge coordinates down-left by a minuscule offset (and the distance by
  // the matching diagonal) so each channel samples an almost-identical field.
  x -= 0.004
  y -= 0.004
  d -= 0.0057
  var g = laser(x, y, d, env1, 0.06)
  x -= 0.004
  y -= 0.004
  d -= 0.0057
  var b = laser(x, y, d, env2, 0.12)

  rgb(r, g, b)
}

// 1D fallback: the strip is a horizontal line along the field's bottom edge;
// longer strips span a proportionally wider slice.
export function render(index) {
  render2D(index, index / (4 * sqrt(pixelCount)), 1)
}
