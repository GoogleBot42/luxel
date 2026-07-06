// name: glitch bands
// Clean-room reimplementation from a prose functional description of the
// community pattern "glitch bands"; original source never consulted.

// Rainbow gradient segments that shear, stretch and hard-wrap ("glitch"),
// overlaid with desaturated flashes where two traveling triangle waves
// collide, plus a hard bright/dim segmentation. Everything is deterministic
// interference between incommensurate time ramps -- no randomness.

var baseHue, modulus, slope, w1t, w2t

export function beforeRender(delta) {
  // slow sine swings the base hue back and forth (~5 s cycle)
  baseHue = 0.5 + 0.35 * sin(time(0.08) * PI2)

  // band wrap modulus: smallish, varying by ~half its own size (~6 s)
  modulus = 0.18 + 0.09 * triangle(time(0.09))

  // spatial slope: slow triangle (~26 s) + moderate sinusoid (~10 s);
  // sweeps from mildly negative through zero to strongly positive
  slope = (triangle(time(0.4)) * 2 - 0.5) * 3 + 1.2 * sin(time(0.15) * PI2)

  // phases for the two traveling waves
  w1t = time(0.05)    // ~3.3 s drift, one direction
  w2t = time(0.016)   // ~1 s, the other direction
}

export function render(index) {
  var pos = index / pixelCount
  var d = pos - 0.5   // signed distance from strip center

  // hue offset = position * slope, wrapped modulo the band modulus.
  // `%` keeps the sign of its left operand, so the two halves of the
  // strip wrap in mirrored directions -- that is the glitch-band trick.
  var h = baseHue + (d * slope) % modulus

  // wave one: several repetitions, sharpened by squaring
  var v1 = pow(triangle(pos * 4 + w1t), 2)
  // wave two: ~one repetition, opposite direction, sharpened hard
  var v2 = pow(triangle(pos - w2t), 4)

  // white-hot flashes where the wave product lands mid-range
  var s = 1 - triangle(v1 * v2)

  // hard bright/dim segmentation; the bright side deliberately overdrives
  var v = v1 > v2 ? 1.5 : 0.55

  hsv(h, s, v)
}
