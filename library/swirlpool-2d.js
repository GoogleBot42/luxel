// name: Swirlpool 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Swirlpool 2D"; original source never consulted.

// A ring of bright dots orbits the center of a matrix, each leaving a
// comet trail, so the display reads as interlocking spiral arms. On a much
// slower, user-set timescale the arms' orbit centers glide between a
// spread arrangement and a collapsed knot through the middle. Arms are
// rainbow-tinted and the whole hue assignment drifts over time.
//
// Draw-into-persistent-buffer-and-decay technique: only one point per arm
// is plotted each frame into a virtual canvas; exponential decay plus the
// squared brightness at output make the trails (~half a second to black).
// Original assumed a square matrix sized sqrt(pixelCount); here the canvas
// is a fixed 16x16 virtual grid sampled by the mapped renderer, so any 2D
// map works.

const SIZE = 16
const CELLS = SIZE * SIZE
const DECAY = 0.93            // per frame — trail length varies with FPS,
                              // matching the original's quirk

var bright = array(CELLS)     // persistent canvas: brightness per cell
var hues = array(CELLS)       // persistent canvas: hue per cell

var arms = 5
var colorSpeed = 0.5
var animateSwirl = 0          // slider-as-toggle, see below
var swirlSpeed = 0.5

var huePhase = 0              // slowly advancing hue multiplier

//# min=2 max=16 step=1 default=5
export function sliderNumberOfArms(v) {
  arms = floor(2 + v * 14 + 0.5)
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderColorSpeed(v) {
  colorSpeed = v
}

// Only the far-left position enables the slow morph animation; anywhere
// else freezes the swirl phase at zero (arms stay spread). A real toggle
// would be cleaner, but this matches the original slider's semantics.
//# min=0 max=1 step=0.01 default=0
export function sliderAnimateSwirl(v) {
  animateSwirl = v
}

// Inverted: right = faster. Even at fastest the morph takes on the order
// of a minute per cycle.
//# min=0 max=1 step=0.01 default=0.5
export function sliderSwirlSpeed(v) {
  swirlSpeed = v
}

export function beforeRender(delta) {
  // Slow morph phase: 0 (spread) .. 1 (collapsed). Period ~1 to ~4
  // minutes depending on the (inverted) swirl-speed slider.
  var swirlPhase = 0
  if (animateSwirl < 0.01) {
    swirlPhase = triangle(time(1 + 3 * (1 - swirlSpeed)))
  }

  // Fade every canvas cell toward black.
  feedback(bright, DECAY)

  // Hue assignment drifts over tens of seconds; its rate is wobbled by a
  // ~2 minute triangle wave so the coloring never settles into a loop.
  var wobble = 0.5 + triangle(time(2))
  huePhase = mod(huePhase + delta * 0.001 * (0.02 + 0.08 * colorSpeed) * wobble, 1)

  // Shared fast rotation: one revolution in ~1.3 s.
  var angle = time(0.02) * PI2

  // As the morph phase rises, each arm's orbit-center offset scales from
  // +1 down through 0 to about -(sqrt(2) - 1): outward, inward, slightly
  // past center.
  var centerScale = 1 - swirlPhase * SQRT2

  for (var a = 0; a < arms; a++) {
    var off = a / arms * PI2
    var x = 0.5 + 0.22 * cos(angle + off) + 0.22 * cos(off) * centerScale
    var y = 0.5 + 0.22 * sin(angle + off) + 0.22 * sin(off) * centerScale
    var cell = floor(y * SIZE) * SIZE + floor(x * SIZE)
    if (cell >= 0 && cell < CELLS) {
      bright[cell] = 1
      // Product, not sum: small phase = all arms near red, large phase =
      // arms fanned across the spectrum. The rainbow opens and closes.
      hues[cell] = a / arms * huePhase
    }
  }
}

export function render2D(index, x, y) {
  var cell = floor(y * 15.99) * SIZE + floor(x * 15.99)
  var v = bright[cell]
  hsv(hues[cell], 1, v * v)
}
