// name: US Flag
// Curated original for the Luxel library. A US flag laid out along a single
// strip: the first quarter is the blue canton carrying a row of white stars,
// the rest is the alternating red/white stripe field (red at both ends, as on
// the real flag). Nothing blinks — the animation is cloth. One slow sine
// travels from the hoist to the fly end, shading the flag like light across a
// fold and dragging it a little back and forth; both effects ramp up with the
// square of the distance from the pole, so the canton sits still and the free
// end does the moving.
//
// Colours are the flag palette pushed toward saturation: Old Glory red and
// blue are dark, low-saturation print colours that read as muddy brown and
// grey-violet on LEDs, so the hues are kept and the saturation is not.

var UNION = 0.25       // canton occupies the first quarter of the strip
var MAXSTRIPES = 13    // the real flag's stripe count, when there is room
var WAVES = 1.5        // sine crests along the flag

// flag palette, LED-legible
var redR = 1.0
var redG = 0.05
var redB = 0.10
var bluR = 0.06
var bluG = 0.08
var bluB = 1.0

// --- controls ---------------------------------------------------------
var waveSecs = 7          // seconds for one wave to cross the flag
//# min=1 max=30 step=0.5 default=7
export function sliderWaveSeconds(v) {
  waveSecs = max(v, 0.5)
}

var swayPixels = 3        // how far the fly end slides, in pixels
//# min=0 max=12 step=0.5 default=3
export function sliderSwayPixels(v) {
  swayPixels = v
}

var shading = 0.35        // fold shading depth, 0..1 (control is a percentage)
//# min=0 max=60 step=1 default=35
export function sliderShadingPercent(v) {
  shading = v / 100
}

var twinkle = 0           // star shimmer, 0..1 (control is a percentage)
//# min=0 max=100 step=1 default=0
export function sliderTwinklePercent(v) {
  twinkle = v / 100
}

// --- layout, derived once from pixelCount ------------------------------
var stripes = MAXSTRIPES
var stars = 5
var starHalf = 0.2        // half a star's width, in star-column units
var lastPx = 1

function layout() {
  var unionPx = pixelCount * UNION
  var fieldPx = pixelCount - unionPx
  // stripes need ~2 px each to survive; an odd count keeps red at both ends
  var n = floor(fieldPx / 2)
  if (n > MAXSTRIPES) n = MAXSTRIPES
  if (n < 3) n = 3
  if (mod(n, 2) == 0) n = n - 1
  stripes = n
  // roughly one star per 6 px of canton, never fewer than 2 or more than 10
  var s = round(unionPx / 6)
  if (s < 2) s = 2
  if (s > 10) s = 10
  stars = s
  // a star is one pixel wide: half a pixel either side of the column centre
  // (a hair over, so a star can never fall between two pixels and vanish)
  var colPx = pixelCount * UNION / stars
  starHalf = 0.51 / colPx
  if (starHalf > 0.5) starHalf = 0.5
  lastPx = max(pixelCount - 1, 1)
}

layout()

var tPhase = 0

export function beforeRender(delta) {
  tPhase = time(waveSecs / 65.536) * PI2
}

export function render(index) {
  var u = index / lastPx                    // 0 at the pole, 1 at the fly end
  var ph = u * WAVES * PI2 - tPhase
  // the cloth is pinned at the hoist: motion grows with u^2
  var reach = u * u
  var s = u + swayPixels * reach * sin(ph) / lastPx
  if (s < 0) s = 0
  // light across the fold — darkest in the trough, never black
  var lum = 1 - shading * (0.25 + 0.75 * u) * (0.5 - 0.5 * cos(ph))

  if (u < UNION) {
    // Canton: blue field, white stars on an even lattice. Read off the
    // UNDISPLACED coordinate — the pole end barely moves anyway, and this
    // way a star can never flicker between two pixels as the cloth shifts.
    var f = u * stars / UNION
    var d = abs(f - floor(f) - 0.5)
    var sv = d < starHalf ? 1 : 0
    if (sv > 0 && twinkle > 0) {
      // tPhase is a sawtooth, so the multiplier must be a whole number or the
      // stars would step at every wrap — the one thing this pattern must not do
      var k = floor(f)
      sv = sv * (1 - twinkle * 0.5 * (0.5 - 0.5 * cos(tPhase * 2 + k * 3.883)))
    }
    rgb(mix(bluR, 1, sv) * lum, mix(bluG, 1, sv) * lum, mix(bluB, 1, sv) * lum)
  } else {
    var stripe = floor((s - UNION) * stripes / (1 - UNION))
    if (stripe < 0) stripe = 0
    if (stripe > stripes - 1) stripe = stripes - 1
    if (mod(stripe, 2) == 0) rgb(redR * lum, redG * lum, redB * lum)
    else rgb(lum, lum, lum)
  }
}
