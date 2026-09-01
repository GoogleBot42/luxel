// name: Shimmer Crossfade 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Shimmer Crossfade 2D"; original source never consulted.
//
// Three 2D sub-scenes cycle on a fixed dwell. Transitions are stochastic
// dithering: during the crossfade window each pixel picks the outgoing or
// incoming scene at random (probability eased 0..1), giving a sparkling
// dissolve with no color-space blending.

const NUM_MODES = 3
var DWELL = 6              // seconds shown per sub-pattern
var XFADE = 0.33           // last third of each slot crossfades
var lockMode = 0           // 0 = cycle; 1..NUM_MODES = hold that sub-pattern

// parallel arrays of per-frame setups and per-pixel renderers —
// add a pattern by registering two more entries
var setups = array(NUM_MODES)
var renderers = array(NUM_MODES)

var mode = 0               // integer part of the master clock
var fadeP = 0              // eased crossfade progress 0..1

// ---------------- sub-pattern 1: rotating white line ----------------
var lineSlope = 0

function setupLine() {
  var a = time(DWELL / 65.536) * PI
  // tangent gives every orientation over a half-turn; clamp so the
  // squaring in the distance formula cannot overflow 16.16
  lineSlope = clamp(tan(a), -100, 100)
}

function drawLine(index, x, y) {
  // perpendicular distance from (x,y) to a line of slope lineSlope
  // through the matrix center
  var d = abs(lineSlope * (x - 0.5) - (y - 0.5)) / sqrt(lineSlope * lineSlope + 1)
  var v = max(0, 1 - d / 0.2)
  hsv(0, 0, v * v)
}

// ---------------- sub-pattern 2: rainbow plasma ----------------
var ph1 = 0
var ph2 = 0
var zoomP = 1

function setupPlasma() {
  ph1 = time(3 / 65.536) * PI2
  ph2 = time(6 / 65.536) * PI2
  zoomP = 1 + 3 * wave(time(14 / 65.536))
}

function drawPlasma(index, x, y) {
  var v = (sin(x * zoomP * PI2 + ph1) + cos(y * zoomP * PI2 + ph2)) / 4 + 0.5
  // cube + halve the field for brightness: dim regions crush to black so
  // the plasma reads as glowing rings on a dark field
  hsv(v, 1, v * v * v / 2)
}

// ---------------- sub-pattern 3: rotating checkerboard ----------------
function setupChecker() { }   // everything happens per pixel

function drawChecker(index, x, y) {
  var a = time(8 / 65.536) * PI2
  var ca = cos(a)
  var sa = sin(a)
  var cx = x - 0.5
  var cy = y - 0.5
  // rotate about the center, shift back slightly unevenly (off-centers
  // the checker grid — harmless quirk kept from the original look)
  var rx = cx * ca - cy * sa + 0.5
  var ry = cx * sa + cy * ca + 0.6
  var t2 = time(4 / 65.536)
  var blocks = 0.5 + 2.5 * triangle(t2)   // zoom breathes in and out
  var lit = mod(floor(rx * blocks) + floor(ry * blocks), 2)
  // gentle diagonal rainbow slice, sliding slowly
  var h = (rx + ry) / 4 + t2
  hsv(h, 1, lit)
}

// ---------------- composition ----------------
setups[0] = setupLine
setups[1] = setupPlasma
setups[2] = setupChecker
renderers[0] = drawLine
renderers[1] = drawPlasma
renderers[2] = drawChecker

export function beforeRender(delta) {
  // master clock ramps 0..NUM_MODES over the whole cycle: integer part is
  // the current mode, fraction is progress within the slot
  var master = time(DWELL * NUM_MODES / 65.536) * NUM_MODES
  mode = floor(master)
  var slot = master - mode
  var p = slot < (1 - XFADE) ? 0 : (slot - (1 - XFADE)) / XFADE
  fadeP = easeInOutQuad(p)   // dissolve starts and ends gently

  // holding one sub-pattern parks the clock: no advance, no dissolve
  if (lockMode > 0) {
    mode = lockMode - 1
    fadeP = 0
  }

  // run every sub-pattern's setup each frame (noted inefficiency; with
  // many patterns only run the two that can be visible)
  var i
  for (i = 0; i < NUM_MODES; i++) {
    var s = setups[i]
    s()
  }
}

// Seconds each sub-pattern holds the display before dissolving into the next.
// The line sub-pattern's rotation is tied to this, so it slows down too.
//# min=1 max=30 step=0.5 default=6
export function sliderDwell(v) {
  DWELL = clamp(v, 1, 30)
}

// Fraction of each slot spent dissolving: 0.05 snaps between scenes, 0.9 is
// almost always sparkling from one into the next.
//# min=0.05 max=0.9 step=0.01 default=0.33
export function sliderCrossfade(v) {
  XFADE = clamp(v, 0.05, 0.9)
}

// Hold one scene instead of cycling: 0 = cycle all three, 1 = rotating line,
// 2 = rainbow plasma, 3 = rotating checkerboard.
//# min=0 max=3 step=1 default=0
export function sliderHoldScene(v) {
  lockMode = clamp(floor(v), 0, NUM_MODES)
}

export function render2D(index, x, y) {
  // Bernoulli dither: pick the incoming scene with probability fadeP
  var m = mode
  if (random(1) < fadeP) m = m + 1
  m = mod(m, NUM_MODES)
  var r = renderers[m]
  r(index, x, y)
}
