// name: Coral Plasma
// Clean-room reimplementation from a prose functional description of the
// community pattern "Coral Plasma"; original source never consulted.

// Glowing coral/vein filaments of ridged fractal noise winding through a
// dark volume. Animation comes entirely from moving the sampling frame:
// the unit cube is centered, "breathes" in scale, and rocks through full
// revolutions on all three axes on slightly different half-minute periods.
// Filament cores wash out toward white; dim regions fall to black.

// Breathing zoom and per-axis oscillation periods (seconds, via time()):
var BREATHE = 0.9    // ~59 s
var PX = 0.42        // ~27.5 s
var PY = 0.49        // ~32 s
var PZ = 0.56        // ~36.7 s
var DRIFT = 0.45     // ~29.5 s global hue drift

var zoom = 1
var drift = 0
// Precomputed rotation terms
var sx, cx, sy, cy, sz, cz

export function beforeRender(delta) {
  // Uniform scale breathing between a base zoom and about double it.
  zoom = 1.5 * (1 + triangle(time(BREATHE)))

  // Each axis rocks through a full turn and back: angle = full turn times
  // a slow sine, each axis on its own period so they never sync.
  var ax = PI2 * sin(time(PX) * PI2)
  var ay = PI2 * sin(time(PY) * PI2)
  var az = PI2 * sin(time(PZ) * PI2)
  sx = sin(ax); cx = cos(ax)
  sy = sin(ay); cy = cos(ay)
  sz = sin(az); cz = cos(az)

  drift = time(DRIFT)
}

export function render3D(index, x, y, z) {
  // Center the unit cube, breathe, then rotate about X, Y, Z in turn.
  var px = (x - 0.5) * zoom
  var py = (y - 0.5) * zoom
  var pz = (z - 0.5) * zoom

  var t
  t  = py * cx - pz * sx
  pz = py * sx + pz * cx
  py = t

  t  = px * cy + pz * sy
  pz = pz * cy - px * sy
  px = t

  t  = px * cz - py * sz
  py = px * sz + py * cz
  px = t

  // Dense fine-veined ridge field, squared to sharpen ridges.
  var r = perlinRidge(px, py, pz, 1.3, 0.75, 5)
  r = r * r

  // Hue: banded along the filaments (noise + position), spanning about a
  // third of the wheel, drifting a further fifth, offset into green-violet.
  var h = triangle(r + px + py + pz) * 0.33 + drift * 0.2 + 0.4

  // Filament cores pale toward white.
  var s = 1 - 0.5 * clamp(r, 0, 1)

  // Low-threshold smoothstep, squared: isolates glowing filaments and
  // keeps the background truly dark.
  var v = smoothstep(0.1, 1, r)
  v = v * v

  hsv(h, s, v)
}

// The original needs a 3D map; these wrappers fabricate the missing
// coordinates so the pattern still runs on 2D and 1D installations.
export function render2D(index, x, y) {
  render3D(index, x, y, triangle((x + y) / 2))
}

export function render(index) {
  var p = index / pixelCount
  render3D(index, p, triangle(p * 2), 0.5)
}
