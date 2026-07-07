// name: green ripple reflections
// Clean-room reimplementation from a prose functional description of the
// community pattern "green ripple reflections"; original source never
// consulted.

// Moonlight on water: three interfering waves slide along the strip on
// close-but-incommensurate clocks. Where the green wave is weak, bright
// crests desaturate to white — specular-looking glints. Brightness is
// capped around half to stay moody.

var t1, t2, t3

export function beforeRender(delta) {
  // ~2 s, ~3 s, ~2.5 s sawtooths, each scaled to a full circle of phase.
  t1 = time(0.0305) * PI2
  t2 = time(0.0458) * PI2
  t3 = time(0.0381) * PI2
}

export function render(index) {
  var p = index / pixelCount

  // 1) ~5 spatial cycles drifting one way, squared: non-negative, sharpened
  //    crests, doubled apparent frequency. Also drives saturation below.
  var w1 = sin(p * 5 * PI2 - t1)
  w1 = w1 * w1

  // 2) ~3 spatial cycles drifting the other way; left signed.
  var w2 = sin(p * 3 * PI2 + t2)

  // 3) ~1.5-cycle triangle whose phase sways back and forth sinusoidally.
  var w3 = triangle(mod(p * 1.5 + 0.3 * sin(t3), 1))

  // Average, square (folds negative troughs into faint glow, deepens
  // contrast), and halve to cap overall brightness.
  var v = (w1 + w2 + w3) / 3
  v = v * v / 2

  // Green where w1 is strong; washes to white where it is weak, so glints
  // made by the other waves land white exactly where the green wave "isn't".
  hsv(1 / 3, w1, v)
}
