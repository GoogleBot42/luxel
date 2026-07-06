// name: 2D Wandering Fireball
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D Wandering Fireball"; original source never consulted.

// A soft glowing ball drifting on a Lissajous-style wander (triangle wave on
// x, sine wave on y, differing periods), slowly cycling hue with a slightly
// hue-shifted hot core, over a very dim wash of the same hue.

var bx = 0.5
var by = 0.5
var hue = 0

const TOL = 0.4            // a little under half the display width

export function beforeRender(delta) {
  bx = triangle(time(0.1))   // linear back-and-forth, ~6.5 s
  by = wave(time(0.15))      // eased at the edges, ~9.8 s (≈2:3 ratio with x)
  hue = time(0.6)            // hue lap ~39 s
}

export function render2D(index, x, y) {
  // triangular closeness profile per axis; product makes a rounded blob
  var cx = max(0, 1 - abs(x - bx) / TOL)
  var cy = max(0, 1 - abs(y - by) / TOL)
  var p = cx * cy

  if (p > 0.1) {
    // hot core is nudged forward on the hue wheel
    var h = p > 0.8 ? hue + 0.06 : hue
    // fringe dim, always fairly saturated
    hsv(h, 0.55 + 0.45 * p, p - 0.08)
  } else {
    // dim, moderately saturated wash of the same cycling hue
    hsv(hue, 0.6, 0.03)
  }
}
