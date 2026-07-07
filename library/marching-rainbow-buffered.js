// name: marching rainbow (buffered)
// Clean-room reimplementation from a prose functional description of the
// community pattern "marching rainbow (buffered)"; original source never
// consulted.

// Broad rainbow bands march one way while a finer, faster ripple travels
// the other way and carves dark notches through them. Demonstrates the
// frame-buffering idiom: all math happens in beforeRender into arrays;
// render is just two lookups (keeps the pixel loop fast and jitter-free
// for clockless LED protocols).

var hues = array(pixelCount)
var vals = array(pixelCount)

export function beforeRender(delta) {
  var t1 = time(0.07)    // slow clock, ~4.6 s
  var t2 = time(0.035)   // twice as fast

  for (var i = 0; i < pixelCount; i++) {
    var p = i / pixelCount

    // Wave A: one spatial cycle across the strip, drifting one way.
    var a = wave(t1 + p)
    // Wave B: ~10 spatial cycles, moving the opposite direction (note the
    // minus sign), faster, with a small phase nudge.
    var b = wave(t2 - p * 10 + 0.12)

    // Difference (not product): where the ripple exceeds the band the
    // value goes negative and clamps to black — hard-edged moving notches.
    vals[i] = a - b

    // Double wave-warping of the slow wave bends the position-to-hue
    // gradient, so the rainbow compresses and stretches as it moves.
    hues[i] = wave(wave(a) - p)
  }
}

export function render(index) {
  hsv(hues[index], 1, vals[index])
}
