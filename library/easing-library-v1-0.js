// name: Easing Library v1.0
// Clean-room reimplementation from a prose functional description of the
// community pattern "Easing Library v1.0"; original source never consulted.
// The thirty standard easings are Luxel builtins (see docs/lang.md), so this
// pattern is now just the showcase: a self-advancing demo that graphs the
// current curve (2D) or warps a rainbow by it (1D), with a marker tracing
// its velocity profile.

// the standard thirty, families in in/out/in-out order — builtins are
// first-class values, so the table is just references to them
ez = array(30)
ez[0]  = easeInSine
ez[1]  = easeOutSine
ez[2]  = easeInOutSine
ez[3]  = easeInQuad
ez[4]  = easeOutQuad
ez[5]  = easeInOutQuad
ez[6]  = easeInCubic
ez[7]  = easeOutCubic
ez[8]  = easeInOutCubic
ez[9]  = easeInQuart
ez[10] = easeOutQuart
ez[11] = easeInOutQuart
ez[12] = easeInQuint
ez[13] = easeOutQuint
ez[14] = easeInOutQuint
ez[15] = easeInExpo
ez[16] = easeOutExpo
ez[17] = easeInOutExpo
ez[18] = easeInCirc
ez[19] = easeOutCirc
ez[20] = easeInOutCirc
ez[21] = easeInBack
ez[22] = easeOutBack
ez[23] = easeInOutBack
ez[24] = easeInElastic
ez[25] = easeOutElastic
ez[26] = easeInOutElastic
ez[27] = easeInBounce
ez[28] = easeOutBounce
ez[29] = easeInOutBounce

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
