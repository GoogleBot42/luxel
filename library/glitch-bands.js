// name: glitch bands
// Clean-room reimplementation from a prose functional description of the
// community pattern "glitch bands"; original source never consulted.

// Rainbow gradient segments that shear, stretch and hard-wrap ("glitch")
// as a slope factor sweeps through zero, overlaid with whitish desaturated
// flashes where two opposing traveling triangle waves collide, plus a hard
// bright/dim segmentation where one wave exceeds the other. Deterministic
// interference between incommensurate time ramps -- no randomness at all.

var baseHue = 0, modulus = 0.2, slope = 0, tW1 = 0, tW2 = 0

export function beforeRender(delta) {
  // base hue swings back and forth through part of the wheel (~6.5 s)
  baseHue = 0.35 * sin(time(0.1) * PI2)

  // hue-band wrap modulus: smallish, varying by about half its size (~5 s)
  modulus = 0.18 + 0.09 * triangle(time(0.08))

  // spatial slope: slow triangle (tens of seconds) + moderate sinusoid
  // (~10 s); sweeps from mildly negative through zero to strongly positive
  slope = (triangle(time(0.5)) - 0.35) * 4 + 1.5 * sin(time(0.16) * PI2)

  // drivers for the two traveling waves
  tW1 = time(0.05)    // ~3.3 s cycle
  tW2 = time(0.015)   // ~1 s cycle, opposite direction
}

export function render(index) {
  var p = index / pixelCount - 0.5   // signed distance from strip center

  // repeating gradient segments with hard wrap discontinuities: modulo the
  // hue OFFSET, not the position; % keeps the sign of its left operand so
  // the two halves of the strip wrap in mirrored directions
  var hue = baseHue + (p * slope) % modulus

  // wave one: several reps across the strip, drifting one way, squared
  var w1 = pow(triangle(p * 4 - tW1), 2)
  // wave two: ~one rep, moving the other way faster, sharpened harder
  var w2 = pow(triangle(index / pixelCount + tW2), 4)

  // desaturated flashes where the waves overlap at moderate strength
  var sat = 1 - triangle(w1 * w2)

  // hard bright/dim segmentation; bright side overdrives and clamps
  var val = w1 > w2 ? 1.5 : 0.5

  hsv(hue, sat, val)
}
