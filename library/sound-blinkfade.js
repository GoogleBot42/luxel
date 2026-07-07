// name: sound - blinkfade
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - blinkfade"; original source never consulted.

// Pixels blink on at random in saturated colors and fade out. A PI
// controller closes the loop between displayed brightness and a target
// fill fraction, so twinkle density self-calibrates to any room volume.
// New blinks take a slowly drifting hue plus a pitch-dependent offset.

// Sensor globals (stubbed with zeros when no sensor board is attached).
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0
export var frequencyData = array(32)

// Observable auto-gain output.
export var sensitivity = 0

// PI controller internals.
var targetFill = 0.2      // aim for ~a fifth of the strip lit
var kP = 4
var kI = 0.4
var integral = 0
var integralMax = 40
var accum = 0             // brightness feedback from last render pass

var vals = array(pixelCount)   // per-pixel stored brightness
var hues = array(pixelCount)   // per-pixel hue

export function beforeRender(delta) {
  var dt = delta / 1000

  // Auto-gain: error between target fill and last frame's measured fill.
  var fill = accum / pixelCount
  accum = 0
  var err = targetFill - fill
  integral = clamp(integral + err * dt, 0, integralMax)
  sensitivity = max(0, kP * err + kI * integral)

  // Base hue cycles over several seconds; dominant pitch adds a bounded
  // offset via a triangle fold over an audible-range scale.
  var baseHue = time(0.08)
  var pitchShift = triangle(maxFrequency / 5000) * 0.25

  // Louder sound fades existing pixels faster.
  var decay = dt * 0.25 + dt * energyAverage * sensitivity * 3

  for (var i = 0; i < pixelCount; i++) {
    vals[i] -= decay
    if (vals[i] <= 0) {
      // Re-ignite: brightness scales with loudness x sensitivity, with
      // per-pixel randomness. In silence this is 0 — the strip idles dark.
      vals[i] = energyAverage * sensitivity * random(1)
      hues[i] = baseHue + pitchShift
    }
  }
}

export function render(index) {
  // Scale up and square for punchy contrast and nicer fades.
  var v = vals[index] * 3
  v = v * v
  accum += clamp(v, 0, 1)   // feed the PI controller
  hsv(hues[index], 1, v)
}
