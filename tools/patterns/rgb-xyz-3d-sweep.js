// name: RGB-XYZ 3D Sweep
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGB-XYZ 3D Sweep"; original source never consulted.

// 3D-map diagnostic: a glowing planar band sweeps the volume in the positive
// direction along each axis in turn — red along X, green along Y, blue along
// Z — about one second per axis. The band's travel is widened by a band-width
// on each side so it slides fully on and off instead of popping at the edges.

const BAND = 0.2       // band half-width (total width < half the axis)
const SMOOTHED = 1     // 1 = sinusoidal bump across the band, 0 = flat band

var axis = 0
var center = 0

export function beforeRender(delta) {
  var phase = time(0.0458) * 3        // full 3-axis cycle ~3 s
  axis = floor(phase)
  var progress = phase - axis         // 0..1 within this axis's second
  // travel from fully outside the low end to fully outside the high end
  center = -BAND + progress * (1 + 2 * BAND)
}

export function render3D(index, x, y, z) {
  var c = axis == 0 ? x : (axis == 1 ? y : z)
  var d = abs(c - center)
  if (d < BAND) {
    var b = 1
    if (SMOOTHED) {
      // fractional position across the band, quarter-cycle offset:
      // zero at both edges, sinusoidal peak at the band center
      var u = (c - center + BAND) / (2 * BAND)
      b = wave(u - 0.25)
    }
    hsv(axis / 3, 1, b)   // true primaries: red X, green Y, blue Z
  } else {
    rgb(0, 0, 0)
  }
}
