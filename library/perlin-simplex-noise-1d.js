// name: Perlin/Simplex Noise 1D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Perlin/Simplex Noise 1D"; original source never consulted.

// The strip is a profile slice through a 2D noise landscape. The slice is
// normalized into a per-pixel heightmap, and colored stripes sweep along
// contour lines of equal altitude: narrow and fast on steep terrain, wide
// and slow on flat terrain. Optional slow panning drifts the viewport
// through the noise field, and an optional bass-detection mode makes the
// stripes lurch forward on musical hits.

// sound inputs (engine stubs these with zeros when no sensor board)
export var frequencyData = array(32)
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0

var NOISE_SEED = 1337
var NOISE_ROW = 1.7    // slice offset; see rebuildMap()
var BASS_GAIN = 1.45   // spectrum-sum gain; see beforeRender()

// --- control state (defaults mirror the //# bounds) ---
var useSimplex = 0     // noise type: 0 = square-grid perlin, 1 = simplex
var scale = 3          // noise cells across the strip (~1..5)
var motion = 0         // viewport panning amount (0 = static terrain)
var autoColor = 1      // 1 = rotating rainbow stripes, 0 = fixed fire hues
var palOffset = 0.07   // hue rotation for auto-color mode
var stripes = 3        // simultaneous sweeping stripes (1..5)
var stripeSpeed = 1    // contour flow rate
var subStripes = 1     // sub-stripe blanking divisor (1 = solid stripes)
var xOffset = 0        // manual viewport scrub
var showProgress = 0   // white sweep-phase indicator overlay
var bassThresh = 0     // 0 disables sound reactivity

//# min=0 max=1 step=1 default=0
export function sliderPerlinOrSimplex(v) { useSimplex = v > 0.5; needRecalc = 1 }
//# min=0 max=1 step=0.01 default=0.5
export function sliderScale(v) { scale = 1 + v * 4; needRecalc = 1 }
//# min=0 max=1 step=0.01 default=0
export function sliderMotion(v) { motion = v }
//# min=0 max=1 step=1 default=1
export function sliderAutoColor(v) { autoColor = v > 0.5 }
//# min=0 max=1 step=0.01 default=0.07
export function sliderAutoColorPalette(v) { palOffset = v }
//# min=0 max=1 step=0.25 default=0.5
export function sliderNumberOfStripes(v) { stripes = 1 + floor(v * 4) }
//# min=0 max=1 step=0.01 default=1
export function sliderStripeSpeed(v) { stripeSpeed = v }
//# min=0 max=1 step=0.25 default=1
export function sliderStripeWeight(v) { subStripes = 5 - floor(v * 4) }
//# min=0 max=1 step=0.01 default=0
export function sliderX_Offset(v) { xOffset = v; needRecalc = 1 }
//# min=0 max=1 step=1 default=0
export function sliderShowProgress(v) { showProgress = v > 0.5 }
//# min=0 max=1 step=0.01 default=0
export function sliderBassThreshold(v) { bassThresh = v }

// --- heightmap state ---
var heights = array(pixelCount)
var mn = 0, rng = 1
var needRecalc = 1

// --- sweep / sound state ---
var phase = 0        // stripe-sweep sawtooth, 0..1
var burstOn = 0      // bass burst latched?
var burstAge = 0     // seconds the current burst has run

function rebuildMap(panOffset) {
  var lo = 32000
  var hi = -32000
  for (var i = 0; i < pixelCount; i++) {
    var nx = (i / pixelCount - panOffset - xOffset) * scale
    var v
    if (useSimplex) {
      v = (simplex2(nx, NOISE_ROW, NOISE_SEED) + 1) / 2
    } else {
      // NOISE_ROW must stay off the lattice: perlin() sampled at y=z=0 is
      // exactly mirror-symmetric in x, which folds the whole strip in half.
      v = (perlin(nx, NOISE_ROW, 0, NOISE_SEED) + 1) / 2
    }
    heights[i] = v
    if (v < lo) lo = v
    if (v > hi) hi = v
  }
  mn = lo
  rng = hi - lo
  if (rng < 0.001) rng = 1  // flat map guard (e.g. degenerate slice)
}

export function beforeRender(delta) {
  var dt = delta / 1000

  // sweep clock: one full sweep in 1.30 s at full speed with the default
  // three stripes. Period is proportional to the stripe count and exactly
  // inversely proportional to the speed dial (0 freezes the sweep).
  phase += dt * stripeSpeed * 2.31 / stripes
  phase = mod(phase, 1)

  // bass reactivity: threshold at zero disables the whole path, and an
  // all-zero (stubbed) spectrum can never trigger it
  if (bassThresh > 0) {
    // Gain calibrates the bass sum against the 0..1 threshold dial: a beat
    // has to clear the TOP of the dial even when the frame rate samples the
    // spectrum off the envelope's peak, otherwise high thresholds go inert
    // at low frame rates.
    var bass = (frequencyData[0] + frequencyData[1] + frequencyData[2]) * BASS_GAIN
    if (!burstOn && bass > bassThresh) {
      burstOn = 1
      burstAge = 0
    }
    if (burstOn) {
      // fast-forward the stripes for a short fixed burst
      phase = mod(phase + dt * (0.5 + stripeSpeed * 2), 1)
      burstAge += dt
      if (burstAge > 0.25) burstOn = 0
    }
  }

  // very slow sinusoidal pan (period on the order of minutes)
  var pan = 0
  if (motion > 0) {
    pan = sin(time(2) * PI2) * motion / scale
    needRecalc = 1
  }

  if (needRecalc) {
    rebuildMap(pan)
    needRecalc = 0
  }
}

export function render(index) {
  var alt = (heights[index] - mn) / rng   // normalized altitude, 0..1
  var d = mod(alt - phase, 1)

  var h, s = 1
  if (autoColor) {
    // one solid hue per sweeping stripe, rotatable by the palette offset
    h = floor(d * stripes) / stripes - palOffset
  } else {
    // fixed fire scheme: three stripes, red / deep orange / yellow-orange
    var band = floor(d * 3)
    h = band == 0 ? 0 : (band == 1 ? 0.03 : 0.09)
    d = mod(alt - phase, 1)  // (three stripes forced below via slots)
  }

  var nStripes = autoColor ? stripes : 3
  var slots = nStripes * subStripes
  var v = triangle(d * slots)
  v = v * v
  // sub-stripe gate: blank all but one slot per stripe (thinner stripes)
  if (mod(floor(d * slots), subStripes) != 0) v = 0

  // optional white sweep-position indicator
  if (showProgress) {
    var dp = abs(index - phase * pixelCount)
    if (dp < 2) {
      h = 0
      s = 0
      v = 1 - dp / 2
    }
  }

  hsv(h, s, v)
}
