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

// --- grid discovery ----------------------------------------------------
// render2D only sees normalized coordinates, so the panel's real dimensions
// come from the smallest non-zero step in the pixel map (same trick as
// us-flag-2d.js). The velocity marker below has to land on exactly ONE
// pixel, and a distance tolerance cannot do that: any tolerance wide enough
// never to miss a column is wider than half the pixel pitch, so it lights
// two columns whenever it falls near a cell border. On the playground's
// 16x16 preview that is about a third of all frames, which is why the
// marker read as a two-pixel ball there while a 64x64 panel hid it
// (Gitea #285).
var lastX = 1
var lastY = 1
var minX = 2
var minY = 2
var built = 0

function scanMap(i, x, y, z) {
  if (x > 0.0005 && x < minX) minX = x
  if (y > 0.0005 && y < minY) minY = y
}

function layout() {
  minX = 2
  minY = 2
  mapPixels(scanMap)
  lastX = minX < 2 ? round(1 / minX) : 1
  lastY = minY < 2 ? round(1 / minY) : 1
  if (lastX < 1) lastX = 1
  if (lastY < 1) lastY = 1
  built = 1
}

export function beforeRender(delta) {
  if (!built) layout()
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

  // white marker: exactly one pixel, on the midline row, its column tracking
  // the eased pingpong. Snapped to the grid instead of tested with a
  // distance tolerance so it stays one pixel on every panel size, and
  // clamped so the back/elastic families' overshoot presses it against the
  // end of the row instead of hiding it off-panel.
  var mx = e(pingpong)
  var mcol = clamp(round(mx * lastX), 0, lastX)
  if (round(y * lastY) == round(0.5 * lastY) && round(x * lastX) == mcol) {
    rgb(1, 1, 1)
  }
}
