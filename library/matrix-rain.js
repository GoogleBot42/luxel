// name: matrix rain
// Clean-room reimplementation from a prose functional description of the
// community pattern "matrix rain"; original source never consulted.

// Single bright dots rain down the columns of a matrix, each drop one lit
// pixel with its own cool hue (green-cyan through blue) and fall speed.
// Drops are bright at the top and fade to nearly nothing near the bottom;
// when a drop passes the bottom its column frees up for a new one.
// Improvement over the described original: fall speed and spawn cadence are
// scaled by the frame delta, so both are frame-rate independent.

// Logical matrix, hardcoded as in the described original: a wide, short
// grid rendered through normalized coordinates, so it letterboxes onto any
// mapped display (on a 16x16 rig a drop is one column wide and two rows
// tall).
const COLS = 32
const ROWS = 8

var dropPos = array(COLS)   // logical rows; -1 = column empty
var dropSpd = array(COLS)   // logical rows per second
var dropHue = array(COLS)

for (var i = 0; i < COLS; i++) dropPos[i] = -1

export function beforeRender(delta) {
  var dt = delta / 1000
  for (var i = 0; i < COLS; i++) {
    if (dropPos[i] >= 0) {
      dropPos[i] += dropSpd[i] * dt
      if (dropPos[i] >= ROWS) dropPos[i] = -1   // fell off: free column
    }
  }
  // ~8 spawn attempts a second, each into one random column
  if (random(1) < 8 * dt) {
    var c = floor(random(COLS))
    if (dropPos[c] < 0) {
      dropPos[c] = 0
      dropSpd[c] = 5 + random(5)       // ~2:1 slowest-to-fastest spread
      dropHue[c] = .42 + random(.26)   // spring green / cyan .. azure blue
    }
  }
}

export function render2D(index, x, y) {
  var col = floor(x * (COLS - .01))
  var row = floor(y * (ROWS - .01))
  var p = dropPos[col]
  if (p >= 0 && row == floor(p)) {
    // full over the top ~third, ~zero at the bottom, eased hard
    var b = saturate(1.4 - y)
    b = b * b
    b = b * b
    hsv(dropHue[col], 1, b)
  } else {
    rgb(0, 0, 0)
  }
}
