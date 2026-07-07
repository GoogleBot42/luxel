// name: 3 color rotation
// Clean-room reimplementation from a prose functional description of the
// community pattern "3 color rotation"; original source never consulted.

// Segmented three-color chase: short segments carry a repeating cyan /
// magenta / warm-yellow sequence that marches one segment along the strip
// on a fixed beat. This implements the described *intent* with both noted
// defects fixed: the role is used to index a three-entry color table (so
// all three colors are reachable and every pixel is painted), and the phase
// advances on wall-clock delta time rather than a frame counter. Segment
// numbers round to nearest, as described, so the first and last segments
// appear half-width.

var segSize = 5
var intervalMs = 500
var accum = 0
var phase = 0

// cyan, magenta/pink, warm yellow-gold
var hues = array(3)
hues[0] = 0.5
hues[1] = 0.87
hues[2] = 0.12

//# min=0 max=1 step=0.04 default=0.17
export function sliderSegmentSize(v) {
  segSize = floor(1 + v * 23)        // 1..24 pixels per segment
}

//# min=0 max=1 step=0.01 default=0.24
export function sliderStepTime(v) {
  intervalMs = 100 + v * 1700        // 0.1..1.8 s per step
}

export function beforeRender(delta) {
  accum += delta
  if (accum > intervalMs) {
    accum = 0
    phase = (phase + 1) % 3
  }
}

export function render(index) {
  var seg = round(index / segSize)
  var role = (phase + seg) % 3
  hsv(hues[role], 1, 1)
}
