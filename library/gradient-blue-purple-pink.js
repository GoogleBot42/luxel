// name: Gradient blue  purple pink
// Clean-room reimplementation from a prose functional description of the
// community pattern "Gradient blue  purple pink"; original source never
// consulted.

// A full-strip gradient confined to the blue -> purple -> pink/magenta
// band, flowing along the strip in a few seconds, while a much slower
// saturation ripple makes regions breathe between richly saturated and
// slightly pastel. The two rates beat against each other so the look
// never exactly repeats on a short timescale. Always full brightness.

var hueClock = 0
var satClock = 0

// Tunables, initialized to the constants the port shipped with so the
// untouched render is unchanged; the sliders re-express them in real units.
var hueInterval = 0.06   // time() interval for the hue flow (~3.9 s)
var baseHue = 0.63       // low end of the band, as a position on the wheel
var hueSpan = 0.24       // width of the band, as a fraction of the wheel
var cycles = 1           // spatial repeats of the gradient along the strip

// Seconds for the gradient to flow one full cycle along the strip.
//# min=0.5 max=20 step=0.1 default=3.9
export function sliderFlowTime(v) {
  hueInterval = max(v, 0.5) / 65.536
}

// Low end of the color band, as a position on the color wheel
// (0 = red, 0.33 = green, 0.63 = blue, 0.85 = pink).
//# min=0 max=1 step=0.01 default=0.63
export function sliderBaseHue(v) {
  baseHue = clamp(v, 0, 1)
}

// Width of the color band, as a fraction of the color wheel
// (0 = one flat color, 0.24 = blue -> purple -> pink, 1 = full rainbow).
//# min=0 max=1 step=0.01 default=0.24
export function sliderHueSpan(v) {
  hueSpan = clamp(v, 0, 1)
}

// How many copies of the gradient fit along the strip.
//# min=1 max=6 step=1 default=1
export function sliderRepeats(v) {
  cycles = clamp(floor(v), 1, 6)
}

export function beforeRender(delta) {
  hueClock = time(hueInterval)   // hue band traverses in ~4 s
  satClock = time(0.35)          // saturation ripple ~23 s — much slower
}

export function render(index) {
  var p = index / pixelCount   // exactly one spatial cycle per strip

  // Hue: triangle wave compressed to a quarter-wheel band centered on
  // blue-violet — true blue at the low end, pink/magenta at the top.
  var h = baseHue + hueSpan * triangle(p * cycles + hueClock)

  // Saturation: slower triangle, offset upward so its upper range pins at
  // full saturation (plateaus of pure color), dipping toward pastel.
  var s = min(1, 0.7 + 0.6 * triangle(p + satClock))

  hsv(h, s, 1)
}
