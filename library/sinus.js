// name: sinus
// Clean-room reimplementation from a prose functional description of the
// community pattern "sinus"; original source never consulted.

// A single bright sinusoidal ribbon snaking horizontally across the matrix
// with a soft colored halo, on black. Its horizontal phase oscillates back
// and forth (speeding up, slowing, reversing), and the whole scheme drifts
// around the hue wheel.

var WAVES = 3          // full sine periods across the display width
var AMP = 0.4          // vertical amplitude of the ribbon (in 0..1 space)
var STRETCH = 1.33     // push crests a bit past the top/bottom edges
var coreSlope = 2      // falloff of the bright core (2 => half-height band)

// time(n) laps in n * 65.536 s. Defaults reproduce the untouched pattern:
// 0.9 -> ~59 s slither, 0.2 -> ~13 s hue drift.
var slowInterval = 0.9
var hueInterval = 0.2

var phaseA = 0         // horizontal phase offset (slithers back and forth)
var hueBase = 0        // slowly cycling base hue

// How many full sine periods fit across the display width.
//# min=1 max=12 step=1 default=3
export function sliderWaves(v) { WAVES = max(1, floor(v)) }

// Peak-to-centre height of the ribbon, as a percentage of the panel height.
//# min=5 max=50 step=1 default=40
export function sliderAmplitude(v) { AMP = clamp(v, 1, 50) / 100 }

// Thickness of the bright core band, as a percentage of the panel height.
//# min=5 max=100 step=5 default=50
export function sliderCoreWidth(v) { coreSlope = 100 / clamp(v, 2, 100) }

// Seconds for one lap around the colour wheel.
//# min=1 max=120 step=1 default=13
export function sliderColorCycleSeconds(v) { hueInterval = max(0.5, v) / 65.536 }

// Seconds for one full back-and-forth slither of the ribbon.
//# min=5 max=300 step=1 default=59
export function sliderSlitherSeconds(v) { slowInterval = max(1, v) / 65.536 }

export function beforeRender(delta) {
  // A very slow ramp (~59 s) run through a sine oscillator. Because it is an
  // oscillation of a ramp, the ribbon drifts sideways with smoothly varying
  // speed and reverses over tens of seconds. Scaled to a couple wavelengths.
  var slow = time(slowInterval)
  phaseA = sin(slow * PI2) * 2

  // Slow hue drift, ~13 s feel.
  hueBase = time(hueInterval)
}

export function render2D(index, x, y) {
  // Stretch vertical away from center so crests run off the edges a little.
  var vy = (y - 0.5) * STRETCH + 0.5

  // Height of the sine curve at this column.
  var curve = 0.5 + AMP * sin((x * WAVES + phaseA) * PI2)

  // Vertical distance from this pixel to the curve.
  var d = abs(vy - curve)

  // Steep falloff -> narrow bright core band.
  var bri = clamp(1 - d * coreSlope, 0, 1)

  // Gentle falloff -> wider colored halo whose hue rides the cycling base.
  var hue = hueBase + clamp(1 - d * 0.5, 0, 1)

  hsv(hue, 1, bri)
}
