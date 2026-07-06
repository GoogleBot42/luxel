// Spirograph: two stacked rotations trace epicycles into a trail canvas.
// The gear ratio breathes slowly through non-integer values, so the
// rosette never closes — it keeps evolving instead of repeating.
gw = 16
canvas = array(gw * gw)
a1 = 0
a2 = 0

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  feedback(canvas, pow(0.975, delta * 0.06))
  ratio = 2.5 + wave(time(0.37)) * 3
  for (var s = 0; s < 4; s++) {  // substeps keep the trace a line, not dots
    a1 = mod(a1 + dt * 0.9, PI2)
    a2 = mod(a2 + dt * 0.9 * ratio, PI2)
    px = 0.5 + 0.27 * cos(a1) + 0.16 * cos(a2)
    py = 0.5 + 0.27 * sin(a1) + 0.16 * sin(a2)
    canvas[floor(py * 15.99) * gw + floor(px * 15.99)] = 1
  }
  t1 = time(0.09)
}

export function render2D(index, x, y) {
  v = canvas[floor(y * 15.99) * gw + floor(x * 15.99)]
  hsv(t1 + v * 0.1, 1 - v * v * 0.5, v * v)
}
