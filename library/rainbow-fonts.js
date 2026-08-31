// name: rainbow fonts
// Clean-room reimplementation from a prose functional description of the
// community pattern "rainbow fonts"; original source never consulted.

// A smoothly animated rainbow, mirror-symmetric about the strip midpoint.
// A folded distance-from-center ramp is passed through a sine-shaped wave,
// phase-shifted, and sine-folded again — so the hue bands compress and
// expand as the phase slides.
//
// Luxel look tweak (2026-08-31, Jeremy's review): the original (and the
// port that copied it) rendered at a fixed ~1/3 brightness, which washed the
// colors out. Full brightness is now the default, so the rainbow reads as
// vivid; the Saturation control is there to walk it back toward pastel.

var phase = 0
var cycleClock = 0.1   // time() interval, ~6.5 s per rainbow cycle
var repeats = 1        // full rainbows packed into one fold
var sat = 1            // color saturation
var mirror = 1         // 1 = fold about the midpoint, 0 = plain end-to-end ramp

// Seconds for one full animation cycle.
//# min=0.5 max=60 step=0.5 default=6.5
export function sliderCycleSeconds(v) { cycleClock = max(v, 0.2) / 65.536 }

// How many complete rainbows the hue ramp packs into the strip.
//# min=1 max=6 step=1 default=1
export function sliderRainbowRepeats(v) { repeats = clamp(floor(v), 1, 6) }

// Color saturation; below 100% the rainbow goes pastel toward white.
//# min=0 max=100 step=1 default=100
export function sliderSaturationPercent(v) { sat = clamp(v, 0, 100) / 100 }

// Mirror the rainbow about the strip midpoint (off: it sweeps end to end).
//# default=1
export function toggleMirror(on) { mirror = on > 0.5 }

export function beforeRender(delta) {
  phase = time(cycleClock)
}

export function render(index) {
  var mid = (pixelCount - 1) / 2
  // 1 at the strip midpoint, falling linearly to 0 at both ends...
  var c = 1 - abs(index - mid) / mid
  // ...or a plain 0..1 ramp when the mirror is off
  if (!mirror) c = index / (pixelCount - 1)
  // double sine-fold: ramp -> wave -> add phase -> wave -> hue (hsv wraps hue,
  // so multiplying by the repeat count simply lays down more rainbows)
  var h = repeats * wave(wave(c) + phase)
  hsv(h, sat, 1)
}
