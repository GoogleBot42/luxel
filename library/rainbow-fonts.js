// name: rainbow fonts
// Clean-room reimplementation from a prose functional description of the
// community pattern "rainbow fonts"; original source never consulted.

// Mirror-symmetric rainbow: hue comes from a folded distance-from-center
// passed through a sine fold twice (once before, once after adding the
// animation phase), so the color bands compress and stretch as they sweep.

var phase = 0

export function beforeRender(delta) {
  phase = time(0.1)   // ~6.5 s cycle — relaxed pace
}

export function render(index) {
  var half = pixelCount / 2
  var c = 1 - abs(index - half) / half   // 1 at midpoint, 0 at both ends
  var h = wave(wave(c) + phase)          // double sine-fold + sliding phase
  hsv(h, 1, 0.35)                        // full saturation, modest brightness
}
