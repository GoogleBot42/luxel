// name: Sound - Spectrum Analyser
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sound - Spectrum Analyser"; original source never consulted.

// Sound sensor bindings (engine stubs them with zeros when absent)
export var frequencyData = array(32)   // 32 bands, low frequencies first

// Matrix layout: width is hardcoded (edit for your matrix); height derived.
var width = 16
var height = max(1, floor(pixelCount / width))

var bars = array(width)     // current bar heights, whole rows
var peaks = array(width)    // floating peak-dot row per column (-1 = none)
var peakMs = 0              // ms accumulator for peak decay
var avgMax = 0              // slow rolling average of the loudest bar
var integral = 0            // PI controller integral term (clamped)
var sensitivity = 1
var hueShift = 0
var i
for (i = 0; i < width; i++) peaks[i] = -1

export function beforeRender(delta) {
  hueShift = time(0.015)    // rainbow scrolls a full cycle in ~1 s

  // Auto gain: PI controller keeps the loudest bar near 9/10 of full height.
  var err = 0.9 - avgMax
  integral = clamp(integral + err * delta * 0.001, 0, 30)  // clamped: no windup
  sensitivity = max(1, err * 4 + integral)   // floored at 1: silence stays dark

  // Peak decay: every 100 ms every peak marker sinks one row (~10 rows/s)
  peakMs += delta
  while (peakMs >= 100) {
    peakMs -= 100
    for (i = 0; i < width; i++) {
      if (peaks[i] > -1) peaks[i] -= 1
    }
  }

  var frameMax = 0
  for (i = 0; i < width; i++) {
    // log curve column->band mapping: bass gets more columns than linear
    var band = floor(log2(1 + i / width) * 32)
    if (band > 31) band = 31
    var v = frequencyData[band] * sensitivity
    if (v > frameMax) frameMax = v          // pre-clamp, feeds the auto-gain
    if (v > 1) v = 1
    bars[i] = floor(v * height)
    if (bars[i] - 1 > peaks[i]) peaks[i] = bars[i] - 1
  }
  // fold loudest bar into the rolling average (~2 s time constant)
  avgMax += (frameMax - avgMax) * min(1, delta * 0.0005)
}

export function render2D(index, x, y) {
  var c = clamp(floor(x * width), 0, width - 1)
  var rTop = clamp(floor(y * height), 0, height - 1)
  var r = height - 1 - rTop        // flip: row 0 at the bottom
  var isPeak = r == peaks[c]
  if (r < bars[c] || isPeak) {
    // rainbow across x, scrolling with time; peak row desaturated to white
    hsv(x + hueShift, isPeak ? 0 : 1, 1)
  } else {
    rgb(0, 0, 0)
  }
}
