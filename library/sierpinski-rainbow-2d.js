// name: Sierpinski Rainbow 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sierpinski Rainbow 2D"; original source never consulted.

// The whole fractal is one bitwise AND of the two coordinates: 16.16
// fixed-point x & y ANDs the binary fraction bits, and the surviving bits
// trace the classic Sierpinski triangle (Pascal's triangle mod 2). That
// static scalar field is run through a chain of wave() calls offset by a
// slow clock (brightness ripples) while a second, ~30% slower clock spins
// the hue wheel. On a bare strip it degrades to a center-mirrored rainbow
// ripple.

var rippleT = 0
var hueT = 0

export function beforeRender(delta) {
  rippleT = time(0.055)   // brightness-ripple clock, ~3.6 s per lap
  hueT = time(0.078)      // hue clock, ~5.1 s — roughly 30% slower
}

export function render2D(index, x, y) {
  var f = x & y           // fixed-point AND of fraction bits = Sierpinski
  var v = wave(f)
  v = wave(v + rippleT)
  v = wave(v + rippleT)
  v = v * v               // deepen contrast: dark voids, bright filaments
  hsv(f + hueT, 1, v)
}

export function render(index) {
  // folded ramp: 1 at the strip midpoint, 0 at both ends, plus one extra
  // wave-shaping pass; then the same time-offset chain as the 2D path
  var s = wave(triangle(index / pixelCount))
  var v = wave(s)
  v = wave(v + rippleT)
  v = wave(v + rippleT)
  v = v * v
  hsv(s + hueT, 1, v)
}
