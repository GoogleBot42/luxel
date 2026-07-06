// name: Animated Asterisks 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Animated Asterisks 2D"; original source never consulted.

// N full-span line segments fan evenly across a half-turn through the
// display center, forming a rotating asterisk. Each arm carries its own
// hue (evenly spread around the wheel, drifting together) and fades
// linearly to black at its edge. By default the arm thickness breathes
// slowly over about a minute — hairline sparkle at the thin extreme, a
// flooded matrix at the thick one. Widths are expressed as fractions of
// the unit square, so no square-matrix assumption is needed.

const MAX_LINES = 24
const HALF_LEN = 0.75    // half-length from center: spans the unit square
const SEG_LEN = 1.5

var x0 = array(MAX_LINES)      // one endpoint of each segment
var y0 = array(MAX_LINES)
var dirX = array(MAX_LINES)    // unit direction along each segment
var dirY = array(MAX_LINES)
var hues = array(MAX_LINES)

var numLines = 6
//# min=0 max=1 step=0.01 default=0.25
export function sliderNumberOfLines(v) {
  numLines = max(1, floor(v * 24.99))   // 1..24, default about half a dozen
}

var widthSetting = 0.5
//# min=0 max=1 step=0.01 default=0.5
export function sliderLineWidth(v) {
  widthSetting = v
}

var animateWidth = 1
//# min=0 max=1 step=1 default=1
export function sliderAnimateWidth(v) {
  animateWidth = v > 0.05   // toggle: on except at the very bottom
}

var rotRate = 0.65   // revolutions per second at the default position
//# min=0 max=1 step=0.01 default=0.5
export function sliderRotationSpeed(v) {
  // eased (quadratic), never stopping: 5 s/rev up to 2 rev/s
  rotRate = 0.2 + 1.8 * v * v
}

var colRate = 0.08   // hue cycles per second at the default position
//# min=0 max=1 step=0.01 default=0.2
export function sliderColorSpeed(v) {
  // eased: a wheel in ~50 s at the bottom, well under a second at the top
  colRate = 0.02 + 1.5 * v * v
}

var rot = 0
var hueBase = 0
var halfWidth = 0.05

function widthCurve(v) {
  // hairline up to arms that flood the matrix; more lines -> thinner max
  return 0.004 + v * v * 0.9 / numLines
}

export function beforeRender(delta) {
  var dt = delta / 1000
  rot = mod(rot + dt * rotRate, 1)
  hueBase = mod(hueBase + dt * colRate, 1)

  // slow triangle-wave breathing (~1 min per thick-thin cycle) when the
  // animate toggle is on; phase offset starts it at a medium width
  var w = animateWidth ? triangle(time(0.9) + 0.25) : widthSetting
  halfWidth = widthCurve(w)

  for (var i = 0; i < numLines; i++) {
    // arms spread over a half-turn (each spans the display through center)
    var a = rot * PI2 + PI * i / numLines
    var cx = cos(a)
    var cy = sin(a)
    dirX[i] = cx
    dirY[i] = cy
    x0[i] = 0.5 - cx * HALF_LEN
    y0[i] = 0.5 - cy * HALF_LEN
    hues[i] = hueBase + i / numLines
  }
}

export function render2D(index, x, y) {
  for (var i = 0; i < numLines; i++) {
    var vx = x - x0[i]
    var vy = y - y0[i]
    // true distance to the *segment*: beyond either endpoint use the
    // endpoint distance, else the perpendicular distance
    var along = vx * dirX[i] + vy * dirY[i]
    var d
    if (along < 0) {
      d = hypot(vx, vy)
    } else if (along > SEG_LEN) {
      d = hypot(vx - dirX[i] * SEG_LEN, vy - dirY[i] * SEG_LEN)
    } else {
      d = abs(vx * dirY[i] - vy * dirX[i])
    }
    if (d < halfWidth) {
      // first matching arm wins; linear falloff to the edge
      hsv(hues[i], 1, 1 - d / halfWidth)
      return
    }
  }
  hsv(0, 0, 0)
}
