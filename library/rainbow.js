// name: Rainbow
// Curated example (hand-written showcase of the Luxel language/builtins).
// The canonical default pattern: a moving rainbow.
export function render(index) {
  hsv(time(.1) + index / pixelCount, 1, 1)
}
