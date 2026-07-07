// name: TwoColorHSVMix
// Clean-room reimplementation from a prose functional description of the
// community pattern "TwoColorHSVMix"; original source never consulted.
//
// A sinusoidal blend between two user-picked HSV colors scrolls along the
// strip. Hue interpolates the short way around the wheel. An optional
// half-sine window (raised to a growing power) dims toward both ends.

var h1 = 0.0, s1 = 1, v1 = 1     // primary: red
var h2 = 0.66, s2 = 1, v2 = 1    // secondary: blue
var interval = 0.05              // time() interval; ~3.3 s cycle
var envPow = 0                   // 0 = no end dimming
var phase = 0

export function hsvPickerPrimaryColor(h, s, v) {
  h1 = h; s1 = s; v1 = v
}

export function hsvPickerSecondaryColor(h, s, v) {
  h2 = h; s2 = s; v2 = v
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  // Exponential mapping: several orders of magnitude of cycle time,
  // centered around a few seconds. v=0 -> ~0.13 s, v=1 -> ~134 s.
  interval = 0.002 * pow(2, v * 10)
}

//# min=0 max=1 step=0.01 default=0
export function sliderEnvelope(v) {
  envPow = v * 8
}

// Interpolate hues the shortest way around the circle: if they are more
// than half a turn apart numerically, lift the smaller one by a full turn
// so the blend crosses the wrap point instead of going the long way.
function mixHue(a, b, t) {
  var d = b - a
  if (abs(d) <= 0.5) return a + d * t
  if (a < b) a += 1
  else b += 1
  return frac(a + (b - a) * t)
}

export function beforeRender(delta) {
  phase = time(interval)
}

export function render(index) {
  // Blend weight: sine wave of position + phase, one repeat per strip
  var w = wave(index / pixelCount + phase)
  var h = mixHue(h1, h2, w)
  var s = mix(s1, s2, w)
  var v = mix(v1, v2, w)
  if (envPow > 0) {
    // Slightly inset position so the window never evaluates its exact
    // zero endpoints.
    var p = (index + 1) / (pixelCount + 1)
    v = v * pow(sin(PI * p), envPow)
  }
  hsv(h, s, v)
}
