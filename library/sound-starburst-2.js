// name: sound - Starburst 2
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - Starburst 2"; original source never consulted.

// Mirrored sets of comet heads sweep the strip in opposite directions,
// crossing through each other. Trails linger while the music is loud and
// rising; in quiet moments trails snuff out fast and the hue strobes.
// With no sound input the strip idles dark.

// Sound sensor bindings (engine stubs them with zeros when absent)
export var frequencyData = array(32)   // 32 bands, low frequencies first
export var energyAverage = 0

var SLOW_FADE = 0.99    // per-frame trail decay while energy is rising
var FAST_FADE = 0.86    // per-frame trail decay while energy is falling
var HEAD_SAT = 0.15     // freshly stamped heads are near-white

// Per-pixel state buffers
var vals = array(pixelCount)   // brightness
var fades = array(pixelCount)  // per-pixel decay coefficient
var sats = array(pixelCount)   // saturation (re-saturates as trail ages)
var i
for (i = 0; i < pixelCount; i++) fades[i] = FAST_FADE

var numLeaders = 4
var rainbow = 0

var avgShort = 0    // short-window EMA of total spectrum energy
var avgLong = 0     // ~double-window EMA
var rising = 0      // beat / rising-energy detector

var fastHue = 0     // cycles a few times per second (quiet passages)
var slowHue = 0     // cycles over a few seconds (loud passages)
var hue = 0         // active hue phase
var pos = 0
var flare = 0

//# min=1 max=12 step=1 default=4
export function sliderHowManyLeaders(v) {
  numLeaders = clamp(floor(v + 0.5), 1, 12)
}

//# min=0 max=1 step=1 default=0
export function sliderRainbow(v) {
  rainbow = v > 0.5
}

export function beforeRender(delta) {
  // Sweep position: one full traversal every ~5 seconds
  pos = time(0.08)

  // Hand-rolled hue phase accumulators, advanced by frame delta (ms)
  fastHue = frac(fastHue + delta / 350)    // ~3 cycles per second
  slowHue = frac(slowHue + delta / 4200)   // one cycle over a few seconds

  // Spectrum sums: lowest handful of bands + all the rest
  var lows = 0, highs = 0
  for (i = 0; i < 5; i++) lows += frequencyData[i]
  for (i = 5; i < 32; i++) highs += frequencyData[i]
  var total = lows + highs

  // Two exponential moving averages; the short one crossing above the
  // long one is the beat / rising-energy detector
  avgShort = avgShort * 0.88 + total * 0.12
  avgLong = avgLong * 0.94 + total * 0.06
  rising = avgShort > avgLong

  // Head brightness: overall energy scaled ~two orders of magnitude,
  // gated to nonzero only while energy is rising
  flare = rising ? clamp(energyAverage * 150, 0, 1) : 0

  // Stamp the heads and their mirror images
  var mirror = 1 - pos
  for (i = 0; i < numLeaders; i++) {
    var p = floor(frac(pos + i / numLeaders) * pixelCount)
    vals[p] = flare
    sats[p] = HEAD_SAT
    fades[p] = SLOW_FADE

    p = floor(frac(mirror + i / numLeaders) * pixelCount)
    vals[p] = flare
    sats[p] = HEAD_SAT
    fades[p] = SLOW_FADE
  }

  // Calm slow hue while loud and rising; fast strobing hue when quiet
  hue = rising ? slowHue : fastHue
}

export function render(index) {
  var h = hue
  // Mild positional gradient when energy is falling: a few hue cycles
  // spread across four strip-lengths
  if (!rising) h += index / pixelCount * 0.75
  // Strong positional gradient when the rainbow toggle is on
  if (rainbow) h += index / pixelCount * 5

  // Whitish hot cores re-saturate to full color over a handful of frames
  sats[index] = min(1, sats[index] * 1.12)

  // Trails collapse quickly whenever energy is falling
  if (!rising) fades[index] = FAST_FADE
  vals[index] = vals[index] * fades[index]

  hsv(h, sats[index], vals[index])
}
