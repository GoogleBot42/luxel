// name: angle and radius from coordinates
// Clean-room reimplementation from a prose functional description of the
// community pattern "angle and radius from coordinates"; original source never
// consulted.

// A teaching pattern for deriving polar coordinates from mapped positions. A
// narrow bright radar spoke sweeps around the map center (a revolution every few
// seconds). Hue is tied to each pixel's distance from the 3D center, so the beam
// sweeps concentric color rings. On a true 3D map the rotation rate grows with
// height, shearing the spoke into a slowly twisting helix. Pure function of
// position and time — no state beyond the ambient clock.

var tsec = 0   // accumulated seconds

export function beforeRender(delta) {
  tsec += delta / 1000
}

// Unit angle of a pixel about the map center: recenter, take the four-quadrant
// arctangent, shift positive, normalize a full turn to 0..1 (zero at "north").
function unitAngle(x, y) {
  var a = atan2(y - 0.5, x - 0.5)   // -PI..PI
  a = a / PI2                        // -0.5..0.5 turns
  if (a < 0) a += 1                  // 0..1, always positive
  return a
}

// 3D Euclidean radius from the recentered midpoint.
function radius3D(x, y, z) {
  return hypot3(x - 0.5, y - 0.5, z - 0.5)
}

export function render3D(index, x, y, z) {
  // Rotation phase increases steadily; its period grows with height (base a few
  // seconds per revolution, roughly doubling from one end of z to the other).
  var period = 3 * (1 + z)                 // seconds per revolution
  var phase = tsec / period                // turns (clockwise sweep)
  var a = unitAngle(x, y) + phase

  // Triangle wave -> brightness ramp, raised to a high power for a thin spoke.
  var v = triangle(a)
  v = v * v; v = v * v                     // ^4
  v = v * v                                // ^8
  v = v * triangle(a) * triangle(a)        // ^10

  var hue = radius3D(x, y, z)              // red at center, walks the wheel out
  hsv(hue, 1, v)
}

// Flat maps: delegate to the 3D renderer at mid-height.
export function render2D(index, x, y) {
  render3D(index, x, y, 0.5)
}
