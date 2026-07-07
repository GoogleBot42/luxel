// name: Christmas Candy Cane
// Clean-room reimplementation from a prose functional description of the
// community pattern "Christmas Candy Cane"; original source never consulted.

// Eight equal segments alternating red / white, scrolling as a slow
// conveyor-belt crawl. Speed is scaled by the frame delta so the crawl is
// wall-clock stable (~3 px/s: a full lap on a 60-px strip takes ~20 s).

var SEGMENTS = 8
var segLen = pixelCount / SEGMENTS
var scrollSpeed = 0.003 // pixels per millisecond
var offset = 0

export function beforeRender(delta) {
  offset += delta * scrollSpeed
  if (offset >= pixelCount) offset -= pixelCount
}

export function render(index) {
  var shifted = mod(index + offset, pixelCount)
  var seg = floor(shifted / segLen)
  if (mod(seg, 2) == 0) {
    hsv(0, 1, 0.5) // saturated red at half brightness
  } else {
    hsv(0, 0, 1) // pure white, full brightness
  }
}
