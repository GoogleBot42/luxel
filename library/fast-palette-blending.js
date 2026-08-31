// name: Fast Palette Blending
// Clean-room reimplementation from a prose functional description of the
// community pattern "Fast Palette Blending"; original source never consulted.

// A palette manager demo: hold each of three gradient palettes for a few
// seconds, then cross-fade to the next over a couple of seconds. Blending
// cost is paid once per frame into a small fixed-size "active" palette —
// and only during transitions — while per-pixel work is a single paint().

var RES = 16         // rows in the active palette

// --- controls (defaults reproduce the original constants) ---------------
var HOLD_S = 5       // seconds to hold each palette
var TRANS_S = 2      // seconds to cross-fade
var washSecs = 6.55  // seconds for one round trip of the gradient wash
var autoCycle = 1    // 1 = walk the palettes, 0 = hold the picked one
var pickedPal = 0    // 0 = blue/magenta, 1 = landscape, 2 = heatmap

//# min=0.5 max=30 step=0.5 default=5
export function sliderHoldSeconds(v) { HOLD_S = max(v, 0.1) }

//# min=0.2 max=10 step=0.2 default=2
export function sliderCrossfadeSeconds(v) { TRANS_S = max(v, 0.1) }

//# min=1 max=60 step=0.05 default=6.55
export function sliderWashSeconds(v) { washSecs = max(v, 0.5) }

//# default=1
export function toggleAutoCycle(v) { autoCycle = v > 0.5 }

//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { pickedPal = clamp(floor(v), 0, 2) }

// Gradient palettes as flat (pos, r, g, b) rows, positions ascending 0..1.

// 1: black -> deep blue -> vivid blue -> blue-violet -> magenta -> pink -> white
var pal1 = [
  0.00, 0.00, 0.00, 0.00,
  0.18, 0.00, 0.00, 0.35,
  0.40, 0.05, 0.15, 1.00,
  0.58, 0.35, 0.10, 0.90,
  0.75, 0.90, 0.05, 0.75,
  0.90, 1.00, 0.45, 0.80,
  1.00, 1.00, 1.00, 1.00]

// 2: landscape — dark greens/earth, a sunlit orange-gold band, back to dark
var pal2 = [
  0.00, 0.00, 0.03, 0.00,
  0.15, 0.15, 0.12, 0.02,
  0.38, 0.05, 0.35, 0.04,
  0.62, 1.00, 0.60, 0.05,
  0.78, 0.12, 0.45, 0.10,
  1.00, 0.00, 0.04, 0.00]

// 3: classic heatmap — black -> red -> yellow -> white
var pal3 = [
  0.00, 0.00, 0.00, 0.00,
  0.50, 1.00, 0.00, 0.00,
  0.85, 1.00, 1.00, 0.00,
  1.00, 1.00, 1.00, 1.00]

var cur = 0            // current palette index
var nxt = 1            // its cyclic successor
var holding = 1        // 1 = hold phase, 0 = transitioning
var phase = 0          // seconds elapsed in the current phase
var blend = 0          // 0..1 blend fraction

var curPal = pal1      // aliases to the two palettes in play
var nxtPal = pal2
var active = array(RES * 4)   // the blended palette handed to the engine

var lookA = array(3)   // scratch colors for the rebuild loop
var lookB = array(3)

function setAliases() {
  if (cur == 0) curPal = pal1
  if (cur == 1) curPal = pal2
  if (cur == 2) curPal = pal3
  if (nxt == 0) nxtPal = pal1
  if (nxt == 1) nxtPal = pal2
  if (nxt == 2) nxtPal = pal3
}

// User-space palette lookup (we must sample two palettes at once, which the
// engine's paint() can't do). Writes r,g,b into out.
function paletteGet(p, t, out) {
  var n = arrayLength(p) / 4
  var i = 0
  while (i < n - 1 && p[i * 4] < t) i += 1
  var pos = p[i * 4]
  if (i == 0 || pos <= t) {
    // exact hit, or clamped at either end: return the row directly
    out[0] = p[i * 4 + 1]
    out[1] = p[i * 4 + 2]
    out[2] = p[i * 4 + 3]
    return
  }
  var lo = (i - 1) * 4
  var f = (t - p[lo]) / (pos - p[lo])
  out[0] = p[lo + 1] + (p[i * 4 + 1] - p[lo + 1]) * f
  out[1] = p[lo + 2] + (p[i * 4 + 2] - p[lo + 2]) * f
  out[2] = p[lo + 3] + (p[i * 4 + 3] - p[lo + 3]) * f
}

// Rebuild the active palette: sample both palettes at RES even positions and
// mix by the current blend fraction.
function rebuildActive() {
  for (var k = 0; k < RES; k++) {
    var t = k / (RES - 1)
    paletteGet(curPal, t, lookA)
    paletteGet(nxtPal, t, lookB)
    active[k * 4] = t
    active[k * 4 + 1] = lookA[0] + (lookB[0] - lookA[0]) * blend
    active[k * 4 + 2] = lookA[1] + (lookB[1] - lookA[1]) * blend
    active[k * 4 + 3] = lookA[2] + (lookB[2] - lookA[2]) * blend
  }
  setPalette(active)
}

// Library surface (unused by the demo renderer): emit the dual-palette blend
// at color position t straight to the current pixel.
function paintBlended(t) {
  paletteGet(curPal, t, lookA)
  paletteGet(nxtPal, t, lookB)
  rgb(lookA[0] + (lookB[0] - lookA[0]) * blend,
      lookA[1] + (lookB[1] - lookA[1]) * blend,
      lookA[2] + (lookB[2] - lookA[2]) * blend)
}

var inited = 0
var osc = 0

export function beforeRender(delta) {
  if (!inited) {
    inited = 1
    setAliases()
    rebuildActive()
  }
  phase += delta / 1000
  if (phase > 3600) phase = 0   // paranoia wrap; phases reset far sooner

  if (!autoCycle) {
    // hold the picked palette: settle onto it once, then leave it alone
    if (cur != pickedPal || blend != 0) {
      cur = pickedPal
      nxt = (cur + 1) % 3
      setAliases()
      blend = 0
      holding = 1
      phase = 0
      rebuildActive()
    }
  } else if (holding) {
    if (phase >= HOLD_S) {
      holding = 0
      phase = 0
    }
  } else {
    blend = phase / TRANS_S
    if (blend >= 1) {
      // transition done: the next palette becomes current
      cur = nxt
      nxt = (nxt + 1) % 3
      setAliases()
      blend = 0
      holding = 1
      phase = 0
    }
    rebuildActive()   // only during transitions — the "fast" part
  }

  // gradient wash: triangle oscillation, ~6.5 s round trip
  // (0.1 * ratio is exactly 0.1 at the control's default)
  osc = triangle(time(0.1 * (washSecs / 6.55)))
}

export function render(index) {
  paint(frac(index / pixelCount + osc * 0.5), 1)
}
