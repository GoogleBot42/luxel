// name: Real World Lights
// Clean-room reimplementation from a prose functional description of the
// community pattern "Real World Lights"; original source never consulted.

// One slider picks among thirteen presets imitating real-world light
// sources: three animated (candle, uranium glass, Cherenkov), one static
// spatial pattern (LED grow light), the rest steady solid colors.
// Dispatch is a pair of tables (per-frame setup fn, per-pixel render fn)
// indexed by the selected preset.

var NUM_MODES = 13
var mode = 0

// Shared scratch color (solid presets fill this once per frame)
var cr = 1, cg = 0.8, cb = 0.6

// Four drifting time phases for the animated presets (full cycles on the
// order of ten seconds)
var p1 = 0, p2 = 0, p3 = 0, p4 = 0

//# min=0 max=1 step=0.01 default=0
export function sliderLightType(v) {
  mode = clamp(floor(v * NUM_MODES), 0, NUM_MODES - 1)
}

// Black-body color temperature to RGB, kelvin given in hundreds
// (standard log/power-curve approximation of the black-body locus)
function blackBody(t) {
  if (t <= 66) {
    cr = 1
    cg = clamp((99.4708 * log(t) - 161.1196) / 255, 0, 1)
  } else {
    cr = clamp(329.6987 * pow(t - 60, -0.1332) / 255, 0, 1)
    cg = clamp(288.1222 * pow(t - 60, -0.0755) / 255, 0, 1)
  }
  if (t >= 66) cb = 1
  else if (t <= 19) cb = 0
  else cb = clamp((138.5177 * log(t - 10) - 305.0448) / 255, 0, 1)
}

// Smooth 1D value noise: four sine waves of normalized position at
// non-harmonic spatial frequencies, each drifting at a slightly
// different rate. Result roughly in -1..1.
function noise1D(p) {
  return (sin(p * 11 * PI2 + p1 * PI2) + sin(p * 17 * PI2 - p2 * PI2) +
          sin(p * 23 * PI2 + p3 * PI2) + sin(p * 29 * PI2 - p4 * PI2)) / 4
}

function setupNothing() {}
function renderSolidRGB(index) { rgb(cr, cg, cb) }

// --- preset setups / renderers ---

// 1. Candlelight: flickering warm orange; brightness floored around a
// third, hue nudged between orange tones by a triangle wave of noise+pos
function renderCandle(index) {
  var p = index / pixelCount
  var n = noise1D(p)
  var v = clamp(0.66 + 0.4 * n, 0.34, 1)
  var h = 0.02 + 0.03 * triangle(n + p)
  hsv(h, 0.95, v)
}

// 2-4. Incandescents share the black-body helper
function setupWarmIncandescent() { blackBody(28) }   // ~2800 K
function setupSoftIncandescent() { blackBody(40) }   // ~4000 K
function setupCoolIncandescent() { blackBody(65) }   // ~6500 K

// 5. Uranium-glass fluorescence: vivid yellow-green, slow brightness sway
function renderUranium(index) {
  var n = noise1D(index / pixelCount)
  hsv(0.24, 1, clamp(0.6 + 0.45 * n, 0.26, 1))
}

// 6-10. Steady discharge / fluorescent lamps
function setupHPSodium()   { hsv2rgbTo(0.05, 1, 1) }     // deep orange-amber
function setupMercury()    { hsv2rgbTo(0.42, 0.22, 1) }  // pale minty green-white
function setupSodium()     { hsv2rgbTo(0.09, 1, 1) }     // strongly orange
function setupWarmFluoro() { hsv2rgbTo(0.97, 0.12, 1) }  // pinkish white
function setupCoolFluoro() { hsv2rgbTo(0.6, 0.13, 1) }   // pale blue white

var hout = array(3)
function hsv2rgbTo(h, s, v) {
  hsv2rgb(h, s, v, hout)
  cr = hout[0]; cg = hout[1]; cb = hout[2]
}

// 11. LED grow light: static bands drifting between blue and violet/purple
function renderGrow(index) {
  var w = sin(index / pixelCount * 30 * PI2)
  hsv(0.63 + w * w * 0.3, 1, 1)
}

// 12. UV / black-light tube: saturated violet-purple
function setupUV() { hsv2rgbTo(0.77, 1, 0.9) }

// 13. Cherenkov radiation: intense saturated blue, broad slow shimmer
function renderCherenkov(index) {
  var n = noise1D(index / pixelCount * 0.33)  // stretched: broader shimmer
  hsv(0.63, 1, clamp(0.6 + 0.5 * n, 0.2, 1))
}

// --- dispatch tables ---
var setups = array(NUM_MODES)
var renders = array(NUM_MODES)
setups[0] = setupNothing;          renders[0] = renderCandle
setups[1] = setupWarmIncandescent; renders[1] = renderSolidRGB
setups[2] = setupSoftIncandescent; renders[2] = renderSolidRGB
setups[3] = setupCoolIncandescent; renders[3] = renderSolidRGB
setups[4] = setupNothing;          renders[4] = renderUranium
setups[5] = setupHPSodium;         renders[5] = renderSolidRGB
setups[6] = setupMercury;          renders[6] = renderSolidRGB
setups[7] = setupSodium;           renders[7] = renderSolidRGB
setups[8] = setupWarmFluoro;       renders[8] = renderSolidRGB
setups[9] = setupCoolFluoro;       renders[9] = renderSolidRGB
setups[10] = setupNothing;         renders[10] = renderGrow
setups[11] = setupUV;              renders[11] = renderSolidRGB
setups[12] = setupNothing;         renders[12] = renderCherenkov

var renderFn = renders[0]

export function beforeRender(delta) {
  // drift phases for the animated presets (~7-16 s per full cycle)
  p1 = time(0.11)
  p2 = time(0.16)
  p3 = time(0.13)
  p4 = time(0.24)

  var setupFn = setups[mode]
  setupFn()
  renderFn = renders[mode]
}

export function render(index) {
  renderFn(index)
}
