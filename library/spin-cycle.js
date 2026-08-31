// name: spin cycle
// Clean-room reimplementation from a prose functional description of the
// community pattern "spin cycle"; original source never consulted.

// About five sharp bright bands race along the strip on a several-second
// loop. Hues are folded into a half-wheel window whose position rotates
// steadily around the full wheel, while the hue striping density breathes
// between ~5 and ~10 repetitions.

var t1

// The five values marked (control) are driven by the sliders below; their
// values here are the untouched defaults. time(n) laps in n * 65.536 s.
var cycleInterval = 0.065  // (control) ~4.3 s loop
var bands = 5              // (control) bright bands along the strip
var hueReps = 5            // (control) hue repeats at the bottom of the breath
var hueWindow = 0.5        // (control) turns of wheel the hues fold into
var sharpness = 3          // (control) band-edge exponent

// Seconds for one full loop of the pattern.
//# min=0.5 max=30 step=0.1 default=4.3
export function sliderCycleSeconds(v) { cycleInterval = max(0.25, v) / 65.536 }

// How many bright bands are on the strip at once.
//# min=1 max=20 step=1 default=5
export function sliderBands(v) { bands = max(1, floor(v)) }

// Hue repetitions across the strip; the breathing doubles this at the top
// of each cycle.
//# min=1 max=20 step=1 default=5
export function sliderHueRepeats(v) { hueReps = max(1, floor(v)) }

// Degrees of the colour wheel the striping is folded into before the whole
// window rotates: 360 uses the full wheel, small values give a tight scheme.
//# min=30 max=360 step=15 default=180
export function sliderColorWindow(v) { hueWindow = clamp(v, 15, 360) / 360 }

// Band edge steepness: 1 is a soft triangle ramp, high values give narrow
// spikes with wide dark gaps.
//# min=1 max=6 step=1 default=3
export function sliderBandSharpness(v) { sharpness = clamp(floor(v), 1, 8) }

export function beforeRender(delta) {
  t1 = time(cycleInterval) // ~4.3 s cycle
}

export function render(index) {
  var p = index / pixelCount

  // Breathing repetition count (hueReps..2x hueReps) plus a scrolling offset,
  // folded into a colour window that itself rotates once per cycle.
  var h = p * (hueReps + hueReps * wave(t1)) + wave(t1) * 2
  h = h % hueWindow + t1

  // Triangular bands translating along the strip several times per cycle;
  // raised to `sharpness` for narrow punchy bars with dark gaps.
  var v = triangle(frac(p * bands + t1 * 10))
  var b = v
  for (var e = 1; e < sharpness; e++) b = b * v
  hsv(h, 1, b)
}
