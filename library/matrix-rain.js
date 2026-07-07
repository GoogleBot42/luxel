// name: matrix rain
// Clean-room reimplementation from a prose functional description of the
// community pattern "matrix rain"; original source never consulted.

// Single bright dots rain down the columns of a matrix, each drop one lit
// pixel with its own cool hue (green-cyan through blue) and fall speed.
// Drops are bright at the top and fade to nearly nothing near the bottom;
// when a drop passes the bottom its column frees up for a new one.
// Improvement over the described original: the fall step is scaled by the
// frame delta, so speed is frame-rate independent.

const COLS = 16
const ROWS = 16

var dropPos = array(COLS)   // row units; -1 = column empty
var dropSpd = array(COLS)   // rows per nominal (60 fps) frame
var dropHue = array(COLS)

arrayReplace(dropPos, -1)

export function beforeRender(delta) {
  var step = delta / 16.667   // nominal-frame units, delta-scaled
  for (var i = 0; i < COLS; i++) {
    if (dropPos[i] >= 0) {
      dropPos[i] += dropSpd[i] * step
      if (dropPos[i] >= ROWS) dropPos[i] = -1   // fell off: free column
    }
  }
  // a bit under half the frames, try to spawn in one random column
  if (random(1) < .4) {
    var c = floor(random(COLS))
    if (dropPos[c] < 0) {
      dropPos[c] = 0
      dropSpd[c] = .09 + random(.09)   // ~2:1 slowest-to-fastest spread
      dropHue[c] = .38 + random(.28)   // spring green / cyan .. azure blue
    }
  }
}

export function render2D(index, x, y) {
  var col = floor(x * 15.99)
  var row = floor(y * 15.99)
  var p = dropPos[col]
  if (p >= 0 && row == floor(p)) {
    // full near the top, ~zero somewhat past full travel, eased hard
    var b = saturate(1 - y * .85)
    b = b * b
    b = b * b
    hsv(dropHue[col], 1, b)
  } else {
    rgb(0, 0, 0)
  }
}
