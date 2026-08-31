// name: Christmas Candy Cane
// Clean-room reimplementation from a prose functional description of the
// community pattern "Christmas Candy Cane"; original source never consulted.

// Eight equal segments alternating red / white, scrolling as a slow
// conveyor-belt crawl. Speed is scaled by the frame delta so the crawl is
// wall-clock stable (~3 px/s: a full lap on a 60-px strip takes ~20 s).

var segments = 8
var segLen = pixelCount / segments
var scrollSpeed = 0.003 // pixels per millisecond
var stripeHue = 0       // hue of the colored stripe (red)
var colorFrac = 0.5     // share of each stripe pair given to the colored half
var offset = 0

// Total stripes around the strip: half colored, half white. Kept even so the
// alternation still meets cleanly where the pattern wraps.
//# min=2 max=24 step=2 default=8
export function sliderStripes(v) {
  segments = max(2, floor(v / 2) * 2)
  segLen = pixelCount / segments
}

// Crawl speed in pixels per second; negative scrolls the other way.
//# min=-30 max=30 step=0.5 default=3
export function sliderPixelsPerSecond(v) { scrollSpeed = v / 1000 }

//# min=0 max=360 step=5 default=0
export function sliderStripeHue(v) { stripeHue = v / 360 }

// Width of the colored stripe as a percentage of one colored/white pair.
//# min=10 max=90 step=5 default=50
export function sliderColorWidth(v) { colorFrac = clamp(v / 100, 0.05, 0.95) }

export function beforeRender(delta) {
  offset += delta * scrollSpeed
  offset = mod(offset, pixelCount)
  if (offset < 0) offset += pixelCount
}

export function render(index) {
  var shifted = mod(index + offset, pixelCount)
  // Position within one colored/white pair; below the split is colored.
  var f = frac(shifted / (segLen * 2))
  if (f < colorFrac) {
    hsv(stripeHue, 1, 0.5) // saturated stripe color at half brightness
  } else {
    hsv(0, 0, 1) // pure white, full brightness
  }
}
