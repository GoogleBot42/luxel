// name: regenbogendrogen
// Clean-room reimplementation from a prose functional description of the
// community pattern "regenbogendrogen"; original source never consulted.

var phase = 0

export function beforeRender(delta) {
  phase = time(0.18)   // ~11.8 s per full color cycle
}

export function render(index) {
  // distance from the strip midpoint -> mirror-symmetric output
  var d = abs(index / pixelCount - 0.5)
  // negate/offset slightly, then fold through wave() twice with the time
  // phase added in between: the double waveshaping compresses/stretches the
  // rainbow into shifting multicolored bands
  var h = wave(wave(0.05 - d) + phase)
  hsv(h, 1, 1)
}
