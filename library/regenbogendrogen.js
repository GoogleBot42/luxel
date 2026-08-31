// name: regenbogendrogen
// Clean-room reimplementation from a prose functional description of the
// community pattern "regenbogendrogen"; original source never consulted.

// A mirrored psychedelic rainbow: the center-distance ramp is folded through
// wave() twice (once before, once after adding the time phase), compressing
// and stretching the colors into flowing multicolored bands.

var t1 = 0

// time(n) runs a full lap in n * 65.536 seconds, so the seconds-based
// controls below all divide by that. Defaults reproduce the untouched
// pattern: 0.2 -> ~13.1 s per colour lap.
var cycleInterval = 0.2   // time() interval for the colour lap
var spread = 1            // stretch of the centre-distance ramp
var hueShift = 0          // turns added to every hue
var mirror = 1            // 1 = fold about the centre, 0 = single sweep

// Seconds for one full trip around the colour wheel.
//# min=2 max=60 step=0.1 default=13.1
export function sliderCycleSeconds(v) { cycleInterval = max(0.5, v) / 65.536 }

// How far the ramp is stretched before the wave folds; higher values pack
// more colour bands into the strip.
//# min=20 max=400 step=5 default=100
export function sliderColorSpread(v) { spread = max(0.05, v / 100) }

//# min=0 max=360 step=5 default=0
export function sliderHueShift(v) { hueShift = v / 360 }

// Off: the ramp sweeps end to end instead of mirroring about the centre.
//# default=1
export function toggleMirror(on) { mirror = on }

export function beforeRender(delta) {
  t1 = time(cycleInterval)
}

export function render(index) {
  var p = index / pixelCount
  // distance from strip midpoint -> symmetric about the center
  var d = mirror ? abs(p - 0.5) : p * 0.5
  // negate/offset slightly so the center sits near one end of the ramp,
  // then fold twice through the sine waveshaper with the time phase between
  var h = wave(wave(0.05 - d * spread) + t1)
  hsv(h + hueShift, 1, 1)
}
