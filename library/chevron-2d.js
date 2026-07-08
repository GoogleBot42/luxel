// name: Chevron 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// The smallest possible render2D (a QMK keyboard classic): abs() folds
// the gradient into a V, time() scrolls it. Whole pattern, one line.
export function render2D(index, x, y) {
  hsv(time(0.06) + (x - abs(y - 0.5)) * 0.7, 1, 1)
}
