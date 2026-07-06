// name: 2d Clock with Hand Color Pickers
// Clean-room reimplementation from a prose functional description of the
// community pattern "2d Clock with Hand Color Pickers"; original source
// never consulted.

// Analog clock on a 2D map: hour/minute/second hands as hard-edged colored
// wedges from the center (12 o'clock straight up), each with its own picked
// color. Four radius modes shape the hands into rings, clusters, hard bands,
// or solid rays. Requires the device wall clock to be set.

// hand colors (HSV); defaults: blue hours, green minutes, red seconds
var hourH = 0.6667, hourS = 1, hourV = 1
var minH = 0.3333, minS = 1, minV = 1
var secH = 0, secS = 1, secV = 1
export function hsvPickerHourHandColor(h, s, v) { hourH = h; hourS = s; hourV = v }
export function hsvPickerMinuteHandColor(h, s, v) { minH = h; minS = s; minV = v }
export function hsvPickerSecondHandColor(h, s, v) { secH = h; secS = s; secV = v }

var mode = 3
//# min=0 max=1 step=0.34 default=1
export function sliderRadiusMode(v) { mode = floor(v * 3.99) }  // rings/clustered/bands/rays

var sharpBase = 11
//# min=0 max=1 step=0.01 default=0.5
export function sliderSharpness(v) { sharpBase = 1 + v * v * 40 }  // squared response

var strength = 1.1
//# min=0 max=1 step=0.01 default=0.32
export function sliderStrength(v) { strength = 1 + v * v }  // ~1..2, squared

var breatheAmt = 0
//# min=0 max=1 step=0.01 default=0
export function sliderBreathe(v) { breatheAmt = v }

var breatheInterval = 0.1
//# min=0 max=1 step=0.01 default=0.1
export function sliderSpeed(v) { breatheInterval = 0.015 + v * 0.9 }  // ~1 s .. ~1 min

var zoom = 1
//# min=0 max=1 step=0.01 default=0
export function sliderDistance(v) { zoom = 1 + v * 4 }

// state: last-seen whole second + sub-second accumulator (kept for a
// smooth-seconds option; the second hand deliberately ticks whole seconds)
var lastSec = -1
var subSec = 0

var hFrac, mFrac, sFrac, sharp

export function beforeRender(delta) {
  var s = clockSecond()
  subSec += delta / 1000
  if (s != lastSec) {
    lastSec = s
    subSec = 0.008   // ~half a frame's jitter guess
  }
  var m = clockMinute() + s / 60
  var h = mod(clockHour(), 12) + m / 60
  sFrac = s / 60      // ticking second hand
  mFrac = m / 60      // sweeping
  hFrac = h / 12      // sweeping, 12-hour dial

  // effective edge sharpness, with optional slow "breathe" oscillation
  sharp = sharpBase * (1 + breatheAmt * 0.6 * sin(time(breatheInterval) * PI2))
  if (sharp < 1) sharp = 1
}

// radial intensity per hand (0=hour, 1=minute, 2=second) for the active mode
function radial(r, hand) {
  if (mode == 3) return 1   // rays: solid to the edge
  if (mode == 0) {
    // equidistant rings: phase-offset triangle waves, overdriven + clamped
    return min(1, triangle(r * 2 + hand * 0.33) * 1.5)
  }
  if (mode == 1) {
    // clustered: different spatial frequencies; hour = fading center disc
    if (hand == 0) return max(0, 1 - r * 2.2)
    if (hand == 1) return min(1, triangle(r * 3) * 1.5)
    return min(1, triangle(r * 5 + 0.3) * 1.5)
  }
  // bands: hard annuli -- hour innermost, minute middle, second outermost
  if (hand == 0) return (r > 0.04 && r < 0.16) ? 1 : 0
  if (hand == 1) return (r > 0.2 && r < 0.32) ? 1 : 0
  return (r > 0.36 && r < 0.48) ? 1 : 0
}

// hard on/off decision for one hand
function handOn(a, f, r, hand) {
  // triangle-wave angular distance to the hand (0 at hand, 1 opposite)
  var d = abs(a - f)
  if (d > 0.5) d = 1 - d
  var v = (strength - d * 2) * radial(r, hand)
  v = clamp(v, 0, 1)   // clamp before the power so overdrive just widens
  return pow(v, sharp) > 0.5
}

export function render2D(index, x, y) {
  var dx = x - 0.5
  var dy = y - 0.5
  // angle from straight up, wrap seam at 12 o'clock: swap the axes'
  // roles in atan2 so 12 is the zero/one seam and 3 o'clock is 0.25
  var a = mod(atan2(dx, -dy) / PI2, 1)
  var r = hypot(dx, dy) * zoom

  // priority: hour occludes minute occludes second; else black
  if (handOn(a, hFrac, r, 0)) hsv(hourH, hourS, hourV)
  else if (handOn(a, mFrac, r, 1)) hsv(minH, minS, minV)
  else if (handOn(a, sFrac, r, 2)) hsv(secH, secS, secV)
  else rgb(0, 0, 0)
}
