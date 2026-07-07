// name: Solid Rainbow
// Clean-room reimplementation from a prose functional description of the
// community pattern "Solid Rainbow"; original source never consulted.

// One hue at a time across the whole strip, cycling smoothly around the
// color wheel. Brightness ramps linearly from dark at the first pixel to
// full at the last, so it reads as a single-color gradient whose color
// rotates through the rainbow. Frame-rate independent: the hue phase
// accumulates delta time scaled by the (squared) speed setting.

var phase = 0        // shared hue phase, 0..1
var speed = 0.5      // slider value, squared before use

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  // squared for fine control at the slow end; v = 1 is ~one cycle/second
  speed = v
}

export function beforeRender(delta) {
  phase = (phase + delta * 0.001 * speed * speed) % 1
}

export function render(index) {
  hsv(phase, 1, index / (pixelCount - 1))
}
