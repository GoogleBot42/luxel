// name: 2D Wandering Fireball
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D Wandering Fireball"; original source never consulted.

// A soft glowing ball drifting on a Lissajous-style wander (triangle wave on
// x, sine wave on y, differing periods), slowly cycling hue with a slightly
// hue-shifted hot core, over a very dim wash of the same hue.

var bx = 0.5
var by = 0.5
var hue = 0

// Tunables — the top-level values reproduce the constants the port shipped
// with (ball 40% of the display, 1x wander, ~39 s hue lap, 3% wash), so an
// untouched pattern renders exactly as before. Clock values are time()
// intervals: seconds / 65.536.
var tol = 0.4        // ball half-width, fraction of the display
var xClock = 0.1     // x sweep, ~6.5 s
var yClock = 0.15    // y sweep, ~9.8 s (≈2:3 ratio with x)
var hueClock = 0.6   // hue lap, ~39 s
var bgV = 0.03       // background wash brightness

// Ball diameter as a percentage of the display; 100% just fills the frame.
//# min=5 max=100 step=1 default=40
export function sliderBallSizePercent(v) { tol = clamp(v, 5, 100) / 100 }

// Wander pace. 1x keeps the 2:3 Lissajous periods the pattern was tuned for.
//# min=0.1 max=4 step=0.1 default=1
export function sliderWanderSpeed(v) {
  var s = max(v, 0.05)
  xClock = 0.1 / s
  yClock = 0.15 / s
}

// Seconds for one full trip around the color wheel.
//# min=2 max=120 step=1 default=39
export function sliderHueCycleSeconds(v) { hueClock = max(v, 1) / 65.536 }

// Brightness of the dim background wash, in percent of full.
//# min=0 max=20 step=1 default=3
export function sliderBackgroundPercent(v) { bgV = clamp(v, 0, 20) / 100 }

export function beforeRender(delta) {
  bx = triangle(time(xClock))   // linear back-and-forth
  by = wave(time(yClock))       // eased at the edges
  hue = time(hueClock)
}

export function render2D(index, x, y) {
  // triangular closeness profile per axis; product makes a rounded blob
  var cx = max(0, 1 - abs(x - bx) / tol)
  var cy = max(0, 1 - abs(y - by) / tol)
  var p = cx * cy

  if (p > 0.1) {
    // hot core is nudged forward on the hue wheel
    var h = p > 0.8 ? hue + 0.06 : hue
    // fringe dim, always fairly saturated
    hsv(h, 0.55 + 0.45 * p, p - 0.08)
  } else {
    // dim, moderately saturated wash of the same cycling hue
    hsv(hue, 0.6, bgV)
  }
}
