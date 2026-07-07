// name: 1 White Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "1 White Fade"; original source never consulted.

// Every pixel breathes together in plain white: one slow time ramp shaped
// by wave() into a smooth 0..1..0 sinusoid, roughly half a minute per
// breath. No state, no randomness, no layout assumptions.

var glow = 0

export function beforeRender(delta) {
  glow = wave(time(0.5))     // 0.5 * 65.536 s = ~33 s per full breathe
}

export function render(index) {
  hsv(0, 0, glow)            // saturation 0 => white; hue irrelevant
}
