// name: color fade pulse
// Clean-room reimplementation from a prose functional description of the
// community pattern "color fade pulse"; original source never consulted.

// Three free-running sawtooth timers with unrelated periods: a fast hue
// scroll, a slow saturation wash (as a full-circle angle), and a medium
// brightness-pulse drift. Everything else is stateless per-pixel math.

var timeScl = 1    // 1 / speed: multiplies every timer's period
var rainbows = 2   // hue cycles laid across the strip
var pulses = 4     // brightness peaks across the strip
var widthK = 1     // pulse narrowing: 1 = the reference 16%-wide peak

// Overall animation rate; 1x is the pattern's native pace.
//# min=0.1 max=4 step=0.1 default=1
export function sliderSpeed(v) { timeScl = 1 / max(0.05, v) }

//# min=0 max=6 step=0.5 default=2
export function sliderRainbows(v) { rainbows = max(0, v) }

//# min=1 max=12 step=1 default=4
export function sliderPulses(v) { pulses = max(1, floor(v)) }

// Width of a pulse at half brightness, as a percentage of the spacing between
// pulses. The native fourth-power triangle peak measures about 16%.
//# min=2 max=100 step=1 default=16
export function sliderPulseWidth(v) { widthK = 16 / clamp(v, 2, 100) }

export function beforeRender(delta) {
  hueT = time(0.01 * timeScl)      // ~0.66 s: fast rainbow scroll
  satT = time(0.08 * timeScl) * PI2 // ~5.2 s: slow saturation wave, as an angle
  pulseT = time(0.025 * timeScl)   // ~1.6 s: pulse-peak drift
}

export function render(index) {
  var p = index / pixelCount   // normalized position, layout-proportional

  // Hue cycles laid across the strip (two by default), scrolling steadily.
  var h = rainbows * p - hueT

  // One long spatial saturation wave (half a hue-circle of offset across
  // the strip) sliding with time: vivid <-> washed-out near-white.
  var s = (1 + sin(satT + p * PI)) / 2

  // ~4 triangular brightness peaks drifting along the strip; the 4th
  // power sharpens them into narrow spikes with long dark valleys, and
  // the halving keeps the peaks moderate. Steepening the triangle first
  // (widthK > 1) narrows every peak proportionally.
  var tri = triangle(pulseT + pulses * p)
  tri = saturate(1 - (1 - tri) * widthK)
  var v = tri * tri * tri * tri * 0.5

  hsv(h, s, v)
}
