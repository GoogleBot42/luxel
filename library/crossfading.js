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
var secondsPerMode = 6     // how long each sub-pattern owns the strip
var fadeFraction = 0.4     // last two-fifths of each slot is the dissolve

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
var sway0, pulse0
function pre0() {
  sway0 = 2.5 * sin(time(0.07) * PI2)          // slow sinusoidal wobble
  pulse0 = 0.6 + 0.4 * wave(time(0.09))        // strip-wide brightness pulse
}
function draw0(index) {
  var p = index / pixelCount
  // reciprocal of position compresses the ripple bands toward one end
  var band = wave(0.35 / (p + 0.12) + sway0 / PI2)
  setHsv(0.6 + 0.16 * band, 1, pulse0)         // blues into violets
}

// --- sub-pattern 1: scrolling rainbow blocks -------------------------
var scroll1
function pre1() {
  scroll1 = time(0.06)                         // ~4 s per revolution
}
function draw1(index) {
  var p = index / pixelCount
  if (frac(p * 5) >= 0.5) {                    // lit latter half of each of 5 segments
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
  pos2 = triangle(time(0.06)) * (pixelCount - 1)  // ~2 s per end-to-end sweep
}
function draw2(index) {
  var v = max(0, 1 - abs(index - pos2) / 4)    // 4 px linear falloff
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
