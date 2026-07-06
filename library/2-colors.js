// name: 2 Colors
// Clean-room reimplementation from a prose functional description of the
// community pattern "2 Colors"; original source never consulted.

// Static pattern: the strip alternates equal-width blocks of two
// user-chosen colors. Nothing animates; only the controls change the
// display. Defaults: color A = saturated red, color B = black, so the
// strip comes up as red blocks separated by dark gaps.

var MAX_WIDTH = 15

// Two HSV triples: [hA, sA, vA, hB, sB, vB]
var colors = array(6)
colors[0] = 0     // color A: red
colors[1] = 1
colors[2] = 1
colors[3] = 0     // color B: black
colors[4] = 0
colors[5] = 0

var blockWidth = 5

export function hsvPickerColor1(h, s, v) {
  colors[0] = h
  colors[1] = s
  colors[2] = v
}

export function hsvPickerColor2(h, s, v) {
  colors[3] = h
  colors[4] = s
  colors[5] = v
}

//# min=0 max=1 step=0.01 default=0.3
export function sliderSpacing(v) {
  // Scale to at most MAX_WIDTH pixels and round up; the tiny epsilon
  // keeps the very bottom of the slider at width 1 (never 0, which
  // would divide by zero).
  blockWidth = ceil(v * MAX_WIDTH + 0.001)
}

export function render(index) {
  var base = (floor(index / blockWidth) % 2) * 3
  hsv(colors[base], colors[base + 1], colors[base + 2])
}
