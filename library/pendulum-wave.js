// name: Pendulum Wave
// Curated example (hand-written showcase of the Luxel language/builtins).
// The physics-classroom classic: one pendulum per pixel, periods
// graduated so the row drifts from unison into traveling waves, apparent
// chaos, and back to unison. tSec wraps exactly at the realignment
// period — every pendulum completes a whole number of swings — so the
// cycle is seamless.
var cycle = 24  // seconds until the pendulums realign
tSec = 0

export function beforeRender(delta) {
  tSec = (tSec + delta * 0.001) % cycle
}

export function render(index) {
  // pendulum i completes (16 + i) full swings per cycle
  d = sin(PI2 * (16 + index) * tSec / cycle)
  hsv(0.72 + d * 0.14, 0.9, d * d)
}
