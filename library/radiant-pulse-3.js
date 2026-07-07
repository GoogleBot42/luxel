// name: radiant pulse 3
// Clean-room reimplementation from a prose functional description of the
// community pattern "radiant pulse 3"; original source never consulted.

// Slow pulses radiate outward/inward around the layout center, morphing
// between concentric rings, straight beams, and a rotating three-lobed
// clover. Every timescale derives from ONE ~5-minute clock: its sine is
// multiplied by large factors, so the seconds-scale pulsing, the lobe
// rotation, and the in/out direction flips all breathe together.

var t, pulse, lobePhase, radK

export function beforeRender(delta) {
  t = time(4.6)                       // master clock, ~300 s period
  var s = sin(t * PI2)
  pulse = s * 20                      // ~20 pulse cycles per swing
  lobePhase = s * 15                  // spins the three-leaf form
  radK = (wave(t) - 0.5) * 7          // ring density, roughly -3.5 .. +3.5;
                                      // sign flips outward/inward, near zero
                                      // the lobes/beams dominate
}

export function render3D(index, x, y, z) {
  // Center the planar coordinates; depth is deliberately ignored, so every
  // horizontal layer of a true 3D map shows the same image.
  x -= 0.5
  y -= 0.5
  var r = hypot(x, y)
  var ang = atan2(x, y)               // swapped args just rotate "angle zero"

  // Sum three phase terms, wrap, triangle: unbounded phase becomes soft
  // repeating bands; squaring shapes them into pulses with dark gaps.
  var ph = pulse + sin(ang * 3 + lobePhase) + r * radK
  var v = triangle(mod(ph, 1))
  v = v * v

  // Bright cores desaturate toward pastel/white; dim regions stay rich.
  var s = 1.5 - v

  // Hue fans gently with direction and distance, and the whole palette
  // cycles once around the wheel per master-clock period.
  var h = triangle(ang / PI2) * 0.2 + r * 0.2 + t

  hsv(h, s, v)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}
