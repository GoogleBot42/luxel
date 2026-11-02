// name: Utility: Palettes
// Clean-room reimplementation from a prose functional description of the
// community pattern "Utility: Palettes"; original source never consulted.
//
// A gradient-palette library for HSV patterns (FastLED-gradient-palette
// spirit), plus a self-running demo: a ~10 s rainbow chase remapped about
// once a second through each example palette in turn.
//
// Palette format: a flat array of PAL_N packed stops, positions ascending.
//   integer part  = stop position on a 0..999 scale
//   fraction part = output value (hue / saturation / brightness, 0..<1)

var PAL_N = 8            // shared stop count (arrays don't know their size)
var POS_MAX = 1000

// interpolation modes
var POSTERIZE = 0        // hard bands: earlier stop's value as-is
var HUE = 1              // shortest path around the hue wheel, wraps
var LINEAR = 2           // plain lerp: for saturation / brightness

function palette() { return array(PAL_N) }

function stops4(a, i, p0, v0, p1, v1, p2, v2, p3, v3) {
  a[i] = p0 + v0
  a[i + 1] = p1 + v1
  a[i + 2] = p2 + v2
  a[i + 3] = p3 + v3
}

// Core remap: input t (wrapped into 0..1) through a palette in a mode.
// Out-of-span inputs use the seamless wrap-around segment last->first.
function palGet(t, pal, mode) {
  t -= floor(t)
  var p = t * (POS_MAX - 1)
  var i = 0
  while (i < PAL_N && floor(pal[i]) <= p) i++
  var a, b, pa, pb
  if (i == 0 || i == PAL_N) {
    a = pal[PAL_N - 1]
    b = pal[0]
    pa = floor(a)
    pb = floor(b) + POS_MAX
    if (p < pa) p += POS_MAX
  } else {
    a = pal[i - 1]
    b = pal[i]
    pa = floor(a)
    pb = floor(b)
  }
  var va = frac(a)
  if (mode == POSTERIZE) return va
  var vb = frac(b)
  var span = pb - pa
  var f = span <= 0 ? 0 : (p - pa) / span
  if (mode == HUE) {
    var d = vb - va
    if (d > 0.5) d -= 1
    if (d < -0.5) d += 1
    var h = va + d * f
    return h - floor(h)
  }
  return va + (vb - va) * f
}

// Full color grading from one scalar: hue + saturation + brightness palettes
function grade(t, hPal, sPal, bPal) {
  hsv(palGet(t, hPal, HUE), palGet(t, sPal, LINEAR), palGet(t, bPal, LINEAR))
}

// ---------------------------------------------------------------- palettes

var rainbowH = palette()
stops4(rainbowH, 0, 0, 0, 142, 0.142, 285, 0.285, 428, 0.428)
stops4(rainbowH, 4, 571, 0.571, 714, 0.714, 857, 0.857, 999, 0.97)

var evenRainbowH = palette()   // hue stops spaced against green/cyan bloat
stops4(evenRainbowH, 0, 0, 0, 160, 0.05, 320, 0.12, 470, 0.25)
stops4(evenRainbowH, 4, 590, 0.45, 720, 0.6, 860, 0.75, 999, 0.92)

var bluesH = palette()         // blues and violets
stops4(bluesH, 0, 0, 0.55, 140, 0.6, 280, 0.64, 430, 0.68)
stops4(bluesH, 4, 570, 0.72, 710, 0.76, 850, 0.8, 999, 0.85)

var fireH = palette()          // deep red holding, rising to yellow
stops4(fireH, 0, 0, 0.001, 250, 0.005, 450, 0.02, 620, 0.045)
stops4(fireH, 4, 760, 0.07, 870, 0.1, 940, 0.13, 999, 0.16)
var fireB = palette()          // brightness falls off along the strip
stops4(fireB, 0, 0, 0.999, 300, 0.9, 500, 0.7, 700, 0.45)
stops4(fireB, 4, 850, 0.25, 940, 0.12, 980, 0.08, 999, 0.05)

var warmB = palette()          // stepped warm-white brightness ramp
stops4(warmB, 0, 0, 0.999, 150, 0.75, 300, 0.55, 450, 0.4)
stops4(warmB, 4, 600, 0.28, 750, 0.18, 880, 0.1, 999, 0.06)

var redCyanH = palette()       // complementary bands: red / cyan
stops4(redCyanH, 0, 0, 0.001, 120, 0.001, 250, 0.001, 380, 0.001)
stops4(redCyanH, 4, 500, 0.5, 620, 0.5, 750, 0.5, 880, 0.5)
var purpleGreenH = palette()   // complementary bands: purple / green
stops4(purpleGreenH, 0, 0, 0.8, 120, 0.8, 250, 0.8, 380, 0.8)
stops4(purpleGreenH, 4, 500, 0.33, 620, 0.33, 750, 0.33, 880, 0.33)
var dipLow = palette()         // dips to ~0 at the band transitions
stops4(dipLow, 0, 0, 0.05, 120, 0.7, 250, 0.999, 380, 0.7)
stops4(dipLow, 4, 500, 0.05, 620, 0.7, 750, 0.999, 880, 0.7)

var earthH = palette()         // narrow warm range, browns
stops4(earthH, 0, 0, 0.05, 150, 0.07, 300, 0.09, 450, 0.1)
stops4(earthH, 4, 600, 0.095, 750, 0.08, 880, 0.065, 999, 0.055)

var pinkOrangeH = palette()    // pink to orange, skipping most of red
stops4(pinkOrangeH, 0, 0, 0.92, 140, 0.945, 280, 0.965, 430, 0.985)
stops4(pinkOrangeH, 4, 570, 0.01, 710, 0.03, 850, 0.055, 999, 0.08)

var bvpoyH = palette()         // black-violet-pink-orange-yellow
stops4(bvpoyH, 0, 0, 0.75, 150, 0.8, 300, 0.87, 450, 0.92)
stops4(bvpoyH, 4, 600, 0.96, 750, 0.02, 880, 0.08, 999, 0.13)
var bvpoyB = palette()
stops4(bvpoyB, 0, 0, 0.02, 150, 0.12, 300, 0.28, 450, 0.45)
stops4(bvpoyB, 4, 600, 0.62, 750, 0.78, 880, 0.9, 999, 0.999)

var pinkPurpleH = palette()    // dusky pink to purple
stops4(pinkPurpleH, 0, 0, 0.9, 140, 0.88, 280, 0.86, 430, 0.84)
stops4(pinkPurpleH, 4, 570, 0.82, 710, 0.8, 850, 0.79, 999, 0.78)
var pinkPurpleS = palette()    // desaturated through the middle
stops4(pinkPurpleS, 0, 0, 0.999, 140, 0.85, 280, 0.55, 430, 0.35)
stops4(pinkPurpleS, 4, 570, 0.35, 710, 0.55, 850, 0.85, 999, 0.999)

var mintH = palette()          // minty greens
stops4(mintH, 0, 0, 0.38, 140, 0.4, 280, 0.42, 430, 0.44)
stops4(mintH, 4, 570, 0.45, 710, 0.46, 850, 0.47, 999, 0.48)

var mintPinkH = palette()      // greens with pinks
stops4(mintPinkH, 0, 0, 0.4, 160, 0.44, 300, 0.9, 450, 0.42)
stops4(mintPinkH, 4, 600, 0.93, 750, 0.45, 880, 0.9, 999, 0.41)

var dawnH = palette()          // dawn sky: warm horizon to blue
stops4(dawnH, 0, 0, 0.05, 150, 0.07, 300, 0.09, 450, 0.12)
stops4(dawnH, 4, 600, 0.55, 750, 0.58, 880, 0.6, 999, 0.62)
var dawnS = palette()
stops4(dawnS, 0, 0, 0.999, 150, 0.9, 300, 0.6, 450, 0.3)
stops4(dawnS, 4, 600, 0.45, 750, 0.6, 880, 0.7, 999, 0.8)
var dawnB = palette()
stops4(dawnB, 0, 0, 0.7, 150, 0.85, 300, 0.95, 450, 0.999)
stops4(dawnB, 4, 600, 0.9, 750, 0.75, 880, 0.6, 999, 0.5)

// ------------------------------------------------------------------- demo

// multi-step modes as named functions (t = chase input, p = raw position)
function fireMode(t, p) {
  var b = palGet(p, fireB, LINEAR)     // raw position: dims toward far end
  hsv(palGet(t, fireH, HUE), 1, b * b)
}
function warmWhiteMode(t, p) {
  var b = palGet(t, warmB, POSTERIZE)
  hsv(0.09, 0.35, b * b)
}
function redCyanMode(t, p) {
  var s = palGet(t, dipLow, LINEAR)    // fade through white at transitions
  hsv(palGet(t, redCyanH, POSTERIZE), s * s, 1)
}
function purpleGreenMode(t, p) {
  var b = palGet(t, dipLow, LINEAR)    // fade through black at transitions
  hsv(palGet(t, purpleGreenH, POSTERIZE), 1, b * b)
}
function bvpoyMode(t, p) {
  var b = palGet(t, bvpoyB, LINEAR)
  hsv(palGet(t, bvpoyH, HUE), 1, b * b)
}
function mintPinkMode(t, p) {
  // deliberately transformed inputs: doubled-and-wrapped sat, reversed bri
  var s = palGet(t * 2, pinkPurpleS, LINEAR)
  var b = palGet(1 - p, bvpoyB, LINEAR)
  hsv(palGet(t, mintPinkH, HUE), s, 0.3 + b * 0.7)
}

var NMODES = 16
var modes = array(NMODES)
modes[0] = (t, p) => hsv(palGet(t, rainbowH, POSTERIZE), 1, 1)
modes[1] = (t, p) => hsv(palGet(t, rainbowH, HUE), 1, 1)
modes[2] = (t, p) => hsv(palGet(t, evenRainbowH, HUE), 1, 1)
modes[3] = (t, p) => hsv(palGet(t, bluesH, POSTERIZE), 1, 1)
modes[4] = (t, p) => hsv(palGet(t, bluesH, HUE), 1, 1)
modes[5] = fireMode
modes[6] = warmWhiteMode
modes[7] = redCyanMode
modes[8] = purpleGreenMode
modes[9] = (t, p) => hsv(palGet(t, earthH, HUE), 0.95, 0.2)
modes[10] = (t, p) => hsv(palGet(t, pinkOrangeH, HUE), 1, 1)
modes[11] = bvpoyMode
modes[12] = (t, p) => grade(t, pinkPurpleH, pinkPurpleS, dawnB)
modes[13] = (t, p) => hsv(palGet(t, mintH, HUE), 0.55, 1)
modes[14] = mintPinkMode
modes[15] = (t, p) => grade(t, dawnH, dawnS, dawnB)

var chase = 0
var mode = 0

export function beforeRender(delta) {
  chase = time(0.15)                          // ~10 s per traversal
  mode = min(NMODES - 1, floor(time(0.25) * NMODES))  // ~1 s per palette
}

export function render(index) {
  var pos = index / pixelCount
  var t = pos - chase
  t -= floor(t)
  var f = modes[mode]
  f(t, pos)
}
