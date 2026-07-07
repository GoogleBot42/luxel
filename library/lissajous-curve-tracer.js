// name: Lissajous curve tracer
// Clean-room reimplementation from a prose functional description of the
// community pattern "Lissajous curve tracer"; original source never
// consulted.

// A bright dot traces a Lissajous figure over a 2D map, leaving a glowing
// trail. Persistence spans oscilloscope-style fading trails to permanent
// paint; the dot's hue wanders gently around a picked base color, so
// long-lived trails carry a color-history gradient. Simulated on a 16x16
// virtual canvas: per-cell brightness and per-cell hue, sampled by render2D.

const SIZE = 16
const CELLS = 256
const CLEAR_ON_SHAPE_CHANGE = 1   // built-in flag: clear trail when A/B/delta move

var bri = array(CELLS)            // persistence/trail buffer
var hueBuf = array(CELLS)         // hue each cell was painted with

var baseHue = 0.6, baseSat = 1, baseVal = 1
var hueAmt = 0.15                 // hue excursion around the base
var fade = 0.96                   // per-frame trail decay factor
var density = 9                   // closeness falloff (inverse of dot size)
var A = 3                         // horizontal frequency ratio
var B = 2                         // vertical frequency ratio
var delta = PI2 / 4               // horizontal phase offset

var dotX = 0.5, dotY = 0.5
var hueNow = 0.6

// --- adjustable-speed clocks that don't jump: fold the difference between
// the old and new sawtooth into a stored phase offset on retime -----------
var dotInterval = 0.011, dotOffset = 0
function retimeDot(newInterval) {
  dotOffset = mod(dotOffset + time(dotInterval) - time(newInterval), 1)
  dotInterval = newInterval
}
var hueInterval = 0.08, hueOffset = 0
function retimeHue(newInterval) {
  hueOffset = mod(hueOffset + time(hueInterval) - time(newInterval), 1)
  hueInterval = newInterval
}

function clearTrail() {
  arrayReplace(bri, 0)
}

// --- controls -------------------------------------------------------------
export function hsvPickerDotColor(h, s, v) {
  baseHue = h
  baseSat = s
  baseVal = v
}

//# min=0 max=1 step=0.01 default=0.15
export function sliderHueShiftAmount(v) {
  hueAmt = v
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderHueShiftSpeed(v) {
  // strongly eased: most of the slider is slow
  retimeHue(0.005 + 0.6 * pow(1 - v, 3))
}

//# min=0 max=1 step=0.01 default=0.6
export function sliderPersistence(v) {
  // 0 → trails die in a fraction of a second; top of range → permanent
  fade = v > 0.99 ? 1 : 1 - 0.3 * (1 - v) * (1 - v)
}

//# min=0 max=1 step=0.01 default=0.35
export function sliderDotSize(v) {
  // eased: big dot = gentle wide falloff, small dot = tight point
  density = 2 + 18 * (1 - v) * (1 - v)
}

//# min=0 max=1 step=0.01 default=0.7
export function sliderSpeed(v) {
  // strongly eased, cubic-feeling: blur at the top, slow crawl at the bottom
  retimeDot(0.003 + 0.3 * pow(1 - v, 3))
}

//# min=0 max=1 step=0.05 default=0.3
export function sliderA(v) {
  var n = floor(1 + v * 7.99)          // small integers 1..8
  if (n != A) {
    A = n
    if (CLEAR_ON_SHAPE_CHANGE) clearTrail()
  }
}

//# min=0 max=1 step=0.05 default=0.15
export function sliderB(v) {
  var n = floor(1 + v * 7.99)
  if (n != B) {
    B = n
    if (CLEAR_ON_SHAPE_CHANGE) clearTrail()
  }
}

//# min=0 max=1 step=0.05 default=0.25
export function sliderDelta(v) {
  var d = floor(v * 7.99) / 8 * PI2    // eight even steps around a cycle
  if (d != delta) {
    delta = d
    if (CLEAR_ON_SHAPE_CHANGE) clearTrail()
  }
}

// --- per frame: move the dot, drift the hue, stamp the canvas -------------
export function beforeRender(rawDelta) {
  var phase = mod(time(dotInterval) + dotOffset, 1) * PI2
  // classic Lissajous parametrization, mapped into the 0..1 canvas;
  // amplitude shrinks a touch when the dot is fat so blobs don't clip
  var amp = 0.47 - 0.04 / density
  dotX = 0.5 + amp * sin(phase * A + delta)
  dotY = 0.5 + amp * sin(phase * B)

  // hue = base plus a triangle-wave excursion centered on it
  var huePhase = mod(time(hueInterval) + hueOffset, 1)
  hueNow = baseHue + hueAmt * (triangle(huePhase) - 0.5)

  var i = 0
  for (var row = 0; row < SIZE; row++) {
    var cy = (row + 0.5) / SIZE
    for (var col = 0; col < SIZE; col++) {
      var cx = (col + 0.5) / SIZE
      var c = clamp(1 - hypot(cx - dotX, cy - dotY) * density, 0, 1)
      // max(faded old, new closeness): trails coexist with the fresh dot
      // without additive blowout
      bri[i] = max(bri[i] * fade, c)
      if (c > 0) hueBuf[i] = hueNow    // trail remembers its paint color
      i++
    }
  }
}

export function render2D(index, x, y) {
  var i = floor(y * 15.99) * 16 + floor(x * 15.99)
  hsv(hueBuf[i], baseSat, bri[i] * baseVal)
}
