// name: Example: Smooth Speed Slider
// Clean-room reimplementation from a prose functional description of the
// community pattern "Example: Smooth Speed Slider"; original source never
// consulted.

// A scrolling rainbow whose speed slider changes the scroll rate with no jump
// or skip. The phase is accumulated manually (rate x elapsed-time) rather than
// derived from the shared clock, so only the *rate* changes when the slider
// moves -- the phase never leaps.

var phase = 0          // 0..1, accumulated manually
var speed = 0.25       // squared slider value, drives phase-per-ms

// scale so slider at max (speed==1) is about one full cycle per second:
// 1 cycle / 1000 ms = 0.001 phase per ms
var SCALE = 0.001

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  speed = v * v        // squared response: fine control at the slow end
}

export function beforeRender(delta) {
  phase = frac(phase + delta * speed * SCALE)   // manual accumulation, wraps in 0..1
}

export function render(index) {
  hsv(phase + index / pixelCount, 1, 1)
}
