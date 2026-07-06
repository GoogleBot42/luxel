// name: RGB-XYZ 3D Sweep
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGB-XYZ 3D Sweep"; original source never consulted.

// 3D-map diagnostic: a glowing planar band sweeps the volume in the
// positive direction along each axis in turn — red along X, green along
// Y, blue along Z — about one second per axis, three per full cycle. The
// travel range is widened by one band-width on each side so the band
// slides completely on and completely off instead of popping at the
// edges. Verifies that a 3D map's axes point the expected directions
// (assuming the strip's color order is already right).

var BAND_W = 0.2       // band half-width: total span somewhat under half an axis
var SMOOTH = 1         // 1 = sinusoidal bump (default), 0 = flat crisp band

var axis = 0
var center = 0

export function beforeRender(delta) {
  var t = time(0.046)              // ~3 s full cycle
  axis = floor(t * 3)              // 0 = X, 1 = Y, 2 = Z
  var progress = t * 3 - axis      // 0..1 within this axis's second
  // Stretch travel by a band-width past each end of the unit range.
  center = -BAND_W + progress * (1 + 2 * BAND_W)
}

export function render3D(index, x, y, z) {
  var coord = x
  if (axis == 1) coord = y
  if (axis == 2) coord = z

  var d = coord - center
  var v = 0
  if (abs(d) < BAND_W) {
    if (SMOOTH) {
      // Fractional position across the band, 0..1; sinusoidal bump that
      // is zero at both edges and peaks at the band center.
      var f = d / BAND_W * 0.5 + 0.5
      v = sin(f * PI)
    } else {
      v = 1
    }
  }

  if (axis == 0) rgb(v, 0, 0)
  else if (axis == 1) rgb(0, v, 0)
  else rgb(0, 0, v)
}

// 2D maps get the same sweep on a z = 0 slice (the Z pass lights the
// whole panel as the band crosses zero).
export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}
