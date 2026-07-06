// name: pixelClock
// Clean-room reimplementation from a prose functional description of the
// community pattern "pixelClock"; original source never consulted.

// An analog clock on a strip or ring: a blue seconds block, a green
// minutes block, and a wider warm-red hours block tick around the layout
// over a very dim near-white face. Seconds occlude minutes, minutes
// occlude the hour block. Clock values map to pixel positions
// proportionally (normalized position times 60 or 12), so any pixel
// count works and remainder pixels are distributed evenly — fixing the
// original's hardcoded integer region widths. Needs wall time from the
// host; with an unset clock the hands sit at the default epoch time.

var sec = 0
var minute = 0
var hour = 0

export function beforeRender(delta) {
  sec = clockSecond()
  minute = clockMinute()
  hour = clockHour() % 12
}

export function render(index) {
  var p = index / pixelCount
  var r60 = floor(p * 60)   // sixty-division region (seconds & minutes)
  var r12 = floor(p * 12)   // twelve-division region (hours)

  if (r60 == sec) {
    hsv(0.667, 1, 0.5)      // seconds: saturated blue, half brightness
  } else if (r60 == minute) {
    hsv(0.333, 1, 1)        // minutes: saturated green, full brightness
  } else if (r12 == hour) {
    hsv(0.98, 0.8, 1)       // hours: warm red, a touch pink
  } else {
    hsv(0, 0, 0.03)         // clock face: very faint neutral glow
  }
}
