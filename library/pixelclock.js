// name: pixelClock
// Clean-room reimplementation from a prose functional description of the
// community pattern "pixelClock"; original source never consulted.

// An analog clock on a strip/ring: colored blocks for seconds, minutes and
// hours tick around the layout over a very dim neutral face. Positions are
// mapped proportionally (normalized position * 60 or * 12) so any pixel
// count works and remainder pixels are distributed evenly, instead of the
// original's fixed-width integer regions.

// Hand colors and face options. The top-level values are exactly what the port
// shipped with, so an untouched clock looks the same as before.
var secH = 0.667, secS = 1,   secV = 0.5    // seconds: saturated blue
var minH = 0.333, minS = 1,   minV = 1      // minutes: saturated green
var hrH  = 0.98,  hrS  = 0.8, hrV  = 1      // hours: warm red, a touch pink
var faceV = 0.03                            // clock face background glow
var hourDivs = 12                           // 12- or 24-hour face
var reverse = 0                             // 1 = hands run counter-clockwise

export function hsvPickerSecondsColor(h, s, v) { secH = h; secS = s; secV = v }
export function hsvPickerMinutesColor(h, s, v) { minH = h; minS = s; minV = v }
export function hsvPickerHoursColor(h, s, v)   { hrH = h;  hrS = s;  hrV = v }

// Divide the face into 24 hours instead of 12.
//# default=0
export function toggle24HourFace(v) { hourDivs = v ? 24 : 12 }

// Run the hands the other way around the ring.
//# default=0
export function toggleReverse(v) { reverse = v }

export function beforeRender(delta) {
  sec = clockSecond()
  minu = clockMinute()
  hr = clockHour() % hourDivs
}

export function render(index) {
  var p = index / pixelCount
  if (reverse) p = (pixelCount - 1 - index) / pixelCount
  var region60 = floor(p * 60)        // which of 60 face divisions
  var regionHr = floor(p * hourDivs)  // which hour division

  if (region60 == sec) {
    hsv(secH, secS, secV)      // seconds hand
  } else if (region60 == minu) {
    hsv(minH, minS, minV)      // minutes hand
  } else if (regionHr == hr) {
    hsv(hrH, hrS, hrV)         // hours block
  } else {
    hsv(0, 0, faceV)           // face: extremely dim neutral white
  }
}
