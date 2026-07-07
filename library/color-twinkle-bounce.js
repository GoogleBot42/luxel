// name: color twinkle bounce
// Clean-room reimplementation from a prose functional description of the
// community pattern "color twinkle bounce"; original source never consulted.
//
// Soft crests of light about a dozen pixels apart sway back and forth
// along the strip while the palette drifts through the rainbow. Stateless:
// every pixel is a pure function of (index, time).

var angle = 0
var hueBase = 0

export function beforeRender(delta) {
  angle = time(0.05) * PI2   // sway clock, ~3.3 s per cycle
  hueBase = time(0.1)        // rainbow drift, ~6.5 s per revolution
}

export function render(index) {
  // spatial sine over the raw pixel index (~0.5 rad/px -> ~12 px crests),
  // phase-swung a few radians by the clock: this is the bounce
  var s = sin(index / 2 + 3 * sin(angle))
  var w = (1 + s) / 2

  // fourth power sharpens broad humps into narrow twinkling crests;
  // peak brightness deliberately capped at half
  var v = w * w * w * w / 2

  // un-normalized sine added to the drifting ramp: hue spans the wheel
  // about twice per spatial wavelength, slowest at the crest centers
  hsv(hueBase + s, 1, v)
}
