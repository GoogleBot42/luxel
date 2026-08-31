// name: Autumn Colors
// Clean-room reimplementation from a prose functional description of the
// community pattern "Autumn Colors"; original source never consulted.

// Four autumn-leaf hues, exported so they can be retargeted live from the
// variable watcher (e.g. Christmas or Halloween variants).
export var hueRed = 0
export var hueBrown = 0.035
export var hueOrange = 0.075
export var hueYellow = 0.13

var hues = array(pixelCount)
var levels = array(pixelCount)
var tickAccum = 0

// The palette above spans 46.8 degrees of the wheel (red -> yellow); the
// controls below shift and stretch that span, and set how long a leaf takes to
// fade out. Defaults reproduce the untouched pattern exactly.
var PALETTE_SPAN = 46.8   // degrees covered by the four hues
var fadeMs = 5000         // full fade-out time
var hueShift = 0          // turns added to every hue
var hueScale = 1          // palette-span stretch
var sat = 1               // saturation

//# min=0.5 max=15 step=0.5 default=5
export function sliderFadeSeconds(v) { fadeMs = max(0.1, v) * 1000 }

//# min=0 max=360 step=5 default=0
export function sliderHueShift(v) { hueShift = v / 360 }

//# min=0 max=180 step=1 default=47
export function sliderColorRange(v) { hueScale = v / PALETTE_SPAN }

//# min=0 max=100 step=1 default=100
export function sliderSaturation(v) { sat = clamp(v / 100, 0, 1) }

// Weighted hue choice: red ~30%, brown ~30%, orange ~37%, yellow ~3%
function pickHue() {
  var r = random(1)
  if (r < 0.3) return hueRed
  if (r < 0.6) return hueBrown
  if (r < 0.97) return hueOrange
  return hueYellow
}

export function beforeRender(delta) {
  tickAccum += delta
  if (tickAccum < 30) return          // throttle updates to ~30 ms ticks
  var elapsed = tickAccum
  tickAccum = 0
  for (var i = 0; i < pixelCount; i++) {
    // decay by the actual elapsed time this tick; full fade ~5 s by default
    levels[i] -= elapsed / fadeMs
    if (levels[i] <= 0) {
      hues[i] = pickHue()
      levels[i] = random(1)           // pop back at a random brightness
    }
  }
}

export function render(index) {
  var b = levels[index]
  // Stored hues stay raw; shift/stretch are applied here so the controls
  // retint pixels that are already alight.
  hsv(hueShift + hues[index] * hueScale, sat, b * b)  // squared value deepens the dim end
}
