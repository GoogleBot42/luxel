// name: tixy
// Clean-room reimplementation from a prose functional description of the
// community pattern "tixy"; original source never consulted.

// A tixy.land-style demo reel for a 16x16 matrix: a catalog of tiny formulas
// mapping (t, i, x, y) -> signed value. Each sketch plays for a few time
// units, then the reel advances (wrapping at the end of the catalog).
// Mono mode: positive values take one picker hue, negative the other,
// brightness = (|v| + offset)^3. Color mode: hue = v * colorShift.
// Intended for 16x16; GRID is the one constant to change.

var GRID = 16
var NUM_FORMULAS = 20
var START_FORMULA = 12       // "mondrian squares"

var t = 0                    // formula time; also the sketch timer
var fi = START_FORMULA       // current catalog entry
var SKETCH_LEN = 4           // time units per sketch

var posHue = 0               // positive: red
var negHue = 0.5             // negative: cyan-ish
var colorMode = 0            // 0 = mono/two-color, 1 = value->hue ramp
var colorShift = 1           // hue turns per unit value (color mode)
var speedDiv = 3             // time divisor; higher = slower
var brightShift = 0          // -0.5..+0.5 added to displayed magnitude

export function hsvPickerPositiveColor(h, s, v) { posHue = h }
export function hsvPickerNegativeColor(h, s, v) { negHue = h }

//# min=0 max=1 step=1 default=0
export function sliderMonoOrColor(v) { colorMode = round(v) }

//# min=0 max=1 step=0.01 default=0.5
export function sliderColorShift(v) { colorShift = v * 2 }

//# min=0 max=1 step=0.01 default=0.3
export function sliderSpeedShift(v) { speedDiv = max(v * 10, 0.3) }

//# min=0 max=1 step=0.01 default=0.5
export function sliderBrightnessShift(v) { brightShift = v - 0.5 }

// ---- the catalog: (t, i, x, y) -> roughly -1..1 ----------------------------
function evalFormula(n, t, i, x, y) {
  var cx = x - 7.5, cy = y - 7.5
  if (n == 0)  return random(2) - 1                                   // static
  if (n == 1)  return sin(t)                                          // pulse
  if (n == 2)  return sin(y / 2 - t * 3)                              // falling bars
  if (n == 3)  return sin(t * 2 - hypot(cx, cy))                      // ripples
  if (n == 4)  return (x & y) ? -0.1 : 1                              // sierpinski
  if (n == 5)  return mod(floor(x + t * 4) + y, 2) * 2 - 1            // checker scroll
  if (n == 6)  return sin(atan2(cy, cx) * 3 + t * 2)                  // pinwheel
  if (n == 7)  return sin(hypot(cx, cy) - t * 4)                      // expanding rings
  if (n == 8)  return sin(x / 2 + t * 3 + sin(y / 2 + t * 2))         // waves
  if (n == 9)  return 1 - hypot(cx - 5 * sin(t), cy - 5 * sin(t * 1.7)) / 5  // blob
  if (n == 10) return sin((x + y) / 3 + t * 3)                        // diagonals
  if (n == 11) return (x == 0 || x == 15 || y == 0 || y == 15) ? sin(t * 2) : -0.15  // frame
  if (n == 12)                                                        // mondrian squares
    return (mod(x, 4) && mod(y, 4)) ? sin(t + floor(x / 4) * 1.7 + floor(y / 4) * 2.3) : 0
  if (n == 13) return sin(x * y / 8 + t)                              // moire
  if (n == 14) return (random(2) - 1) * wave(t / 4)                   // breathing noise
  if (n == 15) return sin(x - t * 5) * cos(y / 3)                     // column chase
  if (n == 16) return (1 - hypot(cx, cy) / 8) * sin(t)                // pulsing sphere
  if (n == 17) return mod(floor(x / 2) + floor(y / 2) + floor(t * 2), 2) * 2 - 1  // big checker
  if (n == 18) {                                                      // comet rain
    var d = mod(y + t * 6 + hash(x) * 16, 16)
    return d < 3 ? 1 - d / 3 : -0.05
  }
  return sin(atan2(cy, cx) * 2 + hypot(cx, cy) - t * 3)               // 19: spiral
}
// ----------------------------------------------------------------------------

export function beforeRender(delta) {
  t += delta / 1000 / speedDiv
  if (t > SKETCH_LEN) {
    t = 0
    fi = mod(fi + 1, NUM_FORMULAS)   // wrap: don't walk off the catalog
  }
}

function paintValue(v) {
  var m = abs(v)
  if (colorMode) {
    hsv(v * colorShift, 1, clamp(m + brightShift, 0, 1))
  } else {
    var b = clamp(m + brightShift, 0, 1)
    hsv(v >= 0 ? posHue : negHue, 1, b * b * b)
  }
}

export function render2D(index, x, y) {
  var col = floor(x * 15.99)
  var row = floor(y * 15.99)
  paintValue(evalFormula(fi, t, row * GRID + col, col, row))
}

// 1D: treat the strip as consecutive GRID-pixel rows, same formula and colors
export function render(index) {
  var col = mod(index, GRID)
  var row = floor(index / GRID)
  paintValue(evalFormula(fi, t, index, col, row))
}
