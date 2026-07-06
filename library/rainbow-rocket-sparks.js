// name: Rainbow rocket sparks
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rainbow rocket sparks"; original source never consulted.

// A fire-colored rocket head sweeps toward the start of the strip, trailed by
// a window of furiously re-randomizing white sparks. No particle state at all:
// motion comes from phase-shifted traveling square waves, and the fire hue is
// a static positional ramp so the flame sheds "exhaust" as it passes.

var phase = 0

export function beforeRender(delta) {
  phase = time(0.04)   // one full traversal every ~2.6 s
}

export function render(index) {
  var p = index / pixelCount

  // spark window: traveling square wave, ~15% duty; adding the phase makes
  // it move toward the low-index end
  var inSparkZone = square(p + phase, 0.15)
  // sparks re-roll every frame: ~1-in-20 chance inside the window
  var spark = inSparkZone && random(1) > 0.95

  // fire window: same phase, offset ~5% ahead, a bit over half as wide
  var fire = square(p + phase + 0.05, 0.08)

  // static spatial hue ramp: red->yellow, repeating ~8x along the strip
  var h = frac(p * 8) * 0.16

  // fire = saturated flame color, spark = pure white, otherwise black
  hsv(h, fire ? 1 : 0, (fire || spark) ? 1 : 0)
}
