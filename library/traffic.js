// name: Traffic
// Clean-room reimplementation from a prose functional description of the
// community pattern "Traffic"; original source never consulted.

// Morphing moiré line art: a Minsky-style shear pair (y update reuses the
// already-modified x) slowly wobbles the plane, a triangle-fold tiles it,
// and sin(shapeClock * f(x, y)) draws contour families whose density climbs
// through a ~minute-long ramp. Inverse falloff gives hot near-white line
// cores with soft hued tails; the hue cycles the full rainbow every few
// seconds. Four combining modes: sum / max / product / distance.

var lineWidth = 0.08   // contour thickness
var speedDiv = 2       // time divisor (right on the slider = faster)
var repeats = 2        // tile count
var zoom = 1.2         // uniform scale
var mode = 0           // 0 sum, 1 max, 2 product, 3 distance

var t = 0              // accumulator, wraps after ~an hour
var shape = 0          // confined pattern-shaping clock
var w1 = 1             // big mixing weight
var w2 = 0             // tiny wobble weight

//# min=0 max=1 step=0.01 default=0.4
export function sliderLineWidth(v) { lineWidth = 0.01 + v * v * 0.4 }

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) { speedDiv = 3 - v * 2 }   // inverted: right = faster

//# min=0 max=1 step=0.2 default=0.2
export function sliderRepeats(v) { repeats = 1 + floor(v * 5.99) }

//# min=0 max=1 step=0.01 default=0.35
export function sliderScale(v) { zoom = 0.8 + v * 1.2 }

//# min=0 max=1 step=0.333 default=0
export function sliderMode(v) { mode = floor(v * 3.99) }

export function beforeRender(delta) {
  t = mod(t + delta / 1000 / speedDiv, 3600)
  shape = 0.4 + mod(t / 3, 18)      // interesting region; ~minute ramp, then reset
  w2 = 0.005 * sin(t * 0.9)         // tiny sinusoidal wobble
  w1 = 1 - w2

  resetTransform()
  translate(-0.5, -0.5)             // recenter on the middle of the map
  scale(zoom, zoom)
}

export function render2D(index, x, y) {
  // Minsky shear-rotate: y deliberately uses the already-updated x
  x = x * w1 + y * w2
  y = y * w1 - x * w2

  // triangle-fold tiling, expanded several-fold
  var xf = abs(mod(x * repeats, 2) - 1) * 4
  var yf = abs(mod(y * repeats, 2) - 1) * 4

  var f
  if (mode == 0)      f = xf + yf
  else if (mode == 1) f = max(xf, yf)
  else if (mode == 2) f = xf * yf
  else                f = hypot(xf, yf)

  var li = lineWidth / (lineWidth + abs(sin(shape * f)))

  var h = t + li * 0.1
  var s = clamp(1.15 - li, 0, 1)    // cores desaturate toward white
  hsv(h, s, li * li)
}
