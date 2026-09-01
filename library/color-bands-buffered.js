// name: color bands (buffered)
// Clean-room reimplementation from a prose functional description of the
// community pattern "color bands (buffered)"; original source never consulted.

// Technique demo: everything is computed in beforeRender into per-pixel
// hue/saturation/brightness buffers; render only reads them, keeping the
// per-pixel callback fast for timing-sensitive LED protocols. Sparse
// bright rainbow bands drift in opposing directions over a dark
// background, brightening where they interfere; narrow desaturated
// (whitish) zones drift through slowly. Spatial wave periods are in
// absolute pixels on purpose, so band width is constant on any strip.

var hueB = array(pixelCount)
var satB = array(pixelCount)
var briB = array(pixelCount)

// --- controls ---------------------------------------------------------
// The setters convert real units into the constants the pattern shipped
// with, so the declared defaults reproduce the stock look.
var cycles = 2         // rainbow revolutions along the whole strip
var bandScale = 1      // multiplies the 2/5/7 px brightness wave periods
var satPx = 5          // period of the white-spot saturation wave, in pixels
var p1arg = 0.09       // time() argument, ~5.9 s
var p2arg = 0.045      // time() argument, ~3.0 s (about twice as fast)
var p3arg = 0.07       // time() argument, ~4.6 s

// How many times the rainbow wraps between the two ends of the strip.
//# min=0.25 max=8 step=0.25 default=2
export function sliderRainbowCycles(v) { cycles = v }

// Width of the bright drifting bands, in pixels (the narrowest of the three
// interfering waves; the other two scale with it).
//# min=1 max=40 step=0.5 default=2
export function sliderBandWidthPixels(v) { bandScale = max(v, 0.5) / 2 }

// Width of the narrow zones that desaturate towards white, in pixels.
//# min=1 max=40 step=0.5 default=5
export function sliderWhiteSpotWidthPixels(v) { satPx = max(v, 0.5) }

// Seconds for the slowest drift clock; the other two keep their ratios to it.
//# min=0.5 max=60 step=0.1 default=5.9
export function sliderDriftSeconds(v) {
  var s = max(v, 0.25)
  p1arg = s / 65.536
  p2arg = s * 0.5 / 65.536
  p3arg = s * 0.77778 / 65.536
}

export function beforeRender(delta) {
  // three sawtooth phases, seconds-scale periods, one about twice as fast
  var p1 = time(p1arg)     // ~5.9 s
  var p2 = time(p2arg)     // ~3.0 s (about twice as fast)
  var p3 = time(p3arg)     // ~4.6 s
  var i = 0
  for (i = 0; i < pixelCount; i++) {
    // static rainbow ramp, never animated
    hueB[i] = i / (pixelCount / cycles)

    // fully saturated almost everywhere; narrow drifting dips to white
    var s = wave(-i / satPx + p3)
    satB[i] = 1 - s * s * s * s

    // two short-period spatial waves drifting in opposite directions,
    // multiplied, plus a slightly longer one drifting with the first;
    // the 4th power crushes the mid-range so only interference peaks
    // survive as distinct bright bands. The sum is NOT halved before the
    // power: halving it left the whole pattern several stops too dim
    // (mean 12/255 against the reference's 75), so the peaks now clip
    // to full brightness the way the sibling `color bands` does.
    var a = wave(i / (2 * bandScale) + p1)
    var b = wave(i / (5 * bandScale) - p2)
    var c = wave(i / (7 * bandScale) + p1)
    var v = a * b + c
    briB[i] = v * v * v * v
  }
}

export function render(index) {
  hsv(hueB[index], satB[index], briB[index])
}
