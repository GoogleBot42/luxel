// name: 1-USFLAG_BLINK
// Clean-room reimplementation from a prose functional description of the
// community pattern "1-USFLAG_BLINK"; original source never consulted.

// A mostly static US-flag layout on a 1D strip: the first quarter alternates
// red/white stripe blocks (~5 px runs), the second quarter is solid blue with
// a white "star" roughly every fifth pixel, and the back half returns to
// red/white blocks. Only the stars animate: they blink in unison — bright
// white ~90% of a few-second cycle, briefly winking out. A signed wrap
// offset lets the flag be rotated around a loop (defaulted to 0 here).

// pixel classification codes
var OFF = 0
var RED = 1
var WHITE = 2
var BLUE = 3

var state = array(pixelCount)
var star = array(pixelCount)     // 1 = this white pixel blinks

export var wrapOffset = 0        // signed rotation, watchable/settable
export var lastStarIndex = 0

var built = 0
var starV = 1                    // shared star brightness this frame

// wrap a logical index (plus the rotation offset) onto the strip
function shifted(i) {
  return mod(i + wrapOffset, pixelCount)
}

function buildFlag() {
  var q = floor(pixelCount / 4)
  var i
  for (i = 0; i < pixelCount; i++) {
    var p = shifted(i)
    if (i < q) {
      // first quarter: alternating red/white blocks of ~5 pixels
      state[p] = mod(floor(i / 5), 2) == 0 ? RED : WHITE
      star[p] = 0
    } else if (i < 2 * q) {
      // second quarter: blue field, a white star roughly every 5th pixel
      if (mod(i - q, 5) == 2) {
        state[p] = WHITE
        star[p] = 1
        lastStarIndex = p
      } else {
        state[p] = BLUE
        star[p] = 0
      }
    } else {
      // back half: red/white blocks again
      state[p] = mod(floor(i / 5), 2) == 0 ? RED : WHITE
      star[p] = 0
    }
  }
  built = 1
}

export function beforeRender(delta) {
  if (!built) buildFlag()
  // ~3.9 s cycle, on nine-tenths of the time — the brief blink-off
  starV = square(time(0.06), 0.9)
}

export function render(index) {
  var s = state[index]
  if (s == BLUE) rgb(0, 0, 1)
  else if (s == RED) rgb(1, 0, 0)
  else if (s == WHITE) {
    if (star[index]) rgb(starV, starV, starV)
    else rgb(1, 1, 1)
  } else rgb(0, 0, 0)
}
