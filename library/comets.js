// name: Comets
// Curated example (hand-written showcase of the Luxel language/builtins).
// Comets with glowing trails. The whole trail idiom is two builtin calls:
// feedback() decays the buffer, blur1D() softens it into a glow. Heads are
// deposited after the blur so they stay crisp.
leds = array(pixelCount)

export var speed = 0.5
export function sliderSpeed(v) { speed = v } //# min=0 max=1 step=0.01 default=0.5

export function beforeRender(delta) {
  feedback(leds, pow(0.92, delta * 0.06))  // frame-rate-independent decay
  blur1D(leds, 1)
  for (var c = 0; c < 3; c++) {
    // three comets bouncing at slightly different rates, staggered
    p = triangle(time(0.03 + c * 0.011) * (0.5 + speed) + c / 3)
    leds[floor(p * (pixelCount - 1))] = 1
  }
  t1 = time(0.1)
}

export function render(index) {
  v = leds[index]
  // bright heads desaturate toward white; tails stay vivid
  hsv(t1 + index / pixelCount * 0.3, 1 - v * v * 0.7, v * v)
}
