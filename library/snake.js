// name: snake
// Clean-room reimplementation from a prose functional description of the
// community pattern "snake"; original source never consulted.
// A bright head chases along a static, fully-saturated rainbow gradient with
// a short linear tail fading to black behind it, wrapping seamlessly.

var lapInterval = 0.1   // time() interval -> several seconds per lap
//# min=0 max=1 step=0.01 default=0.1
export function sliderSpeed(v) {
  lapInterval = max(0.02, v * 0.4)
}

var tailFrac = 0.15     // tail length as a fraction of the strip
//# min=0 max=1 step=0.01 default=0.15
export function sliderTailLength(v) {
  tailFrac = clamp(v, 0.02, 1)
}

var head = 0

export function beforeRender(delta) {
  head = time(lapInterval)   // 0..1 normalized head position
}

export function render(index) {
  var p = index / pixelCount
  // distance this pixel sits behind the head, wrapping the strip
  var d = head - p
  if (d < 0) d += 1
  var b = 1 - d / tailFrac   // head brightest, linear ramp to zero over tail
  hsv(p, 1, clamp(b, 0, 1))
}
