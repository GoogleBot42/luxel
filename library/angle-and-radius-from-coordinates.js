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

var sweepSec = 3     // seconds per revolution at the bottom of the map (z=0);
                     // a 3D map stretches this to 2x at the top, and a flat map
                     // renders at mid-height, i.e. 1.5x this value per lap
var widthK = 1       // beam narrowing: 1 = the reference 24 degree wide spoke
var colorCycles = 1  // hue cycles between the center and the far corner
var dir = 1          // +1 = the default sweep direction, -1 = reversed

//# min=0.5 max=30 step=0.5 default=3
export function sliderSweepSeconds(v) { sweepSec = max(0.1, v) }

// Beam width is the angular full-width at half brightness. The spoke is a
// triangle ramp raised to the 10th power, which is ~24 degrees wide; narrowing
// the ramp by 24/width scales that width directly.
//# min=2 max=120 step=1 default=24
export function sliderBeamWidth(v) { widthK = 24 / max(2, v) }

//# min=0.25 max=4 step=0.25 default=1
export function sliderColorCycles(v) { colorCycles = max(0, v) }

//# min=0 max=1 step=1 default=0
export function toggleReverse(on) { dir = on > 0.5 ? -1 : 1 }

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
  var period = sweepSec * (1 + z)          // seconds per revolution
  var phase = dir * tsec / period          // turns (clockwise sweep)
  var a = unitAngle(x, y) + phase

  // Triangle wave -> brightness ramp, raised to a high power for a thin spoke.
  // Steepening the ramp first (widthK > 1) narrows the spoke proportionally.
  var t = saturate(1 - (1 - triangle(a)) * widthK)
  var v = t * t; v = v * v                 // ^4
  v = v * v                                // ^8
  v = v * t * t                            // ^10

  var hue = radius3D(x, y, z) * colorCycles // red at center, walks the wheel out
  hsv(hue, 1, v)
}

// Flat maps: delegate to the 3D renderer at mid-height.
export function render2D(index, x, y) {
  render3D(index, x, y, 0.5)
}
