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

export function beforeRender(delta) {
  tRipple = time(0.05)  // ~3.3 s cycle
  tHue = time(0.065)    // ~4.3 s cycle
}

// Shared shaping: nested waves offset by the ripple clock, squared for
// contrast so dark regions dominate and bright filaments pop.
function shimmer(f) {
  var v = wave(wave(wave(f) + tRipple) + tRipple)
  return v * v
}

export function render2D(index, x, y) {
  var f = x & y  // fixed-point AND of fractional coords -> Sierpinski field
  hsv(f + tHue, 1, shimmer(f))
}

// 1D fallback: a folded position ramp (1 at the strip midpoint, 0 at both
// ends) stands in for the fractal coordinate, with one extra wave-shaping
// step before the same time-offset chain.
export function render(index) {
  var ramp = 1 - abs(2 * index / pixelCount - 1)
  var f = wave(ramp)
  hsv(f + tHue, 1, shimmer(f))
}
