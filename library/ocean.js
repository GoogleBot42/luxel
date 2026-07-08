// name: Ocean
// Curated example (hand-written showcase of the Luxel language/builtins).
// An ocean in the spirit of FastLED's Pacifica (original construction):
// layered waves scrolling at different speeds and scales, summed through
// a deep palette. Whitecaps break where the layers crest together.
// Integer time multipliers keep every layer seamless when time() wraps.
setPalette([
  0.0,  0,    0.02, 0.10,
  0.45, 0,    0.10, 0.35,
  0.75, 0,    0.35, 0.45,
  0.92, 0.25, 0.60, 0.55,
  1.0,  0.85, 0.95, 0.90
])

export function beforeRender(delta) {
  t1 = time(0.11)
  t2 = time(0.07)
  t3 = time(0.05)
  swell = 0.7 + 0.3 * wave(time(0.19))  // slow set-wave rolling through
}

export function render(index) {
  x = index / pixelCount
  v = 0.35 * wave(x * 3 + t1 * 2)
  v += 0.30 * wave(x * 5 + t2 * 3)
  v += 0.20 * wave(x * 8 - t3 * 4)  // one counter-current layer for chop
  v *= swell
  foam = saturate(v - 0.68) * 4     // crests break past the threshold
  paint(saturate(v + foam), 0.25 + v * 0.75 + foam)
}
