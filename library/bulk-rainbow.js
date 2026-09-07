// name: Bulk Rainbow
// Curated example (hand-written showcase of the Luxel language/builtins).
// The whole-frame entry in its smallest useful form: `renderFrame()` runs
// ONCE per frame instead of `render()` per pixel, and one `fillGradient`
// paints the entire strip. Hue is lerped unwrapped and `hsv()` wraps it, so
// a span of `cycles` turns is exactly `cycles` rainbows across the fixture.
// Nothing else in the frame is per-pixel, so the interpreter cost is flat no
// matter how many pixels the rig has.

var speed = 1
var cycles = 1
var phase = 0

export function beforeRender(delta) {
  // Own phase accumulator rather than time(): the Speed slider can then run
  // to a standstill (and the direction stays stable when it is moved).
  phase = mod(phase + delta * 0.0002 * speed, 1)
}

export function renderFrame() {
  fillGradient(phase, 1, 1, phase + cycles, 1, 1)
}

//# min=0 max=4 step=0.05 default=1
export function sliderSpeed(v) { speed = v }

//# min=0.25 max=4 step=0.25 default=1
export function sliderCycles(v) { cycles = v }
