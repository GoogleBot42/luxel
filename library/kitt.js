// name: KITT
// Curated example (hand-written showcase of the Luxel language/builtins).
// Classic scanner with a decaying trail.
leds = array(pixelCount)

export function beforeRender(delta) {
  for (var i = 0; i < pixelCount; i++) leds[i] *= 0.92
  pos = floor(triangle(time(.05)) * (pixelCount - 1))
  leds[pos] = 1
}

export function render(index) {
  v = leds[index]
  rgb(v * v * 1.2, v * v * v * 0.2, 0)
}
