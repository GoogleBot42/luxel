// name: RGBW Mapping Tester
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGBW Mapping Tester"; original source never consulted.

// Diagnostic: whole strip cycles solid red -> green -> blue -> white.
// If the colors appear in a different order, fix the controller's
// color-order / LED-type settings.

var phase = 0

export function beforeRender(delta) {
  // Full cycle ~8.7 s, so each color holds a bit over two seconds.
  phase = time(0.133)
}

export function render(index) {
  var q = floor(phase * 4)
  if (q == 0) {
    rgb(1, 0, 0)
  } else if (q == 1) {
    rgb(0, 1, 0)
  } else if (q == 2) {
    rgb(0, 0, 1)
  } else {
    // White at ~1/3 per channel keeps power draw close to the
    // single-channel phases.
    rgb(0.33, 0.33, 0.33)
  }
}
