// name: 2d Clock with Hand Color Pickers
// Clean-room reimplementation from a prose functional description of the
// community pattern "2d Clock with Hand Color Pickers"; original source
// never consulted.

// An analog clock face on a 2D map: hour / minute / second hands drawn as
// hard-edged colored wedges radiating from the map center, 12 o'clock
// straight up. Minute and hour hands sweep smoothly; the second hand ticks
// whole seconds. A "radius mode" selector reshapes the hands into soft
// concentric rings, clustered rings, hard annular bands, or solid rays.
// Needs the device wall clock to be set for correct time.

// hand colors (defaults: hour blue, minute green, second red)
var hourH = 0.6667, hourS = 1, hourV = 1
var minH  = 0.3333, minS  = 1, minV  = 1
var secH  = 0,      secS  = 1, secV  = 1

export function hsvPickerHourHandColor(h, s, v)   { hourH = h; hourS = s; hourV = v }
export function hsvPickerMinuteHandColor(h, s, v) { minH  = h; minS  = s; minV  = v }
export function hsvPickerSecondHandColor(h, s, v) { secH  = h; secS  = s; secV  = v }

// 0 = equidistant rings, 1 = clustered, 2 = bands, 3 = rays
var radiusMode = 3
//# min=0 max=1 step=0.333 default=1
export function sliderRadiusMode(v) { radiusMode = floor(v * 3.999) }

var sharpBase = 30
//# min=0 max=1 step=0.01 default=0.9
export function sliderSharpness(v) { sharpBase = 1 + v * v * 40 }

var strength = 1.06
//# min=0 max=1 step=0.01 default=0.2
export function sliderStrength(v) { strength = 1.02 + v * v }

var breatheAmt = 0
//# min=0 max=1 step=0.01 default=0
export function sliderBreathe(v) { breatheAmt = v }

// breathing period, ~1 s .. ~60 s
var breatheInterval = 0.15
//# min=0 max=1 step=0.01 default=0.15
export function sliderSpeed(v) { breatheInterval = (1 + v * 59) / 65.536 }

var zoom = 1
//# min=0 max=1 step=0.01 default=0
export function sliderDistance(v) { zoom = 1 + v * 4 }

// frame state
var hourAngle = 0, minAngle = 0, secAngle = 0
var sharpness = 30
var lastSecond = -1
var subSec = 0        // sub-second accumulator (kept for smooth-seconds use)

export function beforeRender(delta) {
  var s = clockSecond()
  var m = clockMinute()
  var h = clockHour() % 12

  // sub-second bookkeeping: reset on the tick with a half-frame jitter guess
  subSec += delta / 1000
  if (s != lastSecond) {
    lastSecond = s
    subSec = delta / 2000
  }

  secAngle  = s / 60                       // deliberate whole-second ticks
  minAngle  = (m + s / 60) / 60            // smooth sweep
  hourAngle = (h + (m + s / 60) / 60) / 12 // smooth sweep

  sharpness = sharpBase
  if (breatheAmt > 0) {
    // slow oscillation of edge definition
    sharpness = max(1, sharpBase * (1 + breatheAmt * (wave(time(breatheInterval)) - 0.5) * 1.6))
  }
}

// triangle-wave angular distance: 0 when aligned, 1 at the opposite side
function angDist(a, b) {
  return triangle(a - b)
}

// one hand's thresholded intensity test
function handOn(pixAngle, handAngle, radial) {
  var v = (strength - angDist(pixAngle, handAngle)) * radial
  return pow(v, sharpness) > 0.5
}

export function render2D(index, x, y) {
  var dx = x - 0.5
  var dy = y - 0.5

  // angle from straight up (12 o'clock), clockwise, wrap seam at 12:
  // swap the axes into atan2 and normalize to 0..1
  var a = mod(atan2(dx, -dy) / PI2, 1)
  var r = hypot(dx, dy) * 2 * zoom

  // radial intensity per hand, by mode
  var rh, rm, rs
  if (radiusMode == 0) {
    // equidistant soft rings, phase-offset per hand, overdriven + clamped
    rh = min(1, triangle(r * 3)        * 1.5)
    rm = min(1, triangle(r * 3 + 0.33) * 1.5)
    rs = min(1, triangle(r * 3 + 0.67) * 1.5)
  } else if (radiusMode == 1) {
    // clustered: different spatial frequencies; hour is a fading center disc
    rh = clamp(1.2 - r * 2, 0, 1)
    rm = min(1, triangle(r * 4)       * 1.5)
    rs = min(1, triangle(r * 6 + 0.3) * 1.5)
  } else if (radiusMode == 2) {
    // hard annuli: hour innermost, minute middle, second outer
    rh = (r >= 0.08 && r < 0.35) ? 1 : 0
    rm = (r >= 0.40 && r < 0.62) ? 1 : 0
    rs = (r >= 0.67 && r < 0.92) ? 1 : 0
  } else {
    // solid rays to the edge
    rh = 1; rm = 1; rs = 1
  }

  // priority: hour occludes minute occludes second
  if (handOn(a, hourAngle, rh)) {
    hsv(hourH, hourS, hourV)
  } else if (handOn(a, minAngle, rm)) {
    hsv(minH, minS, minV)
  } else if (handOn(a, secAngle, rs)) {
    hsv(secH, secS, secV)
  } else {
    rgb(0, 0, 0)
  }
}
