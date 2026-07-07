// name: sound - Starburst 2
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - Starburst 2"; original source never consulted.

// Evenly spaced comet heads sweep one way while a mirrored set sweeps the
// other, crossing and passing through each other, each leaving a decaying
// trail. Rising sound energy flares the heads, lets trails linger, and
// picks a slow calm hue drift; falling energy snuffs trails fast and
// switches to a fast hue accumulator for a flickery rainbow shimmer.
// With no sound input everything stays dark (flare gate is zero).

// Sensor bindings (engine stubs these with zeros when no sensor board)
export var frequencyData = array(32)
export var energyAverage = 0

// Per-pixel state
var pix = array(pixelCount)   // brightness
var fade = array(pixelCount)  // per-pixel decay coefficient
var sat = array(pixelCount)   // saturation (heads stamp low, re-saturates)
var age = array(pixelCount)   // written for parity with the original; unused

// Energy tracking: short and long exponential moving averages
var avgShort = 0
var avgLong = 0
var rising = 0
var flare = 0

// Hue phase accumulators
var fastHue = 0   // cycles several times per second
var slowHue = 0   // cycles over a few seconds
var activeHue = 0

// Decay constants (applied once per rendered frame, like the original —
// decay speed is therefore frame-rate dependent, kept for fidelity)
var slowFade = 0.985
var fastFade = 0.86

var heads = 4
//# min=0 max=1 step=0.01 default=0.3
export function sliderHowManyLeaders(v) {
  heads = floor(1 + v * 11)   // 1..12 head pairs
}

var rainbow = 0
//# min=0 max=1 step=1 default=0
export function sliderRainbow(v) {
  rainbow = v > 0.5           // above halfway = many rainbow cycles
}

var t1 = 0

export function beforeRender(delta) {
  // position phase: one traversal takes several seconds
  t1 = time(0.08)                       // ~5.2 s per sweep
  var t1r = 1 - t1                      // mirrored, opposite direction

  // hue accumulators advanced by frame delta, wrapping at 1
  fastHue = (fastHue + delta / 350) % 1   // ~3 cycles/s
  slowHue = (slowHue + delta / 4200) % 1  // ~4 s per cycle

  // spectrum sums: lowest handful of bands vs all the rest
  var lows = 0, highs = 0, i = 0
  for (i = 0; i < 4; i++) lows += frequencyData[i]
  for (i = 4; i < 32; i++) highs += frequencyData[i]
  var total = lows + highs

  // short window EMA and one with roughly twice the window
  avgShort += (total - avgShort) * 0.25
  avgLong += (total - avgLong) * 0.125

  // beat / rising-energy detector
  rising = avgShort > avgLong

  // head brightness: overall energy scaled ~two orders of magnitude,
  // gated to nonzero only while energy is rising
  flare = rising ? clamp(energyAverage * 120, 0, 1) : 0

  // stamp the heads and their mirrors
  for (i = 0; i < heads; i++) {
    var p = floor(((t1 + i / heads) % 1) * pixelCount)
    var q = floor(((t1r + i / heads) % 1) * pixelCount)
    pix[p] = flare
    sat[p] = 0.2            // whitish hot core
    fade[p] = slowFade
    age[p] = 0
    pix[q] = flare
    sat[q] = 0.2
    fade[q] = slowFade
    age[q] = 0
  }

  // calm slow hue while loud and rising, fast strobing hue when quiet
  activeHue = rising ? slowHue : fastHue
}

export function render(index) {
  var h = activeHue
  if (!rising) h += index / pixelCount * 0.75   // few cycles per 4 strips
  if (rainbow) h += index / pixelCount * 6      // several full cycles

  // freshly stamped whitish heads re-saturate to full color as they age
  sat[index] = min(1, sat[index] * 1.12)

  // trails collapse quickly whenever energy is falling
  if (!rising) fade[index] = fastFade
  pix[index] = pix[index] * fade[index]

  hsv(h, sat[index], pix[index])
}
