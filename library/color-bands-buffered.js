// name: color bands (buffered)
// Clean-room reimplementation from a prose functional description of the
// community pattern "color bands (buffered)"; original source never consulted.

// Technique demo: all the math happens in beforeRender, filling per-pixel
// hue/saturation/brightness buffers; render only reads them. Sparse bright
// rainbow bands drift in opposing directions over a dark background,
// interfering where they cross, with slow whitish desaturation zones
// drifting through. Spatial wave periods are in absolute pixels on
// purpose, so band width is constant on any strip length.

var hues = array(pixelCount)
var sats = array(pixelCount)
var vals = array(pixelCount)

export function beforeRender(delta) {
  var t1 = time(0.1)    // ~6.5 s
  var t2 = time(0.05)   // ~3.3 s — about twice as fast
  var t3 = time(0.14)   // ~9.2 s
  var i
  for (i = 0; i < pixelCount; i++) {
    // static two-cycle rainbow ramp (hue wraps)
    hues[i] = i / (pixelCount / 2)

    // saturation: narrow drifting dips toward white in a mostly-vivid field
    var s = wave(-i / 5 + t3)
    s = s * s
    sats[i] = 1 - s * s

    // brightness: two short spatial waves drifting in opposite directions,
    // multiplied, plus a slightly longer wave riding with the first;
    // fourth power crushes the midrange so only interference peaks survive
    var v = (wave(i / 2.5 + t1) * wave(i / 5 - t2) + wave(i / 7 + t1)) / 2
    v = v * v
    vals[i] = v * v
  }
}

export function render(index) {
  hsv(hues[index], sats[index], vals[index])
}
