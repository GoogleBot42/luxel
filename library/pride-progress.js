// name: Pride Progress
// Clean-room reimplementation from a prose functional description of the
// community pattern "Pride Progress"; original source never consulted.
//
// The Progress Pride flag as eleven equal hard-edged stripes (chevron
// colors then the six rainbow stripes), scrolling continuously. A triangle
// wave of position mirrors the sequence about the strip midpoint, so the
// two halves appear to scroll in opposite directions.
//
// Note: the original's stripe values were hand-tuned for HDR (APA102-class)
// drivers with many stripes at <1% brightness. These values are re-balanced
// for ordinary strips, keeping the qualitative ordering: black darkest,
// brown barely visible, orange the standout brightest.

var numStripes = 11
var hues = array(numStripes)
var sats = array(numStripes)
var vals = array(numStripes)

function stripe(i, h, s, v) {
  hues[i] = h
  sats[i] = s
  vals[i] = v
}
stripe(0, 0, 0, 0)          // black (rendered fully off)
stripe(1, 0.07, 1, 0.05)    // warm brown, barely-glowing ember
stripe(2, 0.55, 0.45, 0.15) // dim pastel light blue
stripe(3, 0.9, 0.85, 0.5)   // pink/magenta, moderately bright
stripe(4, 0.1, 0.25, 0.12)  // faint desaturated warm white
stripe(5, 0, 1, 0.4)        // red, modest
stripe(6, 0.08, 1, 1)       // orange, brightest by a wide margin
stripe(7, 0.14, 1, 0.4)     // golden yellow, modest
stripe(8, 0.333, 1, 0.1)    // very dim pure green
stripe(9, 0.667, 1, 0.1)    // very dim pure blue
stripe(10, 0.78, 1, 0.1)    // very dim violet

var t1 = 0

export function beforeRender(delta) {
  t1 = time(0.12)   // one full scroll cycle every ~8 s
}

export function render(index) {
  // triangle of normalized position mirrors the flag about the midpoint
  var p = mod(triangle(index / pixelCount) - t1, 1)
  var bin = min(floor(p * numStripes), numStripes - 1)
  hsv(hues[bin], sats[bin], vals[bin])
}
