// name: regenbogendrogen
// Clean-room reimplementation from a prose functional description of the
// community pattern "regenbogendrogen"; original source never consulted.

// A mirrored psychedelic rainbow: the center-distance ramp is folded through
// wave() twice (once before, once after adding the time phase), compressing
// and stretching the colors into flowing multicolored bands.

var t1 = 0

export function beforeRender(delta) {
  t1 = time(0.2) // ~13 s per full cycle
}

export function render(index) {
  // distance from strip midpoint -> symmetric about the center
  var d = abs(index / pixelCount - 0.5)
  // negate/offset slightly so the center sits near one end of the ramp,
  // then fold twice through the sine waveshaper with the time phase between
  var h = wave(wave(0.05 - d) + t1)
  hsv(h, 1, 1)
}
