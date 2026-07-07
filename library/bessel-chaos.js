// name: Bessel Chaos
// Clean-room reimplementation from a prose functional description of the
// community pattern "Bessel Chaos"; original source never consulted.

// Deterministic "chaos": a cubic/quartic spatial chirp inside sines, driven
// by several slow oscillators with non-commensurate periods. No randomness.

var phase = 0      // per-frame interference phase (~2 turns of swing)
var hueDiv = 6     // color-spread divisor (breathes over tens of seconds)
var pan = 0        // wandering focal point, -0.5 .. 0.5

// Shapes the pan sweep. Low = smooth continuous sweep; high = dwell at the
// ends and snap across the middle (mirrored fractional root, applied per
// half-domain so a negative base never sees a fractional exponent).
var panShape = 9
//# min=1 max=15 step=0.1 default=9
export function sliderTransitionSpeed(v) {
  panShape = 1 + v * 14
}

export function beforeRender(delta) {
  // Band-drift phase: ~12 s sine swinging about two full turns of angle.
  phase = sin(time(0.183) * PI2) * PI2 * 2

  // Color spread breathes over ~23 s between a small divisor and ~triple it.
  hueDiv = 4 + 8 * wave(time(0.35))

  // Focal-point pan: ~15 s sine, eased with a mirrored odd-root curve.
  var s = sin(time(0.227) * PI2)
  pan = sign(s) * pow(abs(s), 1 / panShape) * 0.5
}

export function render(index) {
  // Signed coordinate centered on the wandering focal point.
  var d = (index / pixelCount - 0.5 - pan) * 3

  // Nonlinear squeeze: near the focus the argument changes slowly (broad
  // breathing blobs), far away it explodes into fine chaotic ripples.
  var squeeze = d * d * d * 10

  var w1 = sin(squeeze * d + 2 * phase)   // quartic chirp, co-drifting
  var w2 = sin(squeeze - phase)           // cubic chirp, counter-drifting

  // Average then cube: sharp positive crests survive; negative values
  // clamp to black in hsv(), giving the dark background for free.
  var v = (w1 + w2) / 2
  v = v * v * v

  // Hues breathe around cyan; small divisor = wide spread into blue/green
  // /violet, large divisor = near-monochrome cyan.
  hsv(0.5 + w2 / hueDiv, 1, v)
}
