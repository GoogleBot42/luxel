// name: fast pulse 3d
// Clean-room reimplementation from a prose functional description of the
// community pattern "fast pulse 3d"; original source never consulted.

// Sharp, narrow pulses sweep sinusoidally through the display — whipping
// through the middle of their travel, lingering at the extremes. Each has
// a white-hot core and a saturated fringe whose hue cycles through the
// whole rainbow over several seconds. In 3D the pulses are glowing planes
// whose orientation slowly tumbles (three mismatched sine oscillators act
// as the direction-vector components); 2D gets a flat slice of the same
// field; 1D gets racing bands. Fully deterministic, no per-frame state
// carried over.

// --- controls (defaults reproduce the original constants exactly: each
// scales its constant by a ratio that is 1 at the control's default) ------
var cycleSecs = 5.24    // master cycle: one hue revolution + one sweep, seconds
var sharpness = 5       // exponent on the pulse profile; higher = thinner
var pulses = 1          // pulse crests spanning the display
var hueSweep = 1        // fraction of the color wheel walked each cycle
var hueBase = 0         // where that walk starts

//# min=1 max=30 step=0.05 default=5.24
export function sliderCycleSeconds(v) { cycleSecs = max(v, 0.2) }

//# min=1 max=12 step=1 default=5
export function sliderSharpness(v) { sharpness = clamp(floor(v), 1, 12) }

//# min=1 max=8 step=1 default=1
export function sliderPulsesAcross(v) { pulses = max(floor(v), 1) }

//# min=0 max=1 step=0.05 default=1
export function sliderHueSweep(v) { hueSweep = clamp(v, 0, 1) }

// only the hue is used; the white core and fringe are the pattern's own
export function hsvPickerPulseColor(h, s, v) { hueBase = h }

var REF_CYCLE = 5.24

var t, ox, oy, oz, off1, off3, hue

export function beforeRender(delta) {
  var r = cycleSecs / REF_CYCLE
  t = time(0.08 * r)             // master phase: hue + motion, ~5.2 s
  hue = hueBase + t * hueSweep
  ox = sin(t * PI2)              // axis weights: sines with mismatched
  oy = sin(time(0.04 * r) * PI2) // periods (~half the master...
  oz = sin(time(0.053 * r) * PI2)// ...and ~two-thirds), so planes tumble
  off1 = sin(t * PI2) * 2        // sinusoidal sweep offset, 1D scale
  off3 = sin(t * PI2) * 3        // wider sweep through the 3D volume
}

export function render(index) {
  var v = triangle(off1 + index / pixelCount * pulses)  // folded moving crest
  v = pow(v, sharpness)          // 5th power: thin hard pulses, dark gaps
  hsv(hue, v < 0.9, v)           // top ~tenth desaturates: white-hot core
}

export function render3D(index, x, y, z) {
  // position term = projection onto the tumbling direction vector
  var v = triangle(off3 + (x * ox + y * oy + z * oz) * pulses)
  v = pow(v, sharpness)
  hsv(hue, v < 0.8, v)           // slightly more generous white core
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)       // flat matrices get a 2D slice
}
