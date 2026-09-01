// name: Upward waves 3D using accelerometer
// Clean-room reimplementation from a prose functional description of the
// community pattern "Upward waves 3D using accelerometer"; original source
// never consulted. On a mapped LED cube, rainbow rings of light climb
// against real-world gravity: the smoothed accelerometer vector is turned
// into two rotations pushed onto the coordinate transform each frame, so
// the bands always rise "up" no matter how the fixture is held. Shaking
// (higher g-force) flares the bands brighter and wider.

// Sensor bindings (engine stubs these with zeros with no sensor board)
export var accelerometer = array(3)

// --- build-specific config ---
var SMOOTH = 0.72             // IIR low-pass factor for orientation
var RESTG = 0.08              // sensor magnitude at 1 g after remap
var IDLE_G = 1.0              // gentle brightness when no sensor present

// --- watched debug outputs ---
export var gForce = 0
export var polar = 0
export var azimuth = 0
export var fax = 0
export var fay = 0
export var faz = 0

var inited = 0
var phase = 0

export function beforeRender(delta) {
  // Build-specific axis remap would go here; identity for this port.
  var sx = accelerometer[0]
  var sy = accelerometer[1]
  var sz = accelerometer[2]

  var mag = hypot3(sx, sy, sz)
  if (mag < 0.001) {
    // No live sensor: idle gently with gravity pointing straight down.
    sx = 0; sy = 0; sz = RESTG
    gForce = IDLE_G
  } else {
    gForce = mag / RESTG
  }

  if (!inited) { fax = sx; fay = sy; faz = sz; inited = 1 }
  fax = fax * SMOOTH + sx * (1 - SMOOTH)
  fay = fay * SMOOTH + sy * (1 - SMOOTH)
  faz = faz * SMOOTH + sz * (1 - SMOOTH)

  // Spherical angles of the gravity direction (atan2 handles the axis cases).
  // polar = tilt away from the map's z axis; azimuth un-rotates the gravity
  // vector's compass bearing. rotateZ(azimuth) then rotateX(polar) re-levels
  // the map so the bands climb along gravity. (The pair is a quarter turn
  // off a strict "lay gravity on +z" alignment — the lean lands on the
  // orthogonal horizontal axis — which is the original's map convention.)
  polar = atan2(hypot(fax, fay), faz)
  azimuth = 0 - atan2(fay, fax)

  // sawtooth band phase: a full rise well under a second
  phase = time(0.01)

  // Re-level the map so its "up" axis follows true gravity, then the
  // per-pixel code stays trivial.
  resetTransform()
  translate3D(-0.5, -0.5, -0.5)
  rotateZ(azimuth)
  rotateX(polar)
}

export function render3D(index, x, y, z) {
  // hue: radial distance from the vertical axis, cool at the core
  var r = hypot(x, y)
  var hue = r * 1.5 + 0.5

  // brightness: a triangle wave in the vertical coordinate, clipped to
  // its upper half so each period leaves one bright band with dark gaps
  var tw = triangle(z + phase)
  var band = max(0, (tw - 0.5) * 2)
  var v = band * gForce
  v = clamp(v, 0, 1)
  v = v * v * v * v            // sharpen into a crisp, narrow stripe

  hsv(hue, 1, v)
}
