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

export function beforeRender(delta) {
  // three sawtooth phases, seconds-scale periods, one about twice as fast
  var p1 = time(0.09)      // ~5.9 s
  var p2 = time(0.045)     // ~3.0 s (about twice as fast)
  var p3 = time(0.07)      // ~4.6 s
  var i = 0
  for (i = 0; i < pixelCount; i++) {
    // static two-cycle rainbow ramp, never animated
    hueB[i] = i / (pixelCount / 2)

    // fully saturated almost everywhere; narrow drifting dips to white
    var s = wave(-i / 5 + p3)
    satB[i] = 1 - s * s * s * s

    // two short-period spatial waves drifting in opposite directions,
    // multiplied, plus a slightly longer one drifting with the first;
    // the 4th power crushes the mid-range so only interference peaks
    // survive as distinct bright bands
    var a = wave(i / 2 + p1)
    var b = wave(i / 5 - p2)
    var c = wave(i / 7 + p1)
    var v = (a * b + c) / 2
    briB[i] = v * v * v * v
  }
}

export function render(index) {
  hsv(hueB[index], satB[index], briB[index])
}
