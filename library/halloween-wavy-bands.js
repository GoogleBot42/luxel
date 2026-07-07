// name: Halloween Wavy Bands
// Clean-room reimplementation from a prose functional description of the
// community pattern "Halloween Wavy Bands"; original source never consulted.

// Vertical Halloween-colored bands (oranges with a few violets) whose edges
// wobble organically (perlin) and ripple sideways (sine). Band interiors are
// bright, fading to black at the seams via a gamma-shaped triangle.

const NUM_BANDS = 10

// Hue table: mostly red-orange/orange/amber with violet/purple accents
var hues = array(NUM_BANDS)
hues[0] = 0.02
hues[1] = 0.07
hues[2] = 0.75
hues[3] = 0.05
hues[4] = 0.10
hues[5] = 0.03
hues[6] = 0.80
hues[7] = 0.08
hues[8] = 0.04
hues[9] = 0.77

const WAVE_AMP = 1 / (6 * NUM_BANDS) // ~1/6 of one band width

var clock = 0     // seconds, wrapped after ~1 hour
var slowPhase = 0 // slow negative drift: horizontal wave motion
var fastPhase = 0 // ~2x faster positive: animates the noise field

export function beforeRender(delta) {
  clock += delta / 1000
  if (clock > 3600) clock -= 3600
  slowPhase = -clock * 0.08
  fastPhase = clock * 0.16
}

export function render2D(index, x, y) {
  // 1) organic wobble: perturb y with time-animated noise at ~2x display freq
  var wy = y - 0.33 * perlin(x * 2, y * 2, fastPhase, 4.6)
  // 2) traveling sideways ripple whose phase rides on the wobbled y
  var wx = x + WAVE_AMP * sin(PI2 * (wy * 2 + slowPhase))
  // 3) quantize into bands, wrap into the hue table
  var bandPos = wx * NUM_BANDS
  var cell = floor(bandPos)
  var f = bandPos - cell            // fractional position within the band
  var band = mod(cell, NUM_BANDS)
  // 4) triangle brightness (full at band center, black at edges) + gamma
  var v = 1 - abs(2 * f - 1)
  v = pow(v, 1.4)
  hsv(hues[band], 0.88, v)
}

// 1D: sample the 2D field along a horizontal slice a quarter of the way down
export function render(index) {
  render2D(index, index / pixelCount, 0.25)
}
