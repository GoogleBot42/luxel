// name: Christmas Candy Cane
// Clean-room reimplementation from a prose functional description of the
// community pattern "Christmas Candy Cane"; original source never consulted.

// Candy-cane stripes: eight equal segments alternating red / white scroll
// steadily along the strip, wrapping seamlessly. Red segments run at about
// half brightness; white segments are full brightness, so the whites read
// noticeably brighter. Scroll speed is scaled by the frame delta so the
// crawl is wall-clock stable (~30 s per full lap) on any hardware.

var SEGMENTS = 8
var LAP_SECONDS = 30                 // one full trip around the strip

var segLength = pixelCount / SEGMENTS
var offset = 0

export function beforeRender(delta) {
  offset += delta * pixelCount / (LAP_SECONDS * 1000)
  if (offset >= pixelCount) offset -= pixelCount
}

export function render(index) {
  var shifted = mod(index + offset, pixelCount)
  var seg = floor(shifted / segLength)
  if (mod(seg, 2) == 0) {
    hsv(0, 1, 0.5)                   // saturated red, half brightness
  } else {
    hsv(0, 0, 1)                     // pure white, full brightness
  }
}
