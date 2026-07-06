// name: marching rainbow
// Clean-room reimplementation from a prose functional description of the
// community pattern "marching rainbow"; original source never consulted.

// Overlapping rainbow waves march along the strip. Brightness is the
// difference of a slow one-cycle wave and a faster fine-grained wave, so
// bright bands beat against a ripple and roughly half the strip is dark at
// any moment. Hue is the slow traveling wave fed through itself twice more
// (the nesting warps the rainbow nonlinearly), offset by position.

var t1 = 0
var t2 = 0

export function beforeRender(delta) {
  t1 = time(0.1)    // main cycle, ~6.5 s
  t2 = time(0.05)   // fine ripple, twice as fast
}

export function render(index) {
  var p = index / pixelCount
  var v = wave(t1 + p) - wave(t2 - p * 10 + 0.2)
  if (v < 0) v = 0
  var h = wave(wave(wave(t1 + p)) - p)
  hsv(h, 1, v)
}
