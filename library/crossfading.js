// name: Crossfading
// Clean-room reimplementation from a prose functional description of the
// community pattern "Crossfading"; original source never consulted.
//
// A host framework that rotates through several independent sub-patterns,
// dissolving between consecutive ones. Sub-renderers never write pixels:
// they set the shared r/g/b globals (directly or via setHsv) and the host
// decides how to blend and emit them. Swap in your own sub-patterns by
// editing the preFns/drawFns tables below.

var numModes = 3
var secondsPerMode = 5     // how long each sub-pattern owns the strip
var fadeFraction = 0.4     // last two-fifths of each slot is the dissolve
var blocks1 = 8            // sub-pattern 1: lit blocks along the strip
var kittWidth = 3          // sub-pattern 2: pulse falloff radius, in pixels

// --- controls ---------------------------------------------------------
// Every setter converts a real unit into the constant the pattern shipped
// with, so the declared defaults reproduce the stock look exactly.

// How long each sub-pattern owns the strip before handing over.
//# min=1 max=60 step=0.5 default=5
export function sliderSecondsPerMode(v) { secondsPerMode = max(v, 0.5) }

// Share of each slot spent dissolving into the next sub-pattern (0 = hard cut).
//# min=0 max=90 step=1 default=40
export function sliderCrossfadePercent(v) { fadeFraction = clamp(v, 0, 95) / 100 }

// Sub-pattern 1: how many rainbow blocks fit along the strip.
//# min=1 max=32 step=1 default=8
export function sliderRainbowBlocks(v) { blocks1 = max(floor(v), 1) }

// Sub-pattern 2: half-width of the bouncing red pulse, in pixels.
//# min=1 max=30 step=0.5 default=3
export function sliderPulseWidthPixels(v) { kittWidth = max(v, 0.5) }

// shared "current color" written by sub-renderers
var r = 0
var g = 0
var b = 0
var ctmp = array(3)

function setHsv(h, s, v) {
  hsv2rgb(h, s, v, ctmp)
  r = ctmp[0]
  g = ctmp[1]
  b = ctmp[2]
}

// --- sub-pattern 0: blue-purple shimmer ------------------------------
var sway0, pulse0, focus0
function pre0() {
  sway0 = 2.5 * sin(time(0.07) * PI2)          // slow sinusoidal wobble
  // the ripple bands compress around a focus point that slowly wanders
  // three-quarters of the way along the strip (fraction of the strip, so
  // the braid lands in the same PLACE at any pixel count)
  focus0 = 0.80 + 0.07 * sin(time(0.05) * PI2)
  // strip-wide brightness pulse: a deep ~3.25 s breath, not a shallow slow one
  pulse0 = 0.51 + 0.49 * wave(time(0.0496))
}
function draw0(index) {
  var p = index / pixelCount
  // reciprocal of the distance to the focus: bands are wide at the near end,
  // braid into a fine zone at the focus, then open back out past it
  var d = focus0 - p
  if (abs(d) < 0.0005) d = 0.0005              // never divide by zero
  var band = wave(0.035 / d + sway0 / PI2)
  setHsv(0.5 + 0.333 * band, 1, pulse0)        // cyan through blue to magenta
}

// --- sub-pattern 1: scrolling rainbow blocks -------------------------
var scroll1
function pre1() {
  scroll1 = time(0.0458)                       // ~3 s per revolution
}
function draw1(index) {
  var p = index / pixelCount
  if (frac(p * blocks1) >= 0.5) {              // lit latter half of each segment
    setHsv(p + scroll1, 1, 1)
  } else {
    r = 0
    g = 0
    b = 0
  }
}

// --- sub-pattern 2: bouncing red pulse (low-budget KITT) -------------
var pos2
function pre2() {
  pos2 = triangle(time(0.0458)) * (pixelCount - 1) // ~1.5 s per end-to-end sweep
}
function draw2(index) {
  var v = max(0, 1 - abs(index - pos2) / kittWidth) // linear falloff each side
  r = v
  g = 0
  b = 0
}

// --- host: mode tables + crossfade -----------------------------------
var preFns = array(numModes)
var drawFns = array(numModes)
preFns[0] = pre0
preFns[1] = pre1
preFns[2] = pre2
drawFns[0] = draw0
drawFns[1] = draw1
drawFns[2] = draw2

var mode = 0
var nextMode = 1
var fade = 0

export function beforeRender(delta) {
  var t = time(numModes * secondsPerMode / 65.536) * numModes
  mode = floor(t)
  nextMode = (mode + 1) % numModes
  var slot = t - mode
  var fadeStart = 1 - fadeFraction
  fade = slot < fadeStart ? 0 : (slot - fadeStart) / fadeFraction

  var f = preFns[mode]
  f()
  if (fade > 0) {
    f = preFns[nextMode]
    f()
  }
}

export function render(index) {
  var f = drawFns[mode]
  f(index)
  if (fade > 0) {
    var r1 = r, g1 = g, b1 = b
    f = drawFns[nextMode]
    f(index)
    // plain linear mix in RGB space (HSV-space blending: too expensive)
    r = r1 + fade * (r - r1)
    g = g1 + fade * (g - g1)
    b = b1 + fade * (b - b1)
  }
  rgb(r, g, b)
}
