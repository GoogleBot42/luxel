// name: Solid Rainbow
// Clean-room reimplementation from a prose functional description of the
// community pattern "Solid Rainbow"; original source never consulted.

// The whole strip shows one hue at a time, cycling around the color wheel.
// Brightness ramps linearly along the strip (dark at pixel 0, full at the
// end), so it reads as a single-color gradient whose color rotates through
// the rainbow. Frame-rate independent: the hue phase accumulates real
// elapsed time.

var speed = 0.5
var phase = 0

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  speed = v
}

export function beforeRender(delta) {
  // Squaring the slider gives fine control at the slow end; at speed = 1
  // the hue completes roughly one full cycle per second. At exactly zero
  // the phase freezes.
  phase = mod(phase + delta * 0.001 * speed * speed, 1)
}

export function render(index) {
  hsv(phase, 1, index / pixelCount)
}
