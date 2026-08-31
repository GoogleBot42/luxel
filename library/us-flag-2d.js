// name: US Flag 2D
// Curated original for the Luxel library. The real flag geometry on a matrix:
// 13 horizontal stripes (red top and bottom), a blue canton over the top-left
// 40% x 7-stripes, and a staggered lattice of white stars inside it. The grid
// dimensions are measured from the pixel map at startup, so the stripe count
// and the star lattice stay correct on any matrix — on a 32-wide panel the
// lattice lands on nine rows of alternating 7/6 stars, which is the real
// flag's arrangement.
//
// The animation is cloth, never blinking stars: a sine travels from the hoist
// to the fly end, displacing the flag vertically and shading it like light
// crossing a fold. Both grow with the square of the distance from the pole, so
// the canton is nearly still and the free end ripples. Stripe edges are hard
// (a pixel is fully red or fully white) so the stripes stay crisp even when a
// stripe is barely one pixel tall.
//
// Colours are the flag palette pushed toward saturation: Old Glory red and
// blue are dark, low-saturation print colours that read as muddy brown and
// grey-violet on LEDs, so the hues are kept and the saturation is not.

var STRIPES = 13
var CANTON_W = 0.4          // canton is 40% of the flag's width
var CANTON_STRIPES = 7      // ...and 7 of the 13 stripes tall

// flag palette, LED-legible
var redR = 1.0
var redG = 0.05
var redB = 0.10
var bluR = 0.06
var bluG = 0.08
var bluB = 1.0

// --- controls ---------------------------------------------------------
var waveSecs = 6            // seconds for one wave to cross the flag
//# min=1 max=30 step=0.5 default=6
export function sliderWaveSeconds(v) {
  waveSecs = max(v, 0.5)
}

var waveCrests = 1.25       // sine crests visible across the flag at once
//# min=0.5 max=4 step=0.25 default=1.25
export function sliderWaveCrests(v) {
  waveCrests = v
}

// Displacement is measured in STRIPE HEIGHTS, not pixels or screen height:
// that is what keeps the 13 bands legible on a 16-row panel (where a stripe
// is barely one pixel) while still giving a real ripple on a 64-row one.
var waveDepth = 0.5         // fly-end displacement, in stripe heights
//# min=0 max=100 step=5 default=50
export function sliderWaveDepthPercent(v) {
  waveDepth = v / 100
}

var shading = 0.25          // fold shading depth, 0..1
//# min=0 max=60 step=1 default=25
export function sliderShadingPercent(v) {
  shading = v / 100
}

// --- grid discovery ----------------------------------------------------
// render2D only gets normalized coordinates, so the panel's real dimensions
// are recovered from the smallest non-zero step in the pixel map.
var gw = 16
var gh = 16
var lastX = 1
var lastY = 1
var minX = 2
var minY = 2
var built = 0

var cantonCols = 6
var cantonRows = 9
var stepX = 2
var stepY = 2
var starR = 0
var halfX = 1
var stripeH = 1             // rows per stripe

function scan(i, x, y, z) {
  if (x > 0.0005 && x < minX) minX = x
  if (y > 0.0005 && y < minY) minY = y
}

function layout() {
  minX = 2
  minY = 2
  mapPixels(scan)
  gw = minX < 2 ? round(1 / minX) + 1 : 1
  gh = minY < 2 ? round(1 / minY) + 1 : 1
  if (gw < 1) gw = 1
  if (gh < 1) gh = 1
  lastX = max(gw - 1, 1)
  lastY = max(gh - 1, 1)
  stripeH = gh / STRIPES

  cantonCols = round(gw * CANTON_W)
  if (cantonCols < 1) cantonCols = 1
  // the last row whose stripe index is still inside the canton
  cantonRows = floor(CANTON_STRIPES * gh / STRIPES - 0.5) + 1
  if (cantonRows < 1) cantonRows = 1

  // aim for the flag's 6-across x 9-down star lattice, coarsening on big
  // panels and falling back to every-other-pixel on small ones
  stepX = round(cantonCols / 6)
  if (stepX < 2) stepX = 2
  stepY = round(cantonRows / 9)
  if (stepY < 2) stepY = 2
  halfX = floor(stepX / 2)
  // a star is a diamond; it only grows once the lattice has room for it
  starR = floor((min(stepX, stepY) - 1) / 2)
  built = 1
}

var tPhase = 0

export function beforeRender(delta) {
  if (!built) layout()
  tPhase = time(waveSecs / 65.536) * PI2
}

export function render2D(index, x, y) {
  var col = round(x * lastX)
  var row = round(y * lastY)
  var ph = x * waveCrests * PI2 - tPhase
  // pinned at the hoist, free at the fly end
  var reach = x * x
  var src = row + waveDepth * stripeH * reach * sin(ph)
  var stripe = floor((src + 0.5) * STRIPES / gh)
  if (stripe < 0) stripe = 0
  if (stripe > STRIPES - 1) stripe = STRIPES - 1
  var lum = 1 - shading * (0.25 + 0.75 * x) * (0.5 - 0.5 * cos(ph))

  if (stripe < CANTON_STRIPES && col < cantonCols) {
    // canton: stars sit on a staggered lattice, measured on the undisplaced
    // grid so they never shimmer or pop (the canton barely moves anyway)
    var g = round(row / stepY)
    var cy = g * stepY
    if (cy > cantonRows - 1) cy = cy - stepY
    if (cy < 0) cy = 0
    var ox = mod(g, 2) == 0 ? 0 : halfX
    var gx = round((col - ox) / stepX)
    if (gx < 0) gx = 0
    var cx = ox + gx * stepX
    if (cx > cantonCols - 1) cx = cx - stepX
    if (cx < 0) cx = ox
    if (abs(col - cx) + abs(row - cy) <= starR) rgb(lum, lum, lum)
    else rgb(bluR * lum, bluG * lum, bluB * lum)
  } else if (mod(stripe, 2) == 0) {
    rgb(redR * lum, redG * lum, redB * lum)
  } else {
    rgb(lum, lum, lum)
  }
}
