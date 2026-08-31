// name: Line Dancer 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Line Dancer 2D"; original source never consulted.

// A rainbow-striped ribbon twists and coils around the center of the 2D
// surface, screensaver-style. A radius-dependent shear (only x is rewritten)
// smears the image into a dancing line; a ~5 s triangle "breathing" zoom
// widens and narrows the stripes; an optional kaleidoscope repeats the image
// in rotating pie wedges.

var speed = 4.6      // animation clock multiplier (1..10)
var twist = 1.75     // twist parameter (1.25..2)
var sides = 1        // kaleidoscope wedge count (1 = off)

var t = 0            // running time, seconds (wraps after ~10 min)
var clock = 0        // t * speed
var zoom = 0         // 0..1 triangle, ~5 s period
var spin = 0         // kaleidoscope rotation angle

// The dial trims a clock that never stops: even at 0 the twist term keeps
// sweeping, so this is a trim over a live baseline, not an on/off master.
//# min=0 max=1 step=0.01 default=0.4
export function sliderSpeed(v) { speed = 1 + v * 9 }

//# min=0 max=1 step=0.01 default=0.67
export function sliderTwist(v) { twist = 1.25 + v * 0.75 }

//# min=0 max=1 step=0.167 default=0
export function sliderReflections(v) { sides = 1 + floor(v * 6.99) }

export function beforeRender(delta) {
  t = mod(t + delta / 1000, 600)
  clock = t * speed
  // ~5 s breathing cycle. Squaring flattens the BOTTOM of the breath, so the
  // stripe scale lingers near zero: that long, deep trough is the "distort"
  // phase where the whole image melts into a flat dim glow before blooming
  // back into stripes.
  zoom = triangle(time(0.076))
  zoom = zoom * zoom * (2 - zoom)         // 1 at the top, ~2*z^2 near 0
  spin = t * 0.15                    // slow kaleidoscope drift

  resetTransform()
  translate(-0.5, -0.5)              // origin at map center
}

export function render2D(index, x, y) {
  // kaleidoscope: fold the angle into one wedge, add slow spin
  if (sides > 1) {
    var r = hypot(x, y)
    var wedge = PI2 / sides
    var a = mod(atan2(y, x), wedge) + spin
    x = r * cos(a)
    y = r * sin(a)
  }

  // twist: radius-dependent shear; only x is rewritten
  var d = twist - hypot(x, y) * 2.2
  var ang = d * d * sin(d + clock)
  var tx = x * cos(ang) - y * sin(ang)

  // Shading: bright ribbon core carved out of black. The stripe scale is
  // PURELY the zoom (no floor), so when the breath bottoms out the argument
  // collapses to zero for every pixel and the panel settles on wave(0)^2 =
  // 0.25 — a flat, dim, featureless glow instead of a bright line.
  var v = wave(tx * 5 * zoom)
  v = v * v
  var h = tx * zoom + zoom + ang / PI2
  hsv(h, 1, v)
}
