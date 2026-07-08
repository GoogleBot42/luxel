// name: Rainbow Melt
// Curated example (hand-written showcase of the Luxel language/builtins).
// Clean-room reimplementation from a prose description of the community
// pattern "rainbow melt" (no source consulted). A mirrored rainbow that
// slowly rotates while brightness churns in fluid waves — made by nesting
// a sinusoid inside itself, each stage phase-modulated by the clock, over
// two near-but-not-equal periods so it never seems to repeat.
export function beforeRender(delta) {
  t1 = time(0.1)
  t2 = time(0.13)
}

export function render(index) {
  c = 1 - abs(2 * index / pixelCount - 1)  // 1 at center, 0 at ends
  v = wave(wave(wave(c) + t1) + t1)
  hsv(c + t2, 1, v * v)
}
