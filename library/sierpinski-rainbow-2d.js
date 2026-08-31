// name: Sierpinski Rainbow 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sierpinski Rainbow 2D"; original source never consulted.

// The whole fractal is one bitwise AND of the two coordinates: x and y are
// 16.16 fixed-point fractions, so `x & y` ANDs their binary fraction bits,
// and the surviving-bit structure is the classic Sierpinski triangle (the
// Pascal's-triangle-mod-2 trick). Brightness ripples come from chaining
// wave() into itself with a time offset injected at each stage; hue is the
// raw fractal scalar plus a slightly slower rotating phase.

var tRipple = 0 // brightness-ripple clock
var tHue = 0    // hue-rotation clock, ~30% slower

// time(n) laps in n * 65.536 s, so the seconds controls divide by that.
// Defaults reproduce the untouched pattern: 0.05 -> ~3.3 s ripple,
// 0.065 -> ~4.3 s hue lap.
var rippleInterval = 0.05
var hueInterval = 0.065
var zoom = 1        // coordinate scale fed into the fractal AND
var colorRange = 1  // turns of hue spanned by the fractal scalar

// Ripple period: how long the brightness shimmer takes to repeat.
//# min=0.5 max=30 step=0.1 default=3.3
export function sliderRippleSeconds(v) { rippleInterval = max(0.25, v) / 65.536 }

// Seconds for the whole image to walk once around the colour wheel.
//# min=0.5 max=60 step=0.1 default=4.3
export function sliderHueCycleSeconds(v) { hueInterval = max(0.25, v) / 65.536 }

// Scale of the coordinates fed to the fractal: below 1 zooms into the big
// triangle, above 1 packs more (and finer) copies onto the panel.
//# min=0.25 max=4 step=0.25 default=1
export function sliderZoom(v) { zoom = max(0.05, v) }

// Degrees of the colour wheel the fractal scalar spans.
//# min=0 max=1440 step=15 default=360
export function sliderColorRange(v) { colorRange = v / 360 }

export function beforeRender(delta) {
  tRipple = time(rippleInterval)
  tHue = time(hueInterval)
}

// Shared shaping: nested waves offset by the ripple clock, squared for
// contrast so dark regions dominate and bright filaments pop.
function shimmer(f) {
  var v = wave(wave(wave(f) + tRipple) + tRipple)
  return v * v
}

export function render2D(index, x, y) {
  // fixed-point AND of the (scaled) fractional coords -> Sierpinski field
  var f = (x * zoom) & (y * zoom)
  hsv(f * colorRange + tHue, 1, shimmer(f))
}

// 1D fallback: a folded position ramp (1 at the strip midpoint, 0 at both
// ends) stands in for the fractal coordinate, with one extra wave-shaping
// step before the same time-offset chain.
export function render(index) {
  var ramp = 1 - abs(2 * index / pixelCount - 1)
  var f = wave(ramp * zoom)
  hsv(f * colorRange + tHue, 1, shimmer(f))
}
