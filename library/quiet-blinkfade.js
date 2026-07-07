// name: quiet blinkfade
// Clean-room reimplementation from a prose functional description of the
// community pattern "quiet blinkfade"; original source never consulted.

// Sparse, calm purple twinkle: pixels light at a random modest brightness,
// fade to black within about a second, then stay dark for several seconds
// before relighting. One scalar per pixel is both the visible brightness
// (positive) and the countdown-to-respawn dead timer (negative).

const HUE = 0.85         // fixed purple/magenta
const CAP = 0.5          // max respawn brightness
const FLOOR = -3         // dead-timer depth: dark dwell ~6x the lit time
const RATE = 0.5         // decay per second

var vals = array(pixelCount)

// seed everyone at a random phase so there's no startup wave
var i
for (i = 0; i < pixelCount; i++) {
  vals[i] = FLOOR + random(CAP - FLOOR)
}

export function beforeRender(delta) {
  var d = delta / 1000 * RATE
  for (var i = 0; i < pixelCount; i++) {
    vals[i] -= d
    if (vals[i] <= FLOOR) vals[i] = random(CAP)
  }
}

export function render(index) {
  var v = vals[index]
  // only positive values light up; squaring eases the fade-out tail
  if (v > 0) hsv(HUE, 1, v * v)
  else rgb(0, 0, 0)
}
