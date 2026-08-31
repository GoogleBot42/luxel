// name: color twinkle bounce
// Clean-room reimplementation from a prose functional description of the
// community pattern "color twinkle bounce"; original source never consulted.
//
// Soft crests of light about a dozen pixels apart sway back and forth
// along the strip while the palette drifts through the rainbow. Stateless:
// every pixel is a pure function of (index, time).
//
// Luxel look tweak (2026-08-31, Jeremy's review): crests now reach full
// brightness instead of being capped at half, and each crest holds one
// coherent color instead of smearing through two whole turns of the wheel —
// the default reads as bright candy-rainbow beads rather than a dim wash.
// Three fixed palettes sit alongside the drifting rainbow on a selector.

var angle = 0
var hueBase = 0

// Tunables.
var k = 0.5          // spatial frequency, rad/px (~12.6 px between crests)
var swayPx = 12      // sway excursion, peak-to-peak, in pixels
var swayRad = 3      // derived: sway amplitude in radians of spatial phase
var swayClock = 0.05 // time() interval for the sway, ~3.3 s
var hueClock = 0.1   // time() interval for the color drift, ~6.6 s

// Palette: base hue, how much of the wheel one crest spans, and how far the
// drifting clock carries the whole palette.
var palBase = 0
var palSpan = 0.35
var palCycle = 1

// 0 rainbow drift, 1 ember (red→gold), 2 lagoon (teal→blue),
// 3 bubblegum (violet→pink). Crest hue is palBase + palSpan; the dimmer
// shoulders of each crest trail back toward palBase.
//# min=0 max=3 step=1 default=0
export function sliderPalette(v) {
  var p = clamp(floor(v), 0, 3)
  if (p == 1) {
    palBase = 0; palSpan = 0.09; palCycle = 0.05
  } else if (p == 2) {
    palBase = 0.36; palSpan = 0.16; palCycle = 0.1
  } else if (p == 3) {
    palBase = 0.7; palSpan = 0.16; palCycle = 0.18
  } else {
    palBase = 0; palSpan = 0.35; palCycle = 1
  }
}

// Distance between crests, in pixels.
//# min=4 max=40 step=1 default=13
export function sliderCrestSpacingPixels(v) { k = PI2 / clamp(v, 4, 40) }

// How far the whole comb of crests swings, peak-to-peak, in pixels.
//# min=0 max=40 step=1 default=12
export function sliderSwayWidthPixels(v) { swayPx = clamp(v, 0, 40) }

// Seconds for one complete there-and-back sway.
//# min=0.5 max=20 step=0.1 default=3.3
export function sliderBounceSeconds(v) { swayClock = max(v, 0.2) / 65.536 }

// Seconds for the palette to travel once around its color range.
//# min=1 max=120 step=1 default=6.6
export function sliderColorCycleSeconds(v) { hueClock = max(v, 0.5) / 65.536 }

export function beforeRender(delta) {
  angle = time(swayClock) * PI2   // sway clock
  hueBase = time(hueClock)        // palette drift
  // pixels of excursion -> radians of spatial phase (half of peak-to-peak)
  swayRad = swayPx * 0.5 * k
}

export function render(index) {
  // spatial sine over the raw pixel index, phase-swung by the clock: the bounce
  var s = sin(index * k + swayRad * sin(angle))
  var w = (1 + s) / 2

  // fourth power sharpens broad humps into narrow twinkling crests
  var v = w * w * w * w

  // hue: palette base + the drifting clock + a slice of the wheel across the
  // crest (slowest at the crest centers, where brightness peaks)
  hsv(palBase + palCycle * hueBase + palSpan * s, 1, v)
}
