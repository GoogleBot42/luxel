// Polar coordinates from scratch: atan2 + hypot turn (x, y) into angle
// and radius, and one wave() of both makes rotating spiral arms. Integer
// arm counts keep the wave continuous around the seam.
export var arms = 3
export function sliderArms(v) { arms = 1 + floor(v * 5.99) } //# min=1 max=6 step=1 default=3

export function beforeRender(delta) {
  t1 = time(0.05)
  t2 = time(0.13)
}

export function render2D(index, x, y) {
  dx = x - 0.5
  dy = y - 0.5
  a = atan2(dy, dx) / PI2  // turns, -0.5..0.5
  r = hypot(dx, dy) * 1.4
  v = wave(a * arms - t1 * 2 + r * 2.5)
  v = v * v * v
  hsv(0.7 + r * 0.4 + t2, 1 - v * 0.4, v * saturate(1.3 - r * 1.6))
}
