// name: Halloween Wavy Bands
// Clean-room reimplementation from a prose functional description of the
// community pattern "Halloween Wavy Bands"; original source never consulted.

// Vertical Halloween-colored bands (oranges with a few violets) whose edges
// wobble organically (perlin) and ripple sideways (sine). Band interiors are
// bright, fading to black at the seams via a gamma-shaped triangle.

const NUM_HUES = 10   // entries in the hue table (bands cycle through it)

// Hue table: mostly red-orange/orange/amber with violet/purple accents
var hues = array(NUM_HUES)
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

var bands = 10                          // bands across the display
var rippleMul = 1                       // 1 = the natural sixth of a band width
var waveAmp = rippleMul / (6 * bands)
var wobble = 0.33                       // perlin displacement, fraction of height
var speed = 1                           // drift-rate multiplier

var clock = 0     // seconds, wrapped after ~1 hour
var slowPhase = 0 // slow negative drift: horizontal wave motion
var fastPhase = 0 // ~2x faster positive: animates the noise field

export function beforeRender(delta) {
  clock += delta / 1000
  if (clock > 3600) clock -= 3600
  slowPhase = -clock * 0.08 * speed
  fastPhase = clock * 0.16 * speed
}

// How many colored bands span the display. The hue table has ten entries, so
// band colors repeat every ten bands.
//# min=2 max=16 step=1 default=10
export function sliderBands(v) {
  bands = clamp(floor(v), 2, 16)
  waveAmp = rippleMul / (6 * bands)
}

// Overall animation rate: 0 freezes the field, 1 is the natural drift, 4 is
// four times as fast.
//# min=0 max=4 step=0.05 default=1
export function sliderSpeed(v) {
  speed = clamp(v, 0, 4)
}

// How far the perlin field bends the band edges, as a fraction of the display
// height. 0 gives dead-straight vertical bands.
//# min=0 max=1 step=0.01 default=0.33
export function sliderWobble(v) {
  wobble = clamp(v, 0, 1)
}

// Amplitude of the traveling sideways ripple: 1 = the natural sixth of a band
// width, 0 = no ripple, 3 = ripples wider than a band.
//# min=0 max=3 step=0.05 default=1
export function sliderRipple(v) {
  rippleMul = clamp(v, 0, 3)
  waveAmp = rippleMul / (6 * bands)
}

export function render2D(index, x, y) {
  // 1) organic wobble: perturb y with time-animated noise at ~2x display freq
  var wy = y - wobble * perlin(x * 2, y * 2, fastPhase, 4.6)
  // 2) traveling sideways ripple whose phase rides on the wobbled y
  var wx = x + waveAmp * sin(PI2 * (wy * 2 + slowPhase))
  // 3) quantize into bands, wrap into the hue table
  var bandPos = wx * bands
  var cell = floor(bandPos)
  var f = bandPos - cell            // fractional position within the band
  var band = mod(cell, NUM_HUES)
  // 4) triangle brightness (full at band center, black at edges) + gamma
  var v = 1 - abs(2 * f - 1)
  v = pow(v, 1.4)
  hsv(hues[band], 0.88, v)
}

// 1D: sample the 2D field along a horizontal slice a quarter of the way down
export function render(index) {
  render2D(index, index / pixelCount, 0.25)
}
