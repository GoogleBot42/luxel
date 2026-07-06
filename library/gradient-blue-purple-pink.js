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

export function beforeRender(delta) {
  hueClock = time(0.06)   // hue band traverses in ~4 s
  satClock = time(0.35)   // saturation ripple ~23 s — several-fold slower
}

export function render(index) {
  var p = index / pixelCount   // exactly one spatial cycle per strip

  // Hue: triangle wave compressed to a quarter-wheel band centered on
  // blue-violet — true blue at the low end, pink/magenta at the top.
  var h = 0.63 + 0.24 * triangle(p + hueClock)

  // Saturation: slower triangle, offset upward so its upper range pins at
  // full saturation (plateaus of pure color), dipping toward pastel.
  var s = min(1, 0.7 + 0.6 * triangle(p + satClock))

  hsv(h, s, 1)
}
