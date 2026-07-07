// name: Example: color hues
// Clean-room reimplementation from a prose functional description of the
// community pattern "Example: color hues"; original source never consulted.
//
// The color-focused sibling of the time-and-animation demo: full saturation,
// full brightness, and an array of tiny lambdas each mapping position to hue.
// Each mode is a static image; only the mode index animates.

var MODE_HOLD_MS = 800      // advance a bit less than once per second
var REPEATS = 4             // spatial value spans 0..4 across the strip

var numModes = 13
var modes = array(numModes)

// 1. hue equals position: repeated full rainbows
modes[0] = (p) => p
// 2. constant hue at the bottom of the range: red
modes[1] = (p) => 0
// 3. one-third around: green
modes[2] = (p) => 0.333
// 4. two-thirds around: blue
modes[3] = (p) => 0.667
// 5. top of the range wraps back to red
modes[4] = (p) => 1
// 6. narrow warm rainbow slivers with sharp sawtooth edges
modes[5] = (p) => p % 0.2
// 7. same hue span, smooth triangle transitions
modes[6] = (p) => triangle(p) * 0.2
// 8. same span, sine-eased (nonlinearly distributed)
modes[7] = (p) => wave(p) * 0.2
// 9. hard two-tone stripes between two cool hues
modes[8] = (p) => square(p, 0.5) * 0.5 + 0.333
// 10. fine multicolor banding: product of two waveforms at different rates
modes[9] = (p) => wave(p) * triangle(p * 3) * 0.2
// 11. layered texture with hue discontinuities, offset toward blue
modes[10] = (p) => (wave(p) * 0.5) % 0.3 - triangle(p) * 0.2 + 0.6
// 12. gradient plus its own coarse quantization error: stepped bands
modes[11] = (p) => (p + p % 0.25) * 0.3
// 13. symmetric gradient mirrored about the strip midpoint
modes[12] = (p) => abs(p - REPEATS / 2) * 0.5

var accum = 0
var mode = 0

export function beforeRender(delta) {
  accum += delta
  if (accum > MODE_HOLD_MS) {
    accum -= MODE_HOLD_MS
    mode = (mode + 1) % numModes
  }
  // mode = 9        // uncomment to pin one mode while studying it
}

export function render(index) {
  var p = REPEATS * index / pixelCount
  var f = modes[mode]
  hsv(f(p), 1, 1)   // hue wraps; saturation and brightness stay maxed
}
