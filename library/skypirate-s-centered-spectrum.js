// name: SkyPirate's Centered Spectrum
// Clean-room reimplementation from a prose functional description of the
// community pattern "SkyPirate's Centered Spectrum"; original source never
// consulted.

// A spectrum analyzer whose bars grow outward from the vertical center
// line, up and down at once. Each bar is a single hue (warm reds on the
// low side, magenta/purple on the high side) whitening toward its tips,
// with a red falling-peak marker and a PI-controller auto-gain that keeps
// the tallest bar near a target fill at any volume.
//
// Generalized from the original two-controller six-column install:
// columns come from the pixel map's x axis, vertical position from y,
// bin ranges are log-spaced from the bar count, and peak fall is scaled
// by elapsed time instead of per-frame.

export var frequencyData = array(32)   // filled by the sensor board; zeros otherwise

var nBars = 6
var levels = array(6)   // current bar heights, 0..1 of the half-column
var peaks = array(6)    // falling peak marker positions
var binLo = array(6)    // inclusive spectrum-bin ranges per bar
var binHi = array(6)
var hues = array(6)     // fixed per-bar hue table

// log-spaced bin ranges, widening toward high frequencies
var b
for (b = 0; b < nBars; b++) {
  binLo[b] = floor(pow(32, b / nBars)) - 1
  binHi[b] = max(binLo[b], floor(pow(32, (b + 1) / nBars)) - 1)
}

// low half: red through pinkish reds; high half: magenta through violet
hues[0] = 0
hues[1] = 0.97
hues[2] = 0.93
hues[3] = 0.87
hues[4] = 0.81
hues[5] = 0.75

// controls
var silence = 0.02      // noise-floor threshold subtracted from every bin
var targetFill = 0.9    // auto-gain target for the tallest bar
var dropSpeed = 0.05    // peak fall: fraction of the gap per frame @60fps

//# min=0 max=1 step=0.01 default=0.1
export function sliderSilenceLevel(v) { silence = v * 0.2 }
//# min=0 max=1 step=0.01 default=0.9
export function sliderFill(v) { targetFill = 0.3 + v * 0.68 }
//# min=0 max=1 step=0.01 default=0.15
export function sliderPeakDropSpeed(v) { dropSpeed = 0.01 + v * 0.29 }

// auto-gain state
var sens = 1            // never drops below unity
var integ = 0           // clamped PI integrator
var avgMax = 0          // EMA of recent frame maxima (~4 s time constant)

export function beforeRender(delta) {
  var dt = delta / 1000
  var maxP = 0
  var i, j, sum, n, p, gap

  for (i = 0; i < nBars; i++) {
    // average the bar's bins, noise floor subtracted before averaging
    sum = 0
    n = 0
    for (j = binLo[i]; j <= binHi[i]; j++) {
      sum += max(0, frequencyData[j] - silence)
      n += 1
    }
    p = clamp(sum / n * sens, 0, 1)
    levels[i] = p
    maxP = max(maxP, p)

    // peak sinks toward the bar by a fraction of the gap, time-scaled
    gap = peaks[i] - levels[i]
    if (gap > 0) peaks[i] -= gap * clamp(dropSpeed * dt * 60, 0, 1)
    if (levels[i] > peaks[i]) peaks[i] = levels[i]
  }

  // rolling average of frame maxima feeds the PI auto-gain
  avgMax += (maxP - avgMax) * clamp(dt / 4, 0, 1)
  var err = targetFill - avgMax
  integ = clamp(integ + err * dt * 0.4, 0, 15)
  sens = max(1, 1 + err * 3 + integ)
}

export function render2D(index, x, y) {
  var bar = floor(x * nBars * 0.9999)
  var d = abs(y - 0.5) * 2   // 0 at the center line, 1 at the tips

  if (peaks[bar] > 0.04 && abs(d - peaks[bar]) < 0.07) {
    hsv(0, 1, 1)             // red peak marker, full brightness
  } else if (d < levels[bar]) {
    // whitening concentrated near the very ends of the column
    hsv(hues[bar], 1 - pow(d, 4), 1)
  } else {
    hsv(0, 0, 0)
  }
}

// 1D fallback: each strip segment becomes a mini centered bar
export function render(index) {
  var pos = index / pixelCount
  render2D(index, pos, frac(pos * nBars))
}
