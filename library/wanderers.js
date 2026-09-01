// name: wanderers
// Clean-room reimplementation from a prose functional description of the
// community pattern "wanderers"; original source never consulted.
//
// A few dozen colored dots on a dark matrix, each doing an independent
// slow random walk — one-cell hops every couple dozen frames, like
// fireflies meandering. Each walker keeps one fixed hue for life and the
// set spans the whole rainbow. Horizontal motion wraps (cylinder);
// vertical motion clamps at the top and bottom rows.
//
// Simulated on a 16x16 virtual canvas; render2D samples it, so it works
// on any mapped fixture (the original hardcoded a serpentine panel).

var W = 16
var H = 16
var CELLS = W * H

var numWalkers = 45
// Inverse speed: each frame a walker draws one uniform number in
// [0, SPEED); draws 0..3 pick a direction (right/left/up/down), anything
// else stays put — so a walker steps roughly every SPEED/4 frames.
// Must be >= 4 or walkers would move every frame.
var SPEED = 80

var UNLIT = -1
var canvas = array(CELLS)   // hue per cell, or UNLIT
var walkers = array(numWalkers)

// scatter walkers uniformly at startup
var i
for (i = 0; i < numWalkers; i++) {
  walkers[i] = floor(random(CELLS))
}

export function beforeRender(delta) {
  // Blank the canvas. (arrayReplace splats its value arguments starting at
  // index 0 — it is NOT an array fill, so it has to be a loop.)
  for (var c = 0; c < CELLS; c++) canvas[c] = UNLIT

  for (var w = 0; w < numWalkers; w++) {
    var p = walkers[w]
    var x = p % W
    var y = floor(p / W)

    var r = floor(random(SPEED))
    if (r == 0) {
      x = (x + 1) % W            // right, wrapping
    } else if (r == 1) {
      x = (x + W - 1) % W        // left, wrapping
    } else if (r == 2) {
      y = min(y + 1, H - 1)      // down, clamped
    } else if (r == 3) {
      y = max(y - 1, 0)          // up, clamped
    }
    // else: stay put

    p = y * W + x
    walkers[w] = p
    // walker identity as a hue spread evenly over the wheel; later
    // walkers overwrite earlier ones on collision
    canvas[p] = w / numWalkers
  }
}

export function render2D(index, x, y) {
  var h = canvas[floor(y * 15.99) * W + floor(x * 15.99)]
  if (h < 0) {
    rgb(0, 0, 0)
  } else {
    hsv(h, 1, 1)
  }
}
