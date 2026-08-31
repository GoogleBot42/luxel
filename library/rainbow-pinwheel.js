// name: rainbow pinwheel
// Clean-room reimplementation from a prose functional description of the
// community pattern "rainbow pinwheel"; original source never consulted.
//
// A folded, scrolling rainbow: hue is a sinusoid of (time + position), so
// colors sweep up and back down the strip instead of hard-wrapping. On a
// radial layout it reads as a spinning pinwheel. Saturation is overdriven
// well past 1 (it clamps) to guarantee fully saturated color.

var t1 = 0

// Tunables — the top-level values are the constants the port shipped with, so
// an untouched pattern renders exactly as before.
var iSpin = 0.05     // time() interval of the spin (~3.3 s)
var repeats = 1      // folded rainbows along the strip
var dir = 1          // +1 = forward, -1 = reversed
var span = 1         // fraction of the color wheel the rainbow covers
var baseH = 0        // hue the rainbow starts from
var sat = 2          // deliberately overdriven; hsv() clamps it

// Seconds for the pinwheel to turn once.
//# min=0.5 max=30 step=0.1 default=3.3
export function sliderSpinSeconds(v) { iSpin = max(v, 0.2) / 65.536 }

// How many folded rainbows fit along the strip.
//# min=1 max=8 step=1 default=1
export function sliderRepeats(v) { repeats = clamp(floor(v), 1, 16) }

// Spin the other way.
//# default=0
export function toggleReverse(v) { dir = v ? -1 : 1 }

// How much of the color wheel the rainbow covers, as a percentage: 100 is the
// full rainbow, small values sweep a narrow band around the base color.
//# min=1 max=100 step=1 default=100
export function sliderColorRangePercent(v) { span = clamp(v, 1, 100) / 100 }

// Where the sweep starts on the wheel (and its saturation).
export function hsvPickerBaseColor(h, s, v) { baseH = h; sat = s }

export function beforeRender(delta) {
  t1 = time(iSpin)  // ~3.3 s per full cycle
}

export function render(index) {
  var h = wave(t1 + dir * index * repeats / pixelCount)
  hsv(baseH + h * span, sat, 1)
}
