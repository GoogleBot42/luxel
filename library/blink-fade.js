// name: blink fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "blink fade"; original source never consulted.

// Twinkle field: every pixel independently pops on at a random brightness,
// fades linearly to black over a few seconds, then instantly re-ignites
// with a fresh random level and a freshly sampled hue. Hues carry a gentle
// positional gradient (~1/5 of the wheel, triangle-shaped so the strip's
// ends match) on top of a palette that slowly drifts around the rainbow.

var levels = array(pixelCount)   // per-pixel brightness, 0..1
var hues = array(pixelCount)     // hue frozen at ignition time

var HUE_SPREAD = 0.2       // positional band: ~1/5 of the color wheel
var FADE_RATE = 0.0004     // per ms -> full fade in ~2.5 s

export function beforeRender(delta) {
  // Palette anchor drifts a full revolution every ~6.5 s.
  var t1 = time(0.1)
  var fall = delta * FADE_RATE
  for (var i = 0; i < pixelCount; i++) {
    levels[i] -= fall
    if (levels[i] <= 0) {
      // Re-ignite: random restart height keeps pixels desynchronized;
      // the hue is frozen now and held for this whole fade.
      levels[i] = random(1)
      hues[i] = t1 + triangle(i / pixelCount) * HUE_SPREAD
    }
  }
}

export function render(index) {
  var v = levels[index]
  // Squaring reads as a natural fade tail and punchier pops.
  hsv(hues[index], 1, v * v)
}
