// name: rainbow pinwheel
// Clean-room reimplementation from a prose functional description of the
// community pattern "rainbow pinwheel"; original source never consulted.
//
// A folded, scrolling rainbow: hue is a sinusoid of (time + position), so
// colors sweep up and back down the strip instead of hard-wrapping. On a
// radial layout it reads as a spinning pinwheel. Saturation is overdriven
// well past 1 (it clamps) to guarantee fully saturated color.

var t1 = 0

export function beforeRender(delta) {
  t1 = time(0.05)  // ~3.3 s per full cycle
}

export function render(index) {
  var h = wave(t1 + index / pixelCount)
  hsv(h, 2, 1)
}
