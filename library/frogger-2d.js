// name: Frogger 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Frogger 2D"; original source never consulted.

// Horizontal "traffic lanes" on a square matrix: one glowing rainbow bar per
// row slides back and forth at its own random speed, sweeping fully off both
// edges before reversing. Bars are soft-edged (bright at the center line,
// fading to black at the fringes) via exact point-to-segment distance.
// Lower-numbered lanes occlude higher ones; the background is black.

var MAX_LANES = 32
var numLanes = clamp(floor(sqrt(pixelCount)), 1, MAX_LANES)

var BAR_HALF = 0.4          // half-length of each bar (0.8 of the unit width)

// per-lane state, fixed at startup
var laneSpeed = array(numLanes)   // random period multiplier ~0.5 .. 2.5
var lanePhase = array(numLanes)   // random phase so lanes differ at t = 0
// per-lane state, recomputed each frame
var laneX = array(numLanes)       // bar center x
var laneY = array(numLanes)       // lane center y
var laneHue = array(numLanes)

var i
for (i = 0; i < numLanes; i++) {
  laneSpeed[i] = 0.5 + random(2)
  lanePhase[i] = random(1)
  laneY[i] = (i + 0.5) / numLanes
}

// ---- controls -------------------------------------------------------------
var widthCtl = 0.4, moveCtl = 0.7, colorCtl = 0.6
var lineWidth = 0, movePeriod = 0, colorPeriod = 0

function recalcMappings() {
  // thin particle-like traces at the low end, lane-filling fat bars up high;
  // folds in the lane count so the default is a thin-but-visible line
  lineWidth = (0.05 + widthCtl * widthCtl * 3) / numLanes
  // inverted + quadratically eased: top = ~2 s sweeps, bottom = ~1 minute
  movePeriod = 0.03 + (1 - moveCtl) * (1 - moveCtl) * 0.97
  // same feel for the hue cycle: fast churn up top, minutes-long drift low
  colorPeriod = 0.05 + (1 - colorCtl) * (1 - colorCtl) * 2.5
}
recalcMappings()

//# min=0 max=1 step=0.01 default=0.4
export function sliderLineWidth(v) { widthCtl = v; recalcMappings() }
//# min=0 max=1 step=0.01 default=0.7
export function sliderMovementSpeed(v) { moveCtl = v; recalcMappings() }
//# min=0 max=1 step=0.01 default=0.6
export function sliderColorSpeed(v) { colorCtl = v; recalcMappings() }
// ---------------------------------------------------------------------------

export function beforeRender(delta) {
  var baseHue = time(colorPeriod)
  var i
  for (i = 0; i < numLanes; i++) {
    laneHue[i] = frac(baseHue + i / numLanes)
    // triangle sweep 0..1..0, offset per lane so lanes start out of step;
    // scaled so the bar center travels well past both edges of the display
    var t = triangle(frac(time(movePeriod * laneSpeed[i]) + lanePhase[i]))
    laneX[i] = -(BAR_HALF + 0.2) + t * (1 + 2 * (BAR_HALF + 0.2))
  }
}

export function render2D(index, x, y) {
  var i
  for (i = 0; i < numLanes; i++) {
    // true point-to-segment distance to the horizontal bar in this lane
    var x1 = laneX[i] - BAR_HALF
    var x2 = laneX[i] + BAR_HALF
    var d
    if (x < x1) d = hypot(x - x1, y - laneY[i])
    else if (x > x2) d = hypot(x - x2, y - laneY[i])
    else d = abs(y - laneY[i])

    if (d < lineWidth) {
      hsv(laneHue[i], 1, 1 - d / lineWidth)   // linear falloff to the fringe
      return                                   // first matching lane wins
    }
  }
  rgb(0, 0, 0)
}
