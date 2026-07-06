// name: Spring Colors
// Clean-room reimplementation from a prose functional description of the
// community pattern "Spring Colors"; original source never consulted.

// Gentle unsynchronized twinkle field: every pixel independently glows in
// one of four palette hues, fades to black over several seconds, then
// instantly relights in a freshly-drawn hue at a random start brightness.
// No waves, no motion — a calm, ever-shifting mosaic.

var h1 = 0.00    // primary: red          (~30%)
var h2 = 0.03    // secondary: red-orange (~30%)
var h3 = 0.08    // tertiary: orange      (~37%)
var h4 = 0.13    // quaternary accent: golden yellow (rare, ~3%)

var bri = array(pixelCount)
var hues = array(pixelCount)
var accum = 0

var TICK_MS = 40           // housekeeping cadence
var FADE_PER_TICK = 0.008  // full -> black in ~5 s at this cadence

export function hsvPickerPrimary(h, s, v) { h1 = h }
export function hsvPickerSecondary(h, s, v) { h2 = h }
export function hsvPickerTertiary(h, s, v) { h3 = h }
export function hsvPickerQuaternary(h, s, v) { h4 = h }

export function beforeRender(delta) {
  accum += delta
  if (accum < TICK_MS) return
  accum -= TICK_MS   // decrement, don't zero — keeps long-run cadence honest

  for (var i = 0; i < pixelCount; i++) {
    bri[i] -= FADE_PER_TICK
    if (bri[i] <= 0) {
      // weighted palette draw: ~30 / 30 / 37 / 3
      var r = random(1)
      if (r < 0.30) hues[i] = h1
      else if (r < 0.60) hues[i] = h2
      else if (r < 0.97) hues[i] = h3
      else hues[i] = h4
      // relight anywhere from off to full — this keeps it organic, not blinky
      bri[i] = random(1)
    }
  }
}

export function render(index) {
  var v = bri[index]
  hsv(hues[index], 1, v * v)   // squared for a perceptually smooth fade
}
