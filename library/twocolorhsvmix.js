// name: TwoColorHSVMix
// Clean-room reimplementation from a prose functional description of the
// community pattern "TwoColorHSVMix"; original source never consulted.

// Two picked colors blended in a sine wave along the strip; the wave
// scrolls at an exponentially-mapped speed. An optional half-sine window
// raised to a power dims the strip toward both ends.

var h1 = 0, s1 = 1, v1 = 1        // primary color (default red)
var h2 = 0.66, s2 = 1, v2 = 1     // secondary color (default blue)
var cycleSecs = 3
var envPower = 0
var phase = 0

export function hsvPickerPrimary(h, s, v) {
  h1 = h
  s1 = s
  v1 = v
}

export function hsvPickerSecondary(h, s, v) {
  h2 = h
  s2 = s
  v2 = v
}

// Exponential mapping: several orders of magnitude of cycle time,
// centered around a few seconds. Left = glacial, right = very fast.
//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  cycleSecs = pow(10, 2.5 - 3.5 * v)   // ~316 s down to ~0.1 s
}

//# min=0 max=1 step=0.01 default=0
export function sliderEnvelope(v) {
  envPower = v * 8
}

// Shortest-way-around hue interpolation: if the two hues are more than
// half the wheel apart, lift the smaller one by a full turn so the blend
// crosses the wrap point, then wrap the result back into 0..1.
// Swapping the arguments mirrors the weight, keeping it direction-safe.
function hueMix(a, b, t) {
  if (a > b) return hueMix(b, a, 1 - t)
  if (b - a > 0.5) a += 1
  return frac(a + (b - a) * t)
}

export function beforeRender(delta) {
  phase += delta / 1000 / cycleSecs
  phase = frac(phase)
}

export function render(index) {
  var pos = index / pixelCount
  var w = wave(pos + phase)          // sine-shaped blend weight, 0..1

  var h = hueMix(h1, h2, w)
  var s = s1 + (s2 - s1) * w
  var v = v1 + (v2 - v1) * w

  // Half-sine window, slightly inset so the very ends never evaluate the
  // sine at exactly zero, raised to the envelope power.
  if (envPower > 0) {
    v *= pow(sin(PI * (index + 1) / (pixelCount + 1)), envPower)
  }

  hsv(h, s, v)
}
