// name: RGBW Mapping Tester - HSV Version
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGBW Mapping Tester - HSV Version"; original source
// never consulted.

// Diagnostic, not an effect: the whole strip cycles solid red -> green ->
// blue -> white, roughly 1.3 s per color, forever. If the colors come out
// in the wrong order or non-uniform, the strip's color-order / LED-type
// settings are wrong. White is made via zero saturation through the HSV
// path on purpose: on RGBW strips that engages the dedicated white
// channel, which is the whole point of this "HSV version". White runs at
// half brightness to keep power draw comparable to single-channel colors.

var hue = 0
var sat = 1
var val = 1

export function beforeRender(delta) {
  var t = time(0.08)          // ~5.2 s full cycle, ~1.3 s per color
  if (t < 0.25) {
    hue = 0; sat = 1; val = 1        // pure red
  } else if (t < 0.5) {
    hue = 1 / 3; sat = 1; val = 1    // pure green
  } else if (t < 0.75) {
    hue = 2 / 3; sat = 1; val = 1    // pure blue
  } else {
    hue = 0; sat = 0; val = 0.5      // white via S=0 -> RGBW white LED
  }
}

export function render(index) {
  hsv(hue, sat, val)
}
