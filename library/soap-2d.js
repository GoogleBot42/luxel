// name: Soap 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Domain warping (WLED "Soap"): one noise field pushes the coordinates
// around before a second one is sampled through them. The double
// indirection is what makes it smear and swirl instead of just drift.
z = 0

export function beforeRender(delta) {
  z = (z + delta * 0.00008) % 512
  hueT = time(0.23)
}

export function render2D(index, x, y) {
  wx = simplex3(x * 1.3, y * 1.3, z, 11) * 0.5
  wy = simplex3(x * 1.3 + 5, y * 1.3 + 5, z, 12) * 0.5
  n = simplex3((x + wx) * 2, (y + wy) * 2, z * 1.7, 13)
  hsv(hueT + n * 0.18, 0.75, saturate(0.55 + n * 0.7))
}
