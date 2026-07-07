// name: mapped vertical line 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "mapped vertical line 2D"; original source never consulted.

// A solid vertical bar sweeps left-to-right across the mapped surface at a
// steady rate (sawtooth: it snaps back to the left edge, clipping at the
// edges rather than wrapping). Everything else is a dim saturated backdrop.

var speed = 0.5
var barWidth = 0.02 + 0.5 * 0.48
var lineHue = 0
var bgHue = 0.667
var center = 0

//# min=0 max=1 step=0.01 default=0.5
export function sliderLineSpeed(v) {
  speed = v
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderLineWidth(v) {
  // small floor so the bar never vanishes; max covers about half the surface
  barWidth = 0.02 + v * 0.48
}

//# min=0 max=1 step=0.01 default=0
export function sliderLineColor(v) {
  lineHue = v
}

//# min=0 max=1 step=0.01 default=0.667
export function sliderBackgroundColor(v) {
  bgHue = v
}

export function beforeRender(delta) {
  // Interval floor keeps the bar moving even at speed 0 (~8 s per crossing);
  // full speed is a sub-second-ish sweep.
  center = time(0.012 + (1 - speed) * 0.11)
}

export function render2D(index, x, y) {
  if (abs(x - center) < barWidth / 2) {
    hsv(lineHue, 1, 1)
  } else {
    hsv(bgHue, 1, 0.1)
  }
}
