// name: RGBW Mapping Tester
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGBW Mapping Tester"; original source never consulted.

// Diagnostic: solid red -> green -> blue -> white (all channels at ~1/3 so
// power draw matches the single-channel phases), each held for ~2 seconds.
// If the observed order or colors differ, fix the controller's color-order
// or LED-type settings.

var phase = 0

export function beforeRender(delta) {
  phase = time(8 / 65.536) // full 4-color cycle every ~8 s
}

export function render(index) {
  var quarter = floor(phase * 4)
  if (quarter == 0) {
    rgb(1, 0, 0)
  } else if (quarter == 1) {
    rgb(0, 1, 0)
  } else if (quarter == 2) {
    rgb(0, 0, 1)
  } else {
    rgb(1 / 3, 1 / 3, 1 / 3)
  }
}
