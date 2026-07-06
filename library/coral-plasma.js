// name: Coral Plasma
// Clean-room reimplementation from a prose functional description of the
// community pattern "Coral Plasma"; original source never consulted.

// Slowly tumbling organic plasma: bright coral-branch / vein filaments of
// ridged fractal noise wind through the volume against a dark background.
// All animation moves the sampling frame, never the field: per frame the
// unit cube is centered, breathes (triangle-wave zoom, ~1 min period) and
// rocks through full revolutions on all three axes (sine-oscillating
// angles on deliberately unequal ~half-minute periods). Ridge cores wash
// toward white; hue bands follow the filaments and drift over tens of
// seconds. The original is 3D-only; 2D/1D wrappers fabricate the missing
// coordinates as the spec suggests.

// rotation matrix + zoom, rebuilt each frame
var m00 = 1, m01 = 0, m02 = 0
var m10 = 0, m11 = 1, m12 = 0
var m20 = 0, m21 = 0, m22 = 1
var zoom = 2
var huePhase = 0

export function beforeRender(delta) {
  // breathing zoom: base to about double, ~59 s triangle
  zoom = mix(1.6, 3.2, triangle(time(0.9)))

  // rocking rotations: a full turn scaled by a sine oscillation,
  // each axis on its own period (~27 s / ~34 s / ~23 s) so they never sync
  var ax = PI2 * sin(time(0.42) * PI2)
  var ay = PI2 * sin(time(0.52) * PI2)
  var az = PI2 * sin(time(0.35) * PI2)

  var ca = cos(ax), sa = sin(ax)
  var cb = cos(ay), sb = sin(ay)
  var cc = cos(az), sc = sin(az)

  // R = Rz(az) * Ry(ay) * Rx(ax)
  m00 = cc * cb
  m01 = cc * sb * sa - sc * ca
  m02 = cc * sb * ca + sc * sa
  m10 = sc * cb
  m11 = sc * sb * sa + cc * ca
  m12 = sc * sb * ca - cc * sa
  m20 = -sb
  m21 = cb * sa
  m22 = cb * ca

  // slow global hue drift, ~33 s cycle
  huePhase = time(0.5)
}

function shade(x, y, z) {
  // center, breathe, rotate — the moving sampling frame
  var px = (x - 0.5) * zoom
  var py = (y - 0.5) * zoom
  var pz = (z - 0.5) * zoom
  var tx = m00 * px + m01 * py + m02 * pz
  var ty = m10 * px + m11 * py + m12 * pz
  var tz = m20 * px + m21 * py + m22 * pz

  // dense fine-veined ridge field, squared to sharpen ridges
  var r = perlinRidge(tx, ty, tz, 1.3, 0.75, 1, 5)
  r = r * r

  // spatial hue banding (~1/3 of the wheel) following the filaments,
  // plus slow global drift (~1/5 of the wheel) and a constant offset
  // parking hues in greens/teals through blues into violets
  var hue = 0.45 + triangle(r + tx + ty + tz) * 0.33 + huePhase * 0.2

  // filament cores pale toward white
  var sat = 1 - 0.5 * clamp(r, 0, 1)

  // low-threshold smoothstep, squared: dark background, glowing veins
  var v = smoothstep(0.1, 1, r)
  v = v * v

  hsv(hue, sat, v)
}

export function render3D(index, x, y, z) {
  shade(x, y, z)
}

export function render2D(index, x, y) {
  shade(x, y, 0.5)
}

export function render(index) {
  shade(index / pixelCount, 0.5, 0.5)
}
