// name: color fade pulse
// Clean-room reimplementation from a prose functional description of the
// community pattern "color fade pulse"; original source never consulted.

// Three free-running sawtooth timers with unrelated periods: a fast hue
// scroll, a slow saturation wash (as a full-circle angle), and a medium
// brightness-pulse drift. Everything else is stateless per-pixel math.

export function beforeRender(delta) {
  hueT = time(0.01)            // ~0.66 s: fast rainbow scroll
  satT = time(0.08) * PI2      // ~5.2 s: slow saturation wave, as an angle
  pulseT = time(0.025)         // ~1.6 s: pulse-peak drift
}

export function render(index) {
  var p = index / pixelCount   // normalized position, layout-proportional

  // Two full hue cycles across the strip, scrolling steadily.
  var h = 2 * p - hueT

  // One long spatial saturation wave (half a hue-circle of offset across
  // the strip) sliding with time: vivid <-> washed-out near-white.
  var s = (1 + sin(satT + p * PI)) / 2

  // ~4 triangular brightness peaks drifting along the strip; the 4th
  // power sharpens them into narrow spikes with long dark valleys, and
  // the halving keeps the peaks moderate.
  var tri = triangle(pulseT + 4 * p)
  var v = tri * tri * tri * tri * 0.5

  hsv(h, s, v)
}
