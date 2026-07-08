// name: 3D Rotation / Spotlights
// Clean-room reimplementation from a prose functional description of the
// community pattern "3D Rotation / Spotlights"; original source never
// consulted.

// A 3D double cone (hourglass, apex at the volume center, opening along one
// axis both ways) tumbling continuously around a wandering rotation axis.
// On a walled cube it reads as two magenta-rimmed spotlight discs with hot
// white centers gliding across the faces. Signed distance field of the
// double cone, rotated by a per-frame axis-angle matrix, shaded by that
// distance: saturation linear, brightness a steep power curve.

const SPEED = 1                 // divides all four clock periods
const SCALE = 0.10132           // ~1/PI^2: how wide the cones open

// Rotation matrix, rebuilt once per frame (never per pixel).
var m00 = 1, m01 = 0, m02 = 0
var m10 = 0, m11 = 1, m12 = 0
var m20 = 0, m21 = 0, m22 = 1

export function beforeRender(delta) {
  // Three triangle waves (slightly different ~2-3 s periods) form the axis;
  // differing periods make the axis wander without repeating quickly.
  var ax = 2 * triangle(time(0.031 / SPEED)) - 1
  var ay = 2 * triangle(time(0.041 / SPEED)) - 1
  var az = 2 * triangle(time(0.052 / SPEED)) - 1

  // Normalize the axis to unit length (guard the all-zero instant).
  var len = sqrt(ax * ax + ay * ay + az * az)
  if (len < 0.001) { ax = 0; ay = 0; az = 1; len = 1 }
  ax /= len; ay /= len; az /= len

  // A faster sawtooth (~1.1 s) scaled to a full turn is the rotation angle.
  var ang = time(0.017 / SPEED) * PI2
  var c = cos(ang)
  var s = sin(ang)
  var C = 1 - c

  // Classic Rodrigues axis-angle rotation matrix.
  m00 = c + ax * ax * C
  m01 = ax * ay * C - az * s
  m02 = ax * az * C + ay * s
  m10 = ay * ax * C + az * s
  m11 = c + ay * ay * C
  m12 = ay * az * C - ax * s
  m20 = az * ax * C - ay * s
  m21 = az * ay * C + ax * s
  m22 = c + az * az * C
}

export function render3D(index, x, y, z) {
  // Shift so the origin is the center of the mapped volume.
  var px = x - 0.5
  var py = y - 0.5
  var pz = z - 0.5

  // Rotate the position by this frame's matrix.
  var rx = m00 * px + m01 * py + m02 * pz
  var ry = m10 * px + m11 * py + m12 * pz
  var rz = m20 * px + m21 * py + m22 * pz

  // Signed inside-ness of the double cone (axis = rx). Positive is inside;
  // magnitude ~ distance from the cone surface.
  var radial = sqrt((ry / SCALE) * (ry / SCALE) + (rz / SCALE) * (rz / SCALE))
  var d = abs(rx) - radial
  d = clamp(d, -1, 1)

  // Fixed magenta hue; saturation washes to white deep inside; brightness
  // is a steep power curve giving a soft antialiased rim, black well outside.
  var bri = (1 + d)
  bri = bri * bri
  bri = bri * bri                 // (1 + d) ^ 4
  hsv(0.83, 1 - d, bri)
}
