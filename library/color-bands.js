// name: color bands
// Clean-room reimplementation from a prose functional description of the
// community pattern "color bands"; original source never consulted.
//
// A double rainbow along the strip seen through a shifting moire mask:
// short bright bands crawl and interfere, with narrow spots briefly washing
// to white. Purely per-pixel, no state, no randomness; two free-running
// clocks (~10 s and ~15 s) drift the interfering spatial waves.

// --- controls ---------------------------------------------------------
// Defaults below are the shipped constants; the setters convert real units
// into them, so an untouched pattern renders exactly as before.
var cycles = 2         // rainbow revolutions along the whole strip
var bandScale = 1      // multiplies the 5/7/11 px brightness wave periods
var satPx = 3          // period of the white-spot saturation wave, in pixels
var c10arg = 0.153     // time() argument, ~10.0 s
var c15arg = 0.229     // time() argument, ~15.0 s (1.5x the first)

// How many times the rainbow wraps between the two ends of the strip.
//# min=0.25 max=8 step=0.25 default=2
export function sliderRainbowCycles(v) { cycles = v }

// Width of the crawling bright bands, in pixels (the narrowest of the three
// interfering waves; the other two scale with it).
//# min=1 max=40 step=0.5 default=5
export function sliderBandWidthPixels(v) { bandScale = max(v, 0.5) / 5 }

// Width of the narrow zones that wash out to white, in pixels.
//# min=1 max=40 step=0.5 default=3
export function sliderWhiteSpotWidthPixels(v) { satPx = max(v, 0.5) }

// Seconds for the slower of the two drift clocks; the faster one keeps its
// 2:3 relationship so the moire never locks up.
//# min=1 max=120 step=0.5 default=10
export function sliderDriftSeconds(v) {
  var s = max(v, 0.5)
  c10arg = s / 65.536
  c15arg = s * 1.5 / 65.536
}

export function beforeRender(delta) {
  c10 = time(c10arg)   // ~10.0 s cycle
  c15 = time(c15arg)   // ~15.0 s cycle
}

export function render(index) {
  // hue: normalized position traversing the wheel `cycles` times, wrapping
  var h = (index / pixelCount) * cycles

  // saturation: short-period wave drifting slowly, ^4 -> narrow white spots
  var sw = wave(index / satPx + c15)
  var s = 1 - sw * sw * sw * sw

  // brightness: three non-harmonic short-period waves; two drift one way and
  // are multiplied, the third drifts the other way and is added, then ^4
  var wa = wave(index / (5 * bandScale) + c10)
  var wb = wave(index / (7 * bandScale) + c10)
  var wc = wave(index / (11 * bandScale) - c10)
  var m = wa * wb + wc
  var v = m * m * m * m     // renderer clamps the >1 plateaus

  hsv(h, s, v)
}
