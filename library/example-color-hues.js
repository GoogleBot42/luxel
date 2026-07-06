// name: Example: color hues
// Clean-room reimplementation from a prose functional description of the
// community pattern "Example: color hues"; original source never consulted.

// The color-focused sibling of the time/animation example. Every pixel is at
// full saturation and brightness; only hue varies, as a static function of
// position. Thirteen modes — tiny one-argument lambdas mapping position to
// hue — sit in an array and are dispatched by index; a timer advances the
// mode a bit less than once per second. Nothing animates within a mode.

var HOLD_MS = 800            // just under a second per mode
var ZOOM = 4                 // position-driven modes repeat ~4x along strip

var modeTimer = 0
var mode = 0

var NUM_MODES = 13
var modes = array(NUM_MODES)

// 1: hue = position — repeated full rainbows (hue wraps past 1)
modes[0] = (p) => p
// 2-5: constant hue anchors — red, green, blue, and top-of-range red again
modes[1] = (p) => 0
modes[2] = (p) => 1 / 3
modes[3] = (p) => 2 / 3
modes[4] = (p) => 1
// 6: narrow warm slivers with a hard sawtooth edge (~a fifth of the wheel)
modes[5] = (p) => p % 0.2
// 7: same span, smooth triangle transitions
modes[6] = (p) => triangle(p) * 0.2
// 8: same span, sine-eased (nonlinearly distributed)
modes[7] = (p) => wave(p) * 0.2
// 9: hard two-tone stripes between two cool hues
modes[8] = (p) => square(p, 0.5) * 0.5 + 0.33
// 10: fine multicolor banding — sine times a faster triangle, scaled small
modes[9] = (p) => wave(p) * triangle(p * 3) * 0.33
// 11: layered texture with hue discontinuities, offset toward blue
modes[10] = (p) => mod(wave(p) * 0.5 - triangle(p) * 0.3 + 0.66, 1)
// 12: gradient plus its own coarse modulus remainder — stepped "error" bands
modes[11] = (p) => (p + p % 0.25) * 0.5
// 13: absolute-value fold about the strip midpoint — mirrored gradient
modes[12] = (p) => abs(p - ZOOM / 2) * 0.5

export function beforeRender(delta) {
  modeTimer += delta
  if (modeTimer > HOLD_MS) {
    modeTimer -= HOLD_MS
    mode = (mode + 1) % NUM_MODES
  }
  // mode = 9   // uncomment to pin a single mode while studying it
}

export function render(index) {
  var p = ZOOM * index / pixelCount
  var f = modes[mode]
  hsv(f(p), 1, 1)            // hue does all the work; hsv wraps hue for us
}
