// name: Rainbow rocket sparks
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rainbow rocket sparks"; original source never consulted.

// A fiery rocket head loops toward the strip's start, trailed by a window of
// crackling white sparks. Stateless: everything is phase-shifted traveling
// square waves plus a static positional fire-hue ramp (the "exhaust" trick).

var phase = 0

export function beforeRender(delta) {
  phase = time(0.04)               // one traversal every ~2.6 s
}

export function render(index) {
  var p = index / pixelCount

  // spark window: traveling square wave, ~15% duty, moving toward index 0
  var inSparkZone = square(p + phase, 0.15) > 0
  // sparks re-roll every frame: ~1 in 20 pixels light inside the window
  var spark = inSparkZone && random(1) > 0.95

  // fire window: same phase, ~5% ahead of the sparks, a bit over half as wide
  var fire = square(p + phase + 0.05, 0.09) > 0

  // fire hue fixed to the strip: red->yellow ramp repeating ~8 times
  var hue = frac(p * 8) * 0.17

  // exactly one of: black, saturated fire, or pure white spark
  hsv(hue, fire ? 1 : 0, (fire || spark) ? 1 : 0)
}
