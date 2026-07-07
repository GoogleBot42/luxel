// name: RGB Test Pattern
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGB Test Pattern"; original source never consulted.

// Wiring / channel-order / length diagnostic. The first and last pixels
// are always half-brightness white end markers. Every tenth pixel lights
// in the current test color, which cycles half-white -> red -> green ->
// blue about every two seconds. Everything else stays black.

const MODE_MS = 2000    // ~2 s per test color
const STRIDE = 10       // every tenth pixel carries the test color

var timerMs = 0
export var mode = 0     // exported: visible/settable from outside

// lookup table of RGB triples, 4 modes x 3 channels
var modeColors = array(12)
modeColors[0] = 0.5; modeColors[1]  = 0.5; modeColors[2]  = 0.5  // half white
modeColors[3] = 1;   modeColors[4]  = 0;   modeColors[5]  = 0    // pure red
modeColors[6] = 0;   modeColors[7]  = 1;   modeColors[8]  = 0    // pure green
modeColors[9] = 0;   modeColors[10] = 0;   modeColors[11] = 1    // pure blue

export function beforeRender(delta) {
  timerMs += delta
  while (timerMs >= MODE_MS) {
    timerMs -= MODE_MS          // subtract, don't reset: no drift
    mode = (mode + 1) % 4
  }
}

export function render(index) {
  if (index == 0 || index == pixelCount - 1) {
    rgb(0.5, 0.5, 0.5)          // endpoint markers
  } else if (index % STRIDE == 0) {
    var base = floor(mode) * 3
    rgb(modeColors[base], modeColors[base + 1], modeColors[base + 2])
  } else {
    rgb(0, 0, 0)
  }
}
