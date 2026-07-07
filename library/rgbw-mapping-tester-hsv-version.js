// name: RGBW Mapping Tester - HSV Version
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGBW Mapping Tester - HSV Version"; original source
// never consulted.

// Diagnostic: the whole strip cycles solid red -> green -> blue -> white,
// each held a bit over a second. White is made via hsv() with saturation 0,
// which drives the dedicated white channel on RGBW strips — that is the
// point of this "HSV version" of the tester.

var phase = 0

export function beforeRender(delta) {
  phase = time(0.08)   // full cycle ~5.2 s, ~1.3 s per color
}

export function render(index) {
  if (phase < 0.25) {
    hsv(0, 1, 1)              // pure red
  } else if (phase < 0.5) {
    hsv(1 / 3, 1, 1)          // pure green
  } else if (phase < 0.75) {
    hsv(2 / 3, 1, 1)          // pure blue
  } else {
    hsv(0, 0, 0.5)            // white at half brightness (power parity)
  }
}
