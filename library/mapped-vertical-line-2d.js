// name: mapped vertical line 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "mapped vertical line 2D"; original source never consulted.

// A single solid vertical bar sweeps left-to-right across the mapped
// surface (sawtooth: it snaps back to the left edge, no bounce), clipped at
// the edges rather than wrapping. Behind it sits a dim fully-saturated
// backdrop. Bar hue, backdrop hue, speed, and width are all sliders; speed
// and width both have small floors so the bar never stalls or vanishes.

var speed = 0.5                      // raw slider value
var barWidth = 0.02 + 0.5 * 0.48     // fraction of surface width
var lineHue = 0
var bgHue = 0.6667
var pos = 0                          // bar center x, 0..1 sawtooth

//# min=0 max=1 step=0.01 default=0.5
export function sliderLineSpeed(v) {
  speed = v
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderLineWidth(v) {
  barWidth = 0.02 + v * 0.48         // floor so it never vanishes; max ~half
}

//# min=0 max=1 step=0.01 default=0
export function sliderLineColor(v) {
  lineHue = v
}

//# min=0 max=1 step=0.01 default=0.6667
export function sliderBackgroundColor(v) {
  bgHue = v
}

export function beforeRender(delta) {
  // Crossings per second: 0.08 at slider zero (never stalls) up to ~1.3.
  pos += delta * (0.08 + speed * 1.2) / 1000
  pos = pos % 1
}

export function render2D(index, x, y) {
  if (abs(x - pos) <= barWidth / 2) {
    hsv(lineHue, 1, 1)               // the bar: full sat, full brightness
  } else {
    hsv(bgHue, 1, 0.1)               // dim saturated backdrop
  }
}
