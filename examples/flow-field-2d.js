// Streamlines of a drifting simplex field: sample the noise, read it as
// a heading, step along it. Trails on a 16×16 virtual canvas remember a
// hue per cell — streams stay colored by the direction they flowed.
gw = 16
n = 20
px = array(n)
py = array(n)
canvas = array(gw * gw)
hues = array(gw * gw)
z = 0

for (i = 0; i < n; i++) {
  px[i] = random(1)
  py[i] = random(1)
}

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  z += dt * 0.045  // the field itself slowly morphs
  if (z > 512) z -= 512
  feedback(canvas, pow(0.9, delta * 0.06))
  for (var i = 0; i < n; i++) {
    a = simplex3(px[i] * 1.6, py[i] * 1.6, z, 3) * PI2
    px[i] += cos(a) * 0.22 * dt
    py[i] += sin(a) * 0.22 * dt
    if (px[i] < 0 || px[i] >= 1 || py[i] < 0 || py[i] >= 1) {
      px[i] = random(0.999)
      py[i] = random(0.999)
    } else {
      idx = floor(py[i] * 15.99) * gw + floor(px[i] * 15.99)
      canvas[idx] = 1
      hues[idx] = a / PI2  // heading = hue (wraps naturally)
    }
  }
  t1 = time(0.2)
}

export function render2D(index, x, y) {
  idx = floor(y * 15.99) * gw + floor(x * 15.99)
  v = canvas[idx]
  hsv(hues[idx] + t1, 1 - v * v * 0.4, v * v)
}
