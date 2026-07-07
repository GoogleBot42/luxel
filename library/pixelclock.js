// name: pixelClock
// Clean-room reimplementation from a prose functional description of the
// community pattern "pixelClock"; original source never consulted.

// An analog clock on a strip/ring: colored blocks for seconds, minutes and
// hours tick around the layout over a very dim neutral face. Positions are
// mapped proportionally (normalized position * 60 or * 12) so any pixel
// count works and remainder pixels are distributed evenly, instead of the
// original's fixed-width integer regions.

export function beforeRender(delta) {
  sec = clockSecond()
  minu = clockMinute()
  hr = clockHour() % 12        // 12-hour face
}

export function render(index) {
  var p = index / pixelCount
  var region60 = floor(p * 60) // which of 60 face divisions
  var region12 = floor(p * 12) // which of 12 face divisions

  if (region60 == sec) {
    hsv(0.667, 1, 0.5)         // seconds: saturated blue, half brightness
  } else if (region60 == minu) {
    hsv(0.333, 1, 1)           // minutes: saturated green, full brightness
  } else if (region12 == hr) {
    hsv(0.98, 0.8, 1)          // hours: warm red, a touch pink, full brightness
  } else {
    hsv(0, 0, 0.03)            // face: extremely dim neutral white
  }
}
