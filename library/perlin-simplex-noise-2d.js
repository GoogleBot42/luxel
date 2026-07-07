// name: Perlin/Simplex Noise 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Perlin/Simplex Noise 2D"; original source never consulted.

// A smooth gradient-noise heightfield covers the matrix; colored contour
// stripes (bands of equal "elevation") sweep continuously from low ground to
// high ground and wrap. Auto-color gives each stripe its own hue; the
// alternate mode uses a fixed three-color fire palette. Optional slow
// circular camera panning drifts the terrain, and bass hits (32-band audio
// spectrum) kick the stripe flow forward in short fast-forward bursts.
//
// The heightfield is cached on a 16x16 virtual canvas and only recomputed
// when a control dirties it (or every frame while panning), with observed
// min/max tracked so the render always spans the full 0..1 range.

var W = 16
var H = 16
var field = array(W * H)
var fMin = 0
var fInvRange = 1
var dirty = 1
var SEED = 37.62

// sound inputs (engine stubs these with zeros when no sensor board)
export var frequencyData = array(32)
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0

// ---- controls (all sliders) ----------------------------------------------
var noiseType = 0.7    // <0.5 classic perlin grid, >=0.5 simplex
var zoomCtl = 0.35
var motionCtl = 0.3
var autoColor = 1      // >=0.5 rainbow-per-stripe, else fixed fire palette
var paletteOfs = 0
var stripesCtl = 0.5
var speedCtl = 0.5
var weightCtl = 0.8
var xOfs = 0
var yOfs = 0
var showProgress = 0
var bassThresh = 0

//# min=0 max=1 step=0.01 default=0.7
export function sliderNoiseType(v) { noiseType = v; dirty = 1 }
//# min=0 max=1 step=0.01 default=0.35
export function sliderZoom(v) { zoomCtl = v; dirty = 1 }
//# min=0 max=1 step=0.01 default=0.3
export function sliderMotion(v) { motionCtl = v }
//# min=0 max=1 step=0.01 default=1
export function sliderAutoColor(v) { autoColor = v }
//# min=0 max=1 step=0.01 default=0
export function sliderPaletteOffset(v) { paletteOfs = v }
//# min=0 max=1 step=0.01 default=0.5
export function sliderNumberOfStripes(v) { stripesCtl = v }
//# min=0 max=1 step=0.01 default=0.5
export function sliderStripeSpeed(v) { speedCtl = v }
//# min=0 max=1 step=0.01 default=0.8
export function sliderStripeWeight(v) { weightCtl = v }
//# min=0 max=1 step=0.01 default=0
export function sliderXOffset(v) { xOfs = v; dirty = 1 }
//# min=0 max=1 step=0.01 default=0
export function sliderYOffset(v) { yOfs = v; dirty = 1 }
//# min=0 max=1 step=1 default=0
export function sliderShowProgress(v) { showProgress = v }
//# min=0 max=1 step=0.01 default=0
export function sliderBassThreshold(v) { bassThresh = v }
// ---------------------------------------------------------------------------

var panX = 0
var panY = 0

function recalcField() {
  var zoom = 1.5 + zoomCtl * 8            // larger = finer-grained terrain
  var mn = 32000
  var mx = -32000
  var r, c
  for (r = 0; r < H; r++) {
    for (c = 0; c < W; c++) {
      var nx = (c / W - panX - xOfs * 2) * zoom
      var ny = (r / H - panY - yOfs * 2) * zoom
      var v
      if (noiseType < 0.5) v = perlin(nx, ny, 0, SEED)
      else v = simplex2(nx, ny, SEED)
      field[r * W + c] = v
      if (v < mn) mn = v
      if (v > mx) mx = v
    }
  }
  fMin = mn
  if (mx - mn > 0.0001) fInvRange = 1 / (mx - mn)
  else fInvRange = 0
  dirty = 0
}

var phase = 0          // stripe sweep phase, 0..1
var burstMs = 0        // remaining fast-forward time after a bass hit
var stripeCount = 3
var slots = 1

export function beforeRender(delta) {
  stripeCount = 1 + floor(stripesCtl * 5.99)
  if (autoColor < 0.5) stripeCount = 3   // fixed palette forces three stripes

  // stripe flow: ~0.3 s .. several s per stripe-spacing, scaled by count
  var rate = (0.03 + speedCtl * speedCtl * 3) / stripeCount
  phase = frac(phase + delta * rate / 1000)

  // sound: sum the low (bass) bands against the threshold; zero disables
  if (bassThresh > 0) {
    var bass = frequencyData[0] + frequencyData[1] + frequencyData[2] + frequencyData[3]
    if (bass > bassThresh) burstMs = 200
    if (burstMs > 0) {
      burstMs = max(0, burstMs - delta)
      phase = frac(phase + delta * (0.5 + speedCtl * 3) / 1000)
    }
  }

  // glacial circular camera panning, amplitude balanced against zoom
  if (motionCtl > 0.01) {
    var orbit = time(2) * PI2               // ~131 s per orbit
    var amp = motionCtl * 0.6 / (1.5 + zoomCtl * 8)
    panX = sin(orbit) * amp
    panY = cos(orbit) * amp
    dirty = 1
  }

  // stripe thinning: sub-slots per stripe; all but one get blanked
  slots = 1 + floor((1 - weightCtl) * 3.99)

  if (dirty) recalcField()
}

export function render2D(index, x, y) {
  var n = (field[floor(y * 15.99) * 16 + floor(x * 15.99)] - fMin) * fInvRange
  var q = n - phase

  // contour brightness: triangle over stripe*slot cells, blanking gate, gamma
  var v = triangle(q * stripeCount * slots)
  if (mod(floor(q * stripeCount * slots), slots) != 0) v = 0
  v = v * v

  var h
  if (autoColor >= 0.5) {
    // one flat hue per stripe, spread over most of the wheel, then rotated
    h = frac(floor(mod(q, 1) * stripeCount) / stripeCount * 0.75 + paletteOfs)
  } else {
    // fire: deep red / orange / amber by thirds of the sweep cycle
    var third = mod(q, 1)
    h = 0 + 0.045 * (third >= 0.3333) + 0.045 * (third >= 0.6667)
  }

  // optional white sweep-phase marker on the bottom row
  if (showProgress >= 0.5 && floor(y * 15.99) == 15) {
    var pb = 1 - abs(x - phase) * 8
    if (pb > v) {
      hsv(0, 0, clamp(pb, 0, 1))
      return
    }
  }

  hsv(h, 1, clamp(v, 0, 1))
}
