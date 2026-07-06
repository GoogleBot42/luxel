// name: sinpulse 3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "sinpulse 3D"; original source never consulted.

// Classic rainbow plasma: two drifting phases interfere across x/y/z while
// a slow triangle "zoom" breathes the blob scale between coarse and fine.
// Brightness is the field cubed, so dim valleys separate glowing crests.

var p1 = 0    // first phase angle (full circle sawtooth, ~3 s)
var p2 = 0    // second phase angle (~7 s; ~2x period ratio, never repeats)
var zoom = 1  // spatial scale, breathing between ~1 and ~4 over ~10 s

export function beforeRender(delta) {
  p1 = time(0.047) * PI2
  p2 = time(0.101) * PI2
  zoom = 1 + 3 * triangle(time(0.15))
}

export function render3D(index, x, y, z) {
  var v = (1 + sin(x * zoom + p1) + cos(y * zoom + p2) + sin(z * zoom + p1 - p2)) / 2
  hsv(v, 1, v * v * v / 2)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}

export function render(index) {
  render3D(index, index / pixelCount, 0, 0)
}
