// Aurora curtains: simplex2 draws the slowly-waving band, simplex3 adds
// vertical shimmer rays, and a palette paints black → green → violet.
// Simplex is smoother than perlin here — no axis-aligned artifacts.
setPalette([
  0.0,  0,    0,    0,
  0.25, 0,    0.07, 0.03,
  0.55, 0,    0.55, 0.18,
  0.8,  0.15, 0.95, 0.5,
  1.0,  0.75, 0.45, 0.95
])

z = 0

export function beforeRender(delta) {
  z = (z + delta * 0.00015) % 1024  // slow drift along one noise axis
}

export function render2D(index, x, y) {
  band = 0.45 + simplex2(x * 1.8, z, 5) * 0.25
  glow = saturate(1 - abs(y - band) * 2)
  shimmer = 0.6 + 0.4 * simplex3(x * 6, y * 2, z * 4, 9)
  v = saturate(glow * shimmer * 1.4)
  paint(v, v * v)
}
