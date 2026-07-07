// name: 2 Colors
// Clean-room reimplementation from a prose functional description of the
// community pattern "2 Colors"; original source never consulted.

// Static display: alternating equal-width blocks of two user-chosen
// colors. Nothing animates — no beforeRender — the picture only changes
// when a control moves.

// Two HSV triples in one flat array: A in slots 0..2, B in slots 3..5.
// Deliberately seeded with two visible defaults (the original's init
// block was garbled; we don't reproduce it).
var colors = array(6)
colors[0] = 0      // color A: red
colors[1] = 1
colors[2] = 1
colors[3] = 0.667  // color B: blue
colors[4] = 1
colors[5] = 1

var blockWidth = 4

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

//# min=0 max=1 step=0.01 default=0.25
export function sliderSpacing(v) {
  // 0..1 -> block width 1..15; epsilon keeps the bottom of the slider
  // at width 1 (width 0 would divide by zero)
  blockWidth = ceil(v * 15 + 0.001)
}

export function render(index) {
  var base = floor(index / blockWidth) % 2 * 3   // 0 for A, 3 for B
  hsv(colors[base], colors[base + 1], colors[base + 2])
}
