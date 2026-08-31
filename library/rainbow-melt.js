// name: Rainbow Melt
// Curated example (hand-written showcase of the Luxel language/builtins).
// Clean-room reimplementation from a prose description of the community
// pattern "rainbow melt" (no source consulted). A mirrored rainbow that
// slowly rotates while brightness churns in fluid waves — made by nesting
// a sinusoid inside itself, each stage phase-modulated by the clock, over
// two near-but-not-equal periods so it never seems to repeat.
// Tunables — the top-level values are the constants this pattern shipped with,
// so an untouched render is unchanged.
var iMelt = 0.1        // time() interval of the melt churn (~6.5 s)
var iColor = 0.13      // time() interval of the palette rotation (~8.5 s)
var repeats = 1        // mirrored rainbows along the strip
var spread = 1         // hue span across one repeat, fraction of the wheel

// Seconds for the brightness churn to complete one cycle.
//# min=1 max=30 step=0.5 default=6.5
export function sliderMeltSeconds(v) { iMelt = max(v, 0.5) / 65.536 }

// Seconds for the palette to rotate once around the color wheel.
//# min=1 max=60 step=0.5 default=8.5
export function sliderColorCycleSeconds(v) { iColor = max(v, 0.5) / 65.536 }

// How many mirrored rainbows fit along the strip.
//# min=1 max=8 step=1 default=1
export function sliderRepeats(v) { repeats = clamp(floor(v), 1, 16) }

// How much of the color wheel one repeat spans, as a percentage: 100 is the
// full rainbow, small values give a single slowly drifting color.
//# min=0 max=100 step=1 default=100
export function sliderColorSpreadPercent(v) { spread = clamp(v, 0, 100) / 100 }

export function beforeRender(delta) {
  t1 = time(iMelt)
  t2 = time(iColor)
}

export function render(index) {
  // 1 at each repeat's center, 0 at its ends (identical to 1 - |2p - 1| when
  // repeats is 1)
  c = 1 - abs(mod(2 * index * repeats / pixelCount, 2) - 1)
  v = wave(wave(wave(c) + t1) + t1)
  hsv(c * spread + t2, 1, v * v)
}
