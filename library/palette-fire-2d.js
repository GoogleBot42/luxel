// name: Palette Fire 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// setPalette / paint with fbm noise driving a fire look.
setPalette([
  0.0,  0,    0,    0,
  0.3,  0.6,  0,    0,
  0.6,  1,    0.4,  0,
  0.85, 1,    0.9,  0.1,
  1.0,  1,    1,    0.8
])

export function render2D(index, x, y) {
  heat = perlinFbm(x * 3, y * 3 - time(.03) * 8, 0, 2, 0.5, 3)
  paint(clamp(heat + 0.9 - y * 1.4, 0, 1))
}
