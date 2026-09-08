// name: Flow Field 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Streamlines of a drifting simplex field: sample the noise, read it as
// a heading, step along it. Trails on a 16×16 virtual canvas remember a
// hue per cell — streams stay colored by the direction they flowed.
gw = 16
n = 20
px = array(n)
py = array(n)
canvas = array(gw * gw)
hues = array(gw * gw)
// Per-cell read-out channels for the whole-frame fill (Gitea #405). 256
// cells each, so they cost nothing against the array budget.
hC = array(gw * gw)
sC = array(gw * gw)
vC = array(gw * gw)
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

  // Resolve the cell colours once per CELL instead of once per LED: on a
  // 64x64 panel that is 256 answers instead of 4096 VM entries.
  for (var k = 0; k < gw * gw; k++) {
    var vv = canvas[k] * canvas[k]
    hC[k] = hues[k] + t1
    sC[k] = 1 - vv * 0.4
    vC[k] = vv
  }
}

export function renderFrame() {
  fillCanvas(hC, sC, vC, gw, gw)
}
