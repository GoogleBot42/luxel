// name: Spinning Plasma 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// 2D showcase: perlin noise sampled through a rotating transform.
// The //# comment bounds the slider (Luxel extension; PB ignores it).
export var zoom = 0.45
export function sliderZoom(v) { zoom = v } //# min=0.1 max=1.5 step=0.01 default=0.45

export function beforeRender(delta) {
  t1 = time(.05)
  resetTransform()
  translate(-0.5, -0.5)
  rotate(t1 * PI2)
}

export function render2D(index, x, y) {
  n = perlin(x * 4 * zoom + 10, y * 4 * zoom + 10, time(.1) * 4, 7)
  hsv(0.6 + n * 0.5, 1, clamp(n + 0.6, 0, 1))
}
