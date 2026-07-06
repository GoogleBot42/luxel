// name: Halloween Wavy Bands
// Clean-room reimplementation from a prose functional description of the
// community pattern "Halloween Wavy Bands"; original source never consulted.

const NUM_BANDS = 10        // vertical color bands across the display
const WOBBLE = 0.33         // fraction of noise mixed into y
const WAVE_AMP = 1 / (6 * NUM_BANDS)  // ~1/6 of a band width
const EDGE_GAMMA = 1.4      // deepens the dark seams between bands

// Halloween hue table: warm oranges/red-oranges with violet/purple accents
var hues = array(NUM_BANDS)
hues[0] = 0.015
hues[1] = 0.05
hues[2] = 0.76
hues[3] = 0.08
hues[4] = 0.03
hues[5] = 0.79
hues[6] = 0.06
hues[7] = 0.10
hues[8] = 0.74
hues[9] = 0.02

var clock = 0
var slowPhase = 0   // slow negative drift: drives the horizontal wave
var fastPhase = 0   // ~2x faster positive: animates the noise field

export function beforeRender(delta) {
  clock += delta / 1000
  if (clock > 3600) clock -= 3600   // wrap ~hourly to preserve precision
  slowPhase = -clock * 0.02
  fastPhase = clock * 0.045
}

export function render2D(index, x, y) {
  // 1. organic wobble: perturb y with time-animated noise at ~2x spatial freq
  var yy = y - WOBBLE * perlin(x * 2, y * 2, fastPhase, 1.618)

  // 2. traveling sideways ripple, phase driven by the *wobbled* y + slow clock
  var xx = x + WAVE_AMP * sin((yy * 3 + slowPhase) * PI2)

  // 3. quantize into bands; wrap band number into the hue table
  var bp = xx * NUM_BANDS
  var band = floor(bp)
  var f = bp - band                 // fractional position within the band, 0..1
  band = mod(band, NUM_BANDS)

  // 4. triangle brightness (bright center, black edges) with a slight gamma
  var tri = 1 - abs(2 * f - 1)
  hsv(hues[band], 0.93, pow(tri, EDGE_GAMMA))
}

// 1D: sample the same field along a horizontal slice a quarter down
export function render(index) {
  render2D(index, index / pixelCount, 0.25)
}
