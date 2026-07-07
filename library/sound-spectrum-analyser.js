// name: Sound - Spectrum Analyser
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sound - Spectrum Analyser"; original source never consulted.

// Classic spectrum analyser on a matrix: one bar per column (bass left),
// white falling peak dots, a fast horizontally scrolling rainbow, and a
// PI-controller auto-gain so the tallest bar rides near the top.

// Matrix width in pixels — edit to match your matrix (height is derived).
var width = 16
const BAND_COUNT = 32

// Sensor inputs (engine-bound; zeros when no sound board is present)
export var frequencyData = array(BAND_COUNT)
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0

var height = 1
var barHeights = array(width) // per-column bar height in whole rows
var peaks = array(width)      // per-column peak marker row (-1 = hidden)
var peakMs = 0                // ms accumulator for peak decay
var loudAvg = 0               // rolling average of the loudest (pre-clamp) bar
var integral = 0              // PI controller integral term (clamped)
var sensitivity = 1
var t1 = 0

var i
for (i = 0; i < width; i++) peaks[i] = -1

export function beforeRender(delta) {
  height = floor(pixelCount / width)
  t1 = time(0.015) // rainbow scroll: full hue cycle in ~1 s

  // Auto gain: PI controller aiming the loudest bar at ~0.9 of full scale.
  // Sensitivity floored at 1 so silence never amplifies noise to full.
  var target = 0.9
  var err = target - loudAvg
  integral = clamp(integral + err * delta / 1000, 0, 10)
  sensitivity = max(1, 1 + err * 2 + integral)

  // Peak decay: every 100 ms each peak marker drops one row (~10 rows/s)
  peakMs += delta
  while (peakMs >= 100) {
    peakMs -= 100
    for (i = 0; i < width; i++) {
      if (peaks[i] > -1) peaks[i] -= 1
    }
  }

  // Bars: log column->band curve gives bass more columns than linear would
  var loudest = 0
  for (i = 0; i < width; i++) {
    var band = floor(log(1 + i / width) / log(2) * BAND_COUNT)
    if (band > BAND_COUNT - 1) band = BAND_COUNT - 1
    var e = frequencyData[band] * sensitivity
    if (e > loudest) loudest = e     // pre-clamp, feeds auto-gain
    var bar = floor(min(e, 1) * height)
    barHeights[i] = bar
    if (bar - 1 > peaks[i]) peaks[i] = bar - 1
  }

  // Slow exponential blend into the rolling average (~2 s time constant)
  var k = min(1, delta / 2000)
  loudAvg += (loudest - loudAvg) * k
}

export function render2D(index, x, y) {
  var col = clamp(floor(x * width), 0, width - 1)
  // map origin is top-left: flip so row 0 is the bottom
  var row = clamp(floor((1 - y) * height), 0, height - 1)
  var h = x + t1 // scrolling rainbow (hue wraps)
  if (row == peaks[col]) {
    hsv(h, 0, 1)                 // white floating peak dot
  } else if (row < barHeights[col]) {
    hsv(h, 1, 1)                 // bar fill
  } else {
    rgb(0, 0, 0)
  }
}
