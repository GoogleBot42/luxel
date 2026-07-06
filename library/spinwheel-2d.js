// name: Spinwheel 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Spinwheel 2D"; original source never consulted.

// A mandala-like radial spinner: bright petal points arranged in concentric
// rings around the center. The spin rate swells and ebbs (ramp x triangle
// drivers), the rings breathe radially, and hues sweep with time and
// position. Petal cores wash out toward white; fringes stay saturated.

var spin = 0   // angular driver: saw * triangle * -PI (erratic rotation)
var breathe = 0  // radial driver: triangle pushing the rings in and out

export function beforeRender(delta) {
  // ~2 s sawtooth modulated by a ~6.5 s triangle -> non-uniform spin
  spin = -PI * time(0.03) * triangle(time(0.1))
  // ~4 s triangle scaled by a moderate speed constant
  breathe = triangle(time(0.06)) * 5
}

export function render2D(index, x, y) {
  // recenter on the middle of the 0..1 map
  x -= 0.5
  y -= 0.5

  // jittered polar coordinates
  var angle = atan2(y, x) + spin * 30
  var radius = hypot(x, y) + breathe

  // per-ring weight from the integer part of the radius
  var w = floor(radius) / PI2
  if (w == 0) w = 0.618  // keep the innermost band lit

  // local coordinates inside one polar cell
  var fa = frac(angle)
  var fr = frac(radius)

  // inverse-square hotspot: one bright petal point per cell
  var v = w * 0.008 / (fa * fa + fr * fr + 0.0005)

  // intensity feeds hue, saturation, and value: white-hot cores, colored rims
  hsv(spin + x * y + v, 1 - v, v)
}
