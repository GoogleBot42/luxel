// name: firework nova
// Clean-room reimplementation from a prose functional description of the
// community pattern "firework nova"; original source never consulted.

// A thin bright spherical shell erupts from the center of the volume
// roughly once a second and expands outward over black, trailed by random
// pure-white sparks. Hue runs as a rainbow gradient along the cube's main
// diagonal and slowly drifts, so each blast is tinted differently.

const radiusScale = 0.5   // shrink the radius so the wave spans the volume

var t1, t2

export function beforeRender(delta) {
  t1 = time(0.013)   // fast phase, ~0.85 s: blast expansion
  t2 = time(0.08)    // slow phase, ~5 s: hue drift
}

function nova(x, y, z) {
  // recenter on the middle of the unit cube
  x -= 0.5
  y -= 0.5
  z -= 0.5
  var r = hypot3(x, y, z) * radiusScale

  // rainbow along the main diagonal, drifting over time
  var h = (x + y + z) / 3 + t2

  // expanding shell: only the top quarter of the triangle wave survives
  var v = triangle(r - t1) - 0.75

  // trailing sparks: same clipped wave, phase-shifted slightly behind,
  // used as a probability field (up to ~1 in 8 at its peak)
  var s = triangle(r - t1 + 0.04) - 0.75
  if (s > random(2)) {
    rgb(1, 1, 1)
    return
  }

  // renormalize the surviving quarter to 0..1 and cube it: sharpens the
  // shell and (sign-preserving) clips the negative regions to black
  v = v * 4
  v = v * v * v
  hsv(h, 1, v)
}

export function render3D(index, x, y, z) {
  nova(x, y, z)
}

// fallbacks (the original is 3D-only; these treat the missing axes as
// centered so unmapped fixtures still show the blast)
export function render2D(index, x, y) {
  nova(x, y, 0.5)
}

export function render(index) {
  nova(index / pixelCount, 0.5, 0.5)
}
