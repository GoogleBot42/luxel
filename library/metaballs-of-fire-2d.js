// name: Metaballs of Fire 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Metaballs of Fire 2D"; original source never consulted.

// A pool of drifting control points in the unit square. Per pixel, a running
// product of (distance * spread) is min'd into an accumulator; pixels where
// the field drops below a small cutoff light up with a fiery ramp. Nearby
// points multiply their distances together, so blobs bulge and fuse.

var MAX_POINTS = 8
var CUTOFF = 0.083          // roughly a twelfth of the unit scale

var px = array(MAX_POINTS)
var py = array(MAX_POINTS)
var vx = array(MAX_POINTS)
var vy = array(MAX_POINTS)

var numPoints = 5
var spread = 1.5
var speed = 0.5

function randomizePoints() {
  for (var i = 0; i < MAX_POINTS; i++) {
    px[i] = random(1)
    py[i] = random(1)
    vx[i] = random(1) - 0.5   // centered on zero, either sign
    vy[i] = random(1) - 0.5
  }
}

function recomputeSpread() {
  // grows modestly with more points so blob size stays sensible
  spread = 1.5 + (numPoints - 4) * 0.125   // 1.5 .. 2.0
}

//# min=0 max=1 step=0.01 default=0.25
export function sliderNumberOfPoints(v) {
  numPoints = floor(4 + v * (MAX_POINTS - 4) + 0.5)
  recomputeSpread()
  randomizePoints()
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  speed = v
}

randomizePoints()
recomputeSpread()

export function beforeRender(delta) {
  // Deliberately frame-rate dependent (faithful quirk): fixed step per frame.
  var step = speed * 0.12
  for (var i = 0; i < numPoints; i++) {
    px[i] += vx[i] * step
    py[i] += vy[i] * step
    // wall bounce: clamp + flip. Only the first wall hit is handled per
    // frame (faithful shortcut) via the else-if chain.
    if (px[i] < 0) { px[i] = 0; vx[i] = -vx[i] }
    else if (px[i] > 1) { px[i] = 1; vx[i] = -vx[i] }
    else if (py[i] < 0) { py[i] = 0; vy[i] = -vy[i] }
    else if (py[i] > 1) { py[i] = 1; vy[i] = -vy[i] }
  }
}

export function render2D(index, x, y) {
  // metaball field: running product of distances, min'd at each step
  var field = 1
  for (var i = 0; i < numPoints; i++) {
    var d = field * spread * dist(x, y, px[i], py[i])
    field = min(field, d)
  }

  if (field >= CUTOFF) {
    rgb(0, 0, 0)              // background: pure black
  } else {
    var depth = CUTOFF - field            // how far inside the threshold
    var h = depth * 0.98                  // deep red rim -> amber core
    // Concentric molten banding: the wave covers a bit over a third of a
    // period across the field range, giving a thin brighter rim, a dark ring
    // just inside it, then a smooth climb to the hot core. Peaks well below
    // 1 so nothing ever clips and most of the picture stays deep dim red.
    var v = 1.17 - wave(field * 4.7) * 0.97
    hsv(h, 1, clamp(v, 0, 1))
  }
}
