// name: Angry Xmass 3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Angry Xmass 3D"; original source never consulted.
//
// WARNING: aggressive strobe - not epilepsy-friendly.
// Hard-edged red/green bands with thin racing white streaks on a 3D map.
// Every frame the band axis is randomly re-picked from three variants, so
// the whole thing flickers violently. Requires a 3D pixel map (render3D
// only, per the original - no 1D/2D fallback).

var tFast = 0
var tUltra = 0
var variant = 0

export function beforeRender(delta) {
  tFast = time(0.005)     // ~0.33 s per cycle: band scroll + brightness
  tUltra = time(0.0005)   // ~33 ms per cycle: the white streak
  variant = floor(random(3))   // re-pick the axis variant EVERY frame
}

export function render3D(index, x, y, z) {
  var hueAxis = z
  var satAxis = z
  if (variant == 1) {
    hueAxis = x
    satAxis = x
  } else if (variant == 2) {
    hueAxis = z
    satAxis = y
  }

  // two-level square wave clamped up to ~1/3: hue is green or red (1 wraps)
  var h = max(square(hueAxis + tFast, 0.5), 0.33)
  // saturated ~9/10 of the time; the zero slice is the racing white streak
  var s = square(satAxis + tUltra, 0.9)
  // coordinate product = curved hyperbola-like interference sheets
  var v = triangle(x * y * z + tFast)
  hsv(h, s, v)
}
