// name: quiet blinkfade
// Clean-room reimplementation from a prose functional description of the
// community pattern "quiet blinkfade"; original source never consulted.

// Sparse, calm purple twinkle: pixels light at a random modest brightness,
// fade to black within about a second, then stay dark for several seconds
// before relighting. One scalar per pixel is both the visible brightness
// (positive) and the countdown-to-respawn dead timer (negative).

const CAP = 0.5          // max respawn brightness

// Tunables — the top-level values reproduce the constants the port shipped
// with (hue 0.85, 1 s fade, 6 s dark dwell), so an untouched pattern is
// unchanged.
var hue = 0.85           // spark hue (purple/magenta)
var sat = 1              // spark saturation
var spread = 0           // per-pixel hue scatter, fraction of the color wheel
var fadeSecs = 1         // seconds for a full spark to fade to black
var darkSecs = 6         // seconds a pixel stays dark before relighting
var rate = 0.5           // decay per second = CAP / fadeSecs
var floorV = -3          // dead-timer depth = -darkSecs * rate

var vals = array(pixelCount)

// seed everyone at a random phase so there's no startup wave
var i
for (i = 0; i < pixelCount; i++) {
  vals[i] = floorV + random(CAP - floorV)
}

// Twinkle color (hue + saturation; each spark supplies its own brightness).
export function hsvPickerSparkColor(h, s, v) { hue = h; sat = s }

// Seconds a spark takes to fade from full to black.
//# min=0.1 max=8 step=0.1 default=1
export function sliderFadeSeconds(v) {
  fadeSecs = max(v, 0.05)
  rate = CAP / fadeSecs
  floorV = -darkSecs * rate
}

// Seconds a pixel stays dark before it lights again — the "quiet" dial: short
// values crowd the strip, long ones leave a rare spark here and there.
//# min=0.2 max=30 step=0.2 default=6
export function sliderDarkSeconds(v) {
  darkSecs = max(v, 0.1)
  floorV = -darkSecs * rate
}

// Scatter the sparks' hues around the chosen color, as a percentage of the
// color wheel; 0 keeps every spark the same color.
//# min=0 max=100 step=1 default=0
export function sliderColorSpreadPercent(v) { spread = clamp(v, 0, 100) / 100 }

export function beforeRender(delta) {
  var d = delta / 1000 * rate
  for (var i = 0; i < pixelCount; i++) {
    vals[i] -= d
    if (vals[i] <= floorV) vals[i] = random(CAP)
  }
}

export function render(index) {
  var v = vals[index]
  // each pixel keeps a fixed offset in the scatter (golden-ratio spacing, so
  // neighbours differ); at spread 0 this term vanishes exactly
  var h = hue + spread * (frac(index * 0.618034) - 0.5)
  // only positive values light up; squaring eases the fade-out tail
  if (v > 0) hsv(h, sat, v * v)
  else rgb(0, 0, 0)
}
