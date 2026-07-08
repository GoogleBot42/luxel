// name: Mandelbrot 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Mandelbrot 2D"; original source never consulted.
//
// Despite the name this is an animated Julia set: each pixel's coordinate is
// the iteration's STARTING point and one frame-global complex constant is
// added every step. A slow triangle sweep walks that constant along a shallow
// diagonal near the fractal boundary (so the whole shape blooms and collapses)
// while an independent sawtooth rotates the rainbow. Escaped pixels band
// through the hue wheel by how fast they escaped; interior pixels stay black.

var cap = 14        // iteration cap (detail vs. speed trade-off)
var cRe = 0         // frame-global Julia constant, recomputed each frame
var cIm = 0
var hueOffset = 0   // continuously rotating global hue

//# min=0 max=1 step=0.05 default=0.65
export function sliderIterationDepth(v) {
  // ~5 iterations at the low end to just under 20 at the top
  cap = floor(5 + v * 14)
}

export function beforeRender(delta) {
  // triangle sweep, ~12 s per full back-and-forth, recentered to +/-1
  var sweep = triangle(time(0.183)) * 2 - 1
  // base point hand-picked near the boundary (real ~ -1, small +imag); the
  // shared sweep travels a shallow diagonal, imaginary scaled to ~40% of real
  cRe = -0.8 + sweep
  cIm = 0.156 + sweep * 0.4
  // independent hue rotation, a few seconds per trip round the wheel
  hueOffset = time(0.05)
}

export function render2D(index, x, y) {
  // view window ~1 unit wide, centered on the complex-plane origin (kept small
  // so squared intermediates stay comfortably inside 16.16 range)
  var zRe = x - 0.5
  var zIm = y - 0.5
  var i = 0
  var escaped = 0
  while (i < cap) {
    var re2 = zRe * zRe
    var im2 = zIm * zIm
    // squared-magnitude escape test against squared radius (no sqrt)
    if (re2 + im2 > 4) {
      escaped = 1
      break
    }
    // z = z^2 + c   (imag first, it reads the old zRe)
    zIm = 2 * zRe * zIm + cIm
    zRe = re2 - im2 + cRe
    i = i + 1
  }
  if (escaped) {
    // faster escapes land earlier in the wheel -> banded rainbow
    hsv(hueOffset + i / cap, 1, 1)
  } else {
    hsv(0, 0, 0)
  }
}
