// name: cube fire 3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "cube fire 3D"; original source never consulted.

// Roiling volumetric blobs of colored flame: the product of three phase-
// offset axis waves, overdriven so only coincident crests read as blobs.
// Three incommensurate time phases keep the motion from ever locking; the
// cell size slowly breathes; hue cycles globally with a gentle spatial
// gradient; the hottest cores bleach toward white.
// (The original declared sound-sensor bindings it never used — omitted here,
// as the spec allows.)

// --- controls (defaults reproduce the original constants exactly: every
// control scales its constant by a ratio that is 1 at the default) --------
var cycleSecs = 6.55   // period of the primary blob wave, seconds
var blobsAcross = 0.5  // mean wave cycles spanning the display
var heatPercent = 100  // gain on the blob intensity; 100 = as designed
var hueSweep = 1       // fraction of the color wheel walked each cycle
var hueBase = 0        // where that walk starts

//# min=2 max=30 step=0.05 default=6.55
export function sliderCycleSeconds(v) { cycleSecs = max(v, 0.5) }

//# min=0.25 max=4 step=0.05 default=0.5
export function sliderBlobsAcross(v) { blobsAcross = max(v, 0.05) }

//# min=20 max=200 step=5 default=100
export function sliderHeatPercent(v) { heatPercent = max(v, 1) }

//# min=0 max=1 step=0.05 default=1
export function sliderHueSweep(v) { hueSweep = clamp(v, 0, 1) }

// only the hue is used — saturation and value are the pattern's own
export function hsvPickerFlameColor(h, s, v) { hueBase = h }

var REF_CYCLE = 6.55
var REF_BLOBS = 0.5

var gain = 10
var t1 = 0
var t2 = 0
var t3 = 0
var breathe = 0.5

export function beforeRender(delta) {
  // three sawtooth phases in a ~10 : 13 : 8.5 ratio (several seconds each)
  var r = cycleSecs / REF_CYCLE
  t1 = time(0.10 * r)
  t2 = time(0.13 * r)
  t3 = time(0.085 * r)
  // cell size breathes between roughly one quarter and three quarters
  breathe = (0.25 + 0.5 * triangle(time(0.09))) * (blobsAcross / REF_BLOBS)
  gain = 10 * (heatPercent / 100)
}

export function render3D(index, x, y, z) {
  // slow global hue cycle plus a mild positional gradient
  var h = hueBase + t1 * hueSweep + (x + y + z) * 0.2

  // separable product of three axis waves, each drifted by a wave of its own
  // time phase; amplified ~10x so only coincident crests are visible
  var i = wave(x * breathe + wave(t1))
        * wave(y * breathe + wave(t2))
        * wave(z * breathe + wave(t3)) * gain

  // fringes stay saturated; past roughly twice unity the core bleaches
  // toward white, like heat
  var s = clamp(3 - i, 0, 1)

  // cubed brightness: hard black between blobs, blown-out cores clamp at full
  hsv(h, s, i * i * i)
}

// planar slice of the volume
export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}

// a line through the volume
export function render(index) {
  render3D(index, index / pixelCount, 0, 0)
}
