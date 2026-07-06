// name: radiant pulse 3
// Clean-room reimplementation from a prose functional description of the
// community pattern "radiant pulse 3"; original source never consulted.

// Slow pulses radiate outward/inward around the center of the layout,
// morphing between concentric rings, radial beams, and a spinning
// three-lobed clover. Every timescale is derived from ONE ~5-minute clock
// by multiplying its sine by large factors, so pulse speed, lobe spin,
// and in/out direction all breathe (and reverse) together. Depth is
// ignored, so any 2D or 3D mapping can run it.

var t, pulse, lobePhase, radialCoef

export function beforeRender(delta) {
  t = time(4.5)  // master clock: ~295 s full period
  var s = sin(t * PI2)
  pulse = s * 20                        // ~20 pulse cycles per sine swing
  lobePhase = sin(t * PI2 + PI / 2) * 15  // lobe spin, harmonically locked
  radialCoef = s * 3.5                  // signed: sign = out vs in,
                                        // magnitude = ring density
}

export function render2D(index, x, y) {
  // Center the layout, then go polar.
  x -= 0.5
  y -= 0.5
  var r = sqrt(x * x + y * y)
  var a = atan2(x, y)  // swapped args just rotate where angle zero points

  // Sum three phase terms, wrap to unit period, take a triangle wave.
  var phase = pulse + sin(a * 3 + lobePhase) + r * radialCoef
  var v = triangle(phase)
  v = v * v  // sharpen pulses, deepen gaps

  // Bright cores desaturate toward pastel/white; dim regions stay rich.
  var sat = 1.5 - v

  // Hue fans gently with direction and distance, and the whole palette
  // cycles once around the wheel over the multi-minute master clock.
  var h = triangle(a / PI2) * 0.2 + r * 0.2 + t

  hsv(h, sat, v)
}

export function render3D(index, x, y, z) {
  render2D(index, x, y)  // depth ignored: every layer shows the same image
}
