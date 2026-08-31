// name: Color Pick Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "Color Pick Fade"; original source never consulted.

// Four evenly spaced soft pulses of one user-chosen color drift along the
// strip against black. Gentle mood lighting: each pulse is a triangle wave
// raised to the fourth power (narrow bump, long dark gaps) and peak
// brightness is capped at half of maximum.

var pulses = 4

var hue = 0.05           // warm orange default
var sat = 1
var cycleInterval = 0.145 // time() interval: ~9.5 s for one lap of the strip
var widthK = 1           // pulse narrowing: 1 = the reference 16%-wide bump
var t1 = 0

//# min=0 max=360 step=1 default=18
export function sliderHue(v) { hue = v / 360 }

//# min=0 max=100 step=1 default=100
export function sliderSaturation(v) { sat = clamp(v / 100, 0, 1) }

// Seconds for the pulses to drift one full lap of the strip. time()'s
// interval argument is a period of 65.536 s, hence the divisor.
//# min=0.5 max=60 step=0.5 default=9.5
export function sliderCycleSeconds(v) { cycleInterval = max(0.1, v) / 65.536 }

//# min=1 max=12 step=1 default=4
export function sliderPulses(v) { pulses = max(1, floor(v)) }

// Width of a pulse at half brightness, as a percentage of the spacing between
// pulses. The native fourth-power triangle bump measures about 16%.
//# min=2 max=100 step=1 default=16
export function sliderPulseWidth(v) { widthK = 16 / clamp(v, 2, 100) }

export function beforeRender(delta) {
  t1 = time(cycleInterval)
}

export function render(index) {
  var p = frac(index / pixelCount * pulses + t1)
  var b = triangle(p)
  b = saturate(1 - (1 - b) * widthK)   // steeper ramp = narrower bump
  b = b * b
  hsv(hue, sat, b * b * 0.5)
}
