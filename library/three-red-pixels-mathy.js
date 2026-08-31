// name: Marching Dots
// Clean-room reimplementation from a prose functional description of the
// community pattern "Three Red Pixels (mathy)"; original source never
// consulted. Renamed after controls turned the "three" and the "red" into
// dials (Jeremy's review, 2026-08-31); the defaults still are three red dots.

// Teaching example: frame-rate-independent motion + modular arithmetic.
// Evenly spaced dots crawl over a solid background at a speed given in
// pixels per second. One fractional position drives every dot: pixel offsets
// are taken modulo the dot spacing, so a single comparison finds them all.

var speed = 10          // pixels per second, identical at any frame rate
var dots = 3            // number of evenly spaced dots
var spacing = floor(pixelCount / dots)
var dotWidth = 1        // dot length in pixels

var dotHue = 0          // red
var dotSat = 1
var bgHue = 2 / 3       // blue
var bgSat = 1

export var pos = 0 // exported for external inspection/adjustment

// Controls — the top-level values above are what the pattern shipped with,
// so an untouched pattern renders exactly as before.

// How many dots march around the strip, evenly spaced.
//# min=1 max=12 step=1 default=3
export function sliderDots(v) {
  dots = clamp(floor(v), 1, 12)
  spacing = max(floor(pixelCount / dots), 1)
}

// Crawl speed in pixels per second; negative values march the other way.
//# min=-60 max=60 step=1 default=10
export function sliderSpeedPixelsPerSec(v) { speed = v }

// Dot length in pixels.
//# min=1 max=5 step=1 default=1
export function sliderDotWidthPixels(v) { dotWidth = clamp(floor(v), 1, 5) }

// Color of the marching dots (brightness is always full).
export function hsvPickerDotColor(h, s, v) { dotHue = h; dotSat = s }

// Color of the field they march over.
export function hsvPickerBackgroundColor(h, s, v) { bgHue = h; bgSat = s }

export function beforeRender(delta) {
  // mod() (not %) so a negative speed wraps instead of running off the end
  pos = mod(pos + speed * delta / 1000, pixelCount)
}

export function render(index) {
  // Add a strip length before subtracting so the offset never goes negative.
  var offset = (index - pos + pixelCount) % spacing
  if (offset < dotWidth) {
    hsv(dotHue, dotSat, 1)   // dot (within dotWidth of an image of pos)
  } else {
    hsv(bgHue, bgSat, 1)     // background
  }
}
