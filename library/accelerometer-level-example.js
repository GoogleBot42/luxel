// name: Accelerometer level example
// Clean-room reimplementation from a prose functional description of the
// community pattern "Accelerometer level example"; original source never
// consulted. A rainbow horizontal bar through the panel center that
// counter-rotates against accelerometer tilt to stay level with gravity.

export var accelerometer = array(3)   // [x, y, z]; engine stubs to zeros

var angle = 0          // smoothed correction angle (radians), persists

// Flip sign of the correction depending on sensor mounting orientation.
var flip = 1
export function toggleFlip(v) { flip = v ? -1 : 1 }

export function beforeRender(delta) {
  var ax = accelerometer[0]
  var ay = accelerometer[1]

  // Guard all-zero (no sensor / idle): target angle 0 -> bar sits level.
  var target = 0
  if (ax != 0 || ay != 0) {
    target = flip * -atan2(ay, ax)
  }

  // Low-pass filter: blend a couple percent toward the measurement per frame.
  angle += (target - angle) * 0.03

  // Rebuild transform each frame: origin to center, then rotate.
  // translate by -half so 0..1 coords become centered (~ -0.5 .. +0.5).
  resetTransform()
  translate(-0.5, -0.5)
  rotate(angle)
}

export function render2D(index, x, y) {
  // After the transform x,y are centered (~ -0.5 .. +0.5) and rotated.
  var b = 1 - abs(y) * 6
  b = clamp(b, 0, 1)
  b = b * b
  // Hue undoes the centering so it runs the full wheel across the bar length.
  hsv(x + 0.5, 1, b)
}

// Read-only gauge: full when level, lower as tilt grows.
export function gaugeLevel() {
  return 1 - abs(angle) / PI
}
