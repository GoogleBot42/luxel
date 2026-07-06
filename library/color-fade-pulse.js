// name: color fade pulse
// Clean-room reimplementation from a prose functional description of the
// community pattern "color fade pulse"; original source never consulted.

// A scrolling double rainbow with narrow bright pulse peaks drifting over
// it while saturation breathes between vivid color and washed-out white.
// Three free-running sawtooth timers with unrelated periods drive hue
// scroll (fast), the saturation wash (slow, as a full-circle angle), and
// the brightness-pulse motion (medium). Raising the triangle brightness
// wave to the fourth power sharpens broad triangles into narrow spikes.

export function beforeRender(delta) {
  hueT = time(0.012)            // fast hue scroll, ~0.8 s per cycle
  satAngle = time(0.09) * PI2   // slow saturation wave, ~6 s breathing
  pulseT = time(0.025)          // medium pulse drift, ~1.6 s
}

export function render(index) {
  var p = index / pixelCount

  // Two full hue cycles across the strip, scrolling steadily.
  var h = p * 2 - hueT

  // One long spatial saturation wave (half a hue-circle of positional
  // offset), remapped from -1..1 into 0..1, sliding with time.
  var s = (1 + sin(satAngle + p * PI)) / 2

  // ~Four triangular brightness peaks across the strip, moving with time;
  // fourth power turns them into sharp pulses, halved to keep peaks tame.
  var v = triangle(frac(pulseT + p * 4))
  v = v * v
  v = v * v * 0.5

  hsv(h, s, v)
}
