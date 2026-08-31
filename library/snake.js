// name: snake
// Clean-room reimplementation from a prose functional description of the
// community pattern "snake"; original source never consulted.
// A bright head chases along a static, fully-saturated rainbow gradient with
// a short linear tail fading to black behind it, wrapping seamlessly.
// Color Mode switches that rainbow for a solid body color or a head-to-tail
// two-color fade, both driven by the pickers.

// time(0.1) is a 0..1 sawtooth with a 0.1 x 65.536 = ~6.6 s period, i.e. one
// lap of the strip every ~6.6 s at speed 1.
var BASE_INTERVAL = 0.1
var lapInterval = BASE_INTERVAL

// Speed multiplier — HIGHER IS FASTER. 1 = ~6.6 s/lap, 10 = ~0.66 s/lap,
// 0.1 = a barely-crawling ~65 s/lap.
//# min=0.1 max=10 step=0.1 default=1
export function sliderSpeed(v) {
  lapInterval = BASE_INTERVAL / clamp(v, 0.05, 20)
}

var tailFrac = 0.15     // tail length as a fraction of the strip
//# min=0.02 max=1 step=0.01 default=0.15
export function sliderTailLength(v) {
  tailFrac = clamp(v, 0.02, 1)
}

// --- color -------------------------------------------------------------
// 0 = rainbow gradient painted on the strip (the classic look)
// 1 = one solid body color
// 2 = head color fading into body color along the tail
var colorMode = 0
//# min=0 max=2 step=1 default=0
export function sliderColorMode(v) {
  colorMode = clamp(floor(v + 0.5), 0, 2)
}

var headHue = 0.13      // warm yellow head
var headSat = 1
export function hsvPickerHeadColor(h, s, v) {
  headHue = h
  headSat = s
}

var bodyHue = 0.33      // green body
var bodySat = 1
export function hsvPickerBodyColor(h, s, v) {
  bodyHue = h
  bodySat = s
}

var head = 0

export function beforeRender(delta) {
  head = time(lapInterval)   // 0..1 normalized head position
}

export function render(index) {
  var p = index / pixelCount
  // distance this pixel sits behind the head, wrapping the strip
  var d = head - p
  if (d < 0) d += 1
  var b = 1 - d / tailFrac   // head brightest, linear ramp to zero over tail
  b = clamp(b, 0, 1)
  if (colorMode == 0) {
    hsv(p, 1, b)             // static rainbow the snake travels over
  } else if (colorMode == 1) {
    hsv(bodyHue, bodySat, b)
  } else {
    var t = 1 - b            // 0 at the head, 1 at the tail tip
    // take the short way round the hue wheel, or a blue-to-yellow snake
    // would sweep the entire rainbow on the way
    var dh = bodyHue - headHue
    if (dh > 0.5) dh -= 1
    if (dh < -0.5) dh += 1
    hsv(headHue + dh * t, headSat + (bodySat - headSat) * t, b)
  }
}
