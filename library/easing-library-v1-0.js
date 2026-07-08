// name: Easing Library v1.0
// Clean-room reimplementation from a prose functional description of the
// community pattern "Easing Library v1.0"; original source never consulted.
// The thirty standard easings are implemented from the public easings.net
// reference. A self-advancing demo graphs the current curve (2D) or warps a
// rainbow by it (1D), with a marker tracing its velocity profile.

var BACK1 = 1.70158
var BACK2 = 1.70158 * 1.525
var BACK3 = 1.70158 + 1
var ELC4 = PI2 / 3
var ELC5 = PI2 / 4.5

function bounceOut(x) {
  var n1 = 7.5625
  var d1 = 2.75
  if (x < 1 / d1) return n1 * x * x
  if (x < 2 / d1) { x = x - 1.5 / d1; return n1 * x * x + 0.75 }
  if (x < 2.5 / d1) { x = x - 2.25 / d1; return n1 * x * x + 0.9375 }
  x = x - 2.625 / d1
  return n1 * x * x + 0.984375
}

// thirty easings, families in in/out/in-out order
ez = array(30)
ez[0]  = (x) => 1 - cos((x * PI) / 2)
ez[1]  = (x) => sin((x * PI) / 2)
ez[2]  = (x) => -(cos(PI * x) - 1) / 2
ez[3]  = (x) => x * x
ez[4]  = (x) => 1 - (1 - x) * (1 - x)
ez[5]  = (x) => x < 0.5 ? 2 * x * x : 1 - pow(-2 * x + 2, 2) / 2
ez[6]  = (x) => x * x * x
ez[7]  = (x) => 1 - pow(1 - x, 3)
ez[8]  = (x) => x < 0.5 ? 4 * x * x * x : 1 - pow(-2 * x + 2, 3) / 2
ez[9]  = (x) => x * x * x * x
ez[10] = (x) => 1 - pow(1 - x, 4)
ez[11] = (x) => x < 0.5 ? 8 * x * x * x * x : 1 - pow(-2 * x + 2, 4) / 2
ez[12] = (x) => x * x * x * x * x
ez[13] = (x) => 1 - pow(1 - x, 5)
ez[14] = (x) => x < 0.5 ? 16 * x * x * x * x * x : 1 - pow(-2 * x + 2, 5) / 2
ez[15] = (x) => x <= 0 ? 0 : pow(2, 10 * x - 10)
ez[16] = (x) => x >= 1 ? 1 : 1 - pow(2, -10 * x)
ez[17] = (x) => x <= 0 ? 0 : x >= 1 ? 1 : x < 0.5 ? pow(2, 20 * x - 10) / 2 : (2 - pow(2, -20 * x + 10)) / 2
ez[18] = (x) => 1 - sqrt(1 - x * x)
ez[19] = (x) => sqrt(1 - (x - 1) * (x - 1))
ez[20] = (x) => x < 0.5 ? (1 - sqrt(1 - pow(2 * x, 2))) / 2 : (sqrt(1 - pow(-2 * x + 2, 2)) + 1) / 2
ez[21] = (x) => BACK3 * x * x * x - BACK1 * x * x
ez[22] = (x) => 1 + BACK3 * pow(x - 1, 3) + BACK1 * pow(x - 1, 2)
ez[23] = (x) => x < 0.5 ? (pow(2 * x, 2) * ((BACK2 + 1) * 2 * x - BACK2)) / 2 : (pow(2 * x - 2, 2) * ((BACK2 + 1) * (2 * x - 2) + BACK2) + 2) / 2
ez[24] = (x) => x <= 0 ? 0 : x >= 1 ? 1 : -pow(2, 10 * x - 10) * sin((10 * x - 10.75) * ELC4)
ez[25] = (x) => x <= 0 ? 0 : x >= 1 ? 1 : pow(2, -10 * x) * sin((10 * x - 0.75) * ELC4) + 1
ez[26] = (x) => x <= 0 ? 0 : x >= 1 ? 1 : x < 0.5 ? -(pow(2, 20 * x - 10) * sin((20 * x - 11.125) * ELC5)) / 2 : (pow(2, -20 * x + 10) * sin((20 * x - 11.125) * ELC5)) / 2 + 1
ez[27] = (x) => 1 - bounceOut(1 - x)
ez[28] = (x) => bounceOut(x)
ez[29] = (x) => x < 0.5 ? (1 - bounceOut(1 - 2 * x)) / 2 : (1 + bounceOut(2 * x - 1)) / 2

// exported teaching/debug state
export var elapsed = 0
export var curFunc = 0
export var pingpong = 0
export var curMin = 0
export var curMax = 0
export var prevMin = 0
export var prevMax = 0

var lastSwitch = 0

export function beforeRender(delta) {
  elapsed = elapsed + delta / 1000
  if (elapsed - lastSwitch > 5) {
    // snapshot then reset the running range trackers
    prevMin = curMin
    prevMax = curMax
    curMin = 999
    curMax = -999
    curFunc = (curFunc + 1) % 30
    lastSwitch = elapsed
  }
  // ping-pong 0..1..0 over ~2 s, reset on each function change
  pingpong = triangle((elapsed - lastSwitch) / 2)
}

// 1D: rainbow whose hue distribution is warped by the current easing
export function render(index) {
  var e = ez[curFunc]
  var v = e(index / pixelCount)
  hsv(v, 1, 1)
}

// 2D: graph the current easing as a rainbow line, with a white velocity marker
export function render2D(index, x, y) {
  var e = ez[curFunc]
  var v = e(x)

  if (v < curMin) curMin = v
  if (v > curMax) curMax = v

  var tol = 0.7 / sqrt(pixelCount)

  hsv(0, 0, 0)   // black background; later calls override

  // dim gray reference diagonal (optional)
  if (abs(x - y) < tol * 0.5) { hsv(0, 0, 0.12) }

  // the eased curve, colored by its own value
  if (abs(y - v) < tol) { hsv(v, 1, 1) }

  // white marker: thin band just above midline, x tracks eased pingpong
  var mx = e(pingpong)
  if (y >= 0.5 && y < 0.5 + tol * 2 && abs(x - mx) < tol) { rgb(1, 1, 1) }
}
