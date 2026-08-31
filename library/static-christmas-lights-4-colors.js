// name: Static Christmas Lights - 4 Colors
// Clean-room reimplementation from a prose functional description of the
// community pattern "Static Christmas Lights - 4 Colors"; original source
// never consulted.

// A completely static repeating four-color sequence — red, green, blue,
// yellow — like a classic incandescent Christmas light string. No animation,
// no state, no randomness. The original kept block size and brightness as
// hand-edited constants; block size is exposed as a slider here (the spec's
// own suggested improvement), defaulting to the original look. Brightness is
// deliberately NOT a pattern control — Luxel has a global brightness setting.

var hues = array(4)
hues[0] = 0        // red
hues[1] = 1 / 3    // green
hues[2] = 2 / 3    // blue
hues[3] = 1 / 6    // yellow

var blockSize = 1      // pixels per color block

//# min=1 max=10 step=1 default=1
export function sliderBlockSize(v) {
  blockSize = max(1, floor(v))
}

export function render(index) {
  var slot = mod(floor(index / blockSize), 4)
  hsv(hues[slot], 1, 1)
}
