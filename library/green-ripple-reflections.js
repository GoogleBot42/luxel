// name: green ripple reflections
// Clean-room reimplementation from a prose functional description of the
// community pattern "green ripple reflections"; original source never
// consulted.

// Moonlight on gently moving water: three overlapping waves slide along
// the strip (one each way, one swaying), interfering into bright crests
// and dark troughs. Saturation is driven by the first wave, so glints
// that land where it is weak wash out to pale white — reading as
// specular reflections. Brightness is capped around half to stay moody.

var GREEN = 1 / 3
var p1 = 0
var p2 = 0
var p3 = 0

export function beforeRender(delta) {
  // three close-but-incommensurate clocks (~2.0 s, ~3.0 s, ~2.5 s),
  // each scaled to a full circle of phase per cycle — the composite
  // never visibly repeats
  p1 = time(0.03) * PI2
  p2 = time(0.046) * PI2
  p3 = time(0.038) * PI2
}

export function render(index) {
  var f = index / pixelCount

  // ~5 spatial cycles drifting one way; squared: non-negative, sharper
  // crests, doubled apparent frequency
  var c1 = sin(f * 5 * PI2 - p1)
  c1 = c1 * c1

  // ~3 spatial cycles drifting the other way; left signed
  var c2 = sin(f * 3 * PI2 + p2)

  // ~1.5 spatial cycles of triangle wave, swaying back and forth
  // sinusoidally rather than drifting (folded back into range)
  var c3 = triangle((f * 1.5 + 1 + 0.3 * sin(p3)) % 1)

  // average, then square: destructive interference folds back into
  // faint glow instead of clipping black; halve to cap brightness
  var v = (c1 + c2 + c3) / 3
  v = v * v / 2

  // full green where wave one is strong, desaturating to white where
  // it is weak — the white sparkles land where the green wave "isn't"
  hsv(GREEN, c1, v)
}
