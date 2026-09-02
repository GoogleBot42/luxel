// name: Ember Diffusion
// Curated example (hand-written showcase of the Luxel language/builtins).
// Heat spreads along the strip through the engine's per-pixel state buffer:
// each pixel reads its neighbours' PREVIOUS-frame heat — pixelState is
// double-buffered, so every read in a frame sees the same snapshot no matter
// which pixels have already rendered — cools a little, and writes the new
// value back with setPixelState. No arrays, no swap bookkeeping, and the
// buffer only exists because this pattern writes it.
export var sparkRate = 0.01
export var cooling = 0.97

export function sliderSparks(v) { sparkRate = v * v * 0.1 }
export function sliderCooling(v) { cooling = 0.9 + v * 0.099 }

export function render(index) {
  // out-of-range taps read 0, so the ends bleed heat away by themselves
  h = (pixelState(index - 1) + 2 * pixelState(index) + pixelState(index + 1)) / 4
  h *= cooling
  if (random(1) < sparkRate) h = 1
  setPixelState(index, h)
  hsv(0.02 + h * 0.06, 1 - h * h * 0.6, h * h)
}
