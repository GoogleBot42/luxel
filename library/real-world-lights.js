// name: Real World Lights
// Clean-room reimplementation from a prose functional description of the
// community pattern "Real World Lights"; original source never consulted.

// A single slider picks one of thirteen presets imitating real-world light
// sources. Most are steady solid colors; candle, uranium glass and
// Cherenkov shimmer with smooth summed-sine noise; the grow light shows
// static blue-to-purple bands. Structure: two parallel dispatch tables
// (per-frame setup, per-pixel renderer) indexed by the preset.

var NUM = 13
var preset = 0

//# min=0 max=1 step=0.01 default=0
export function sliderLightType(v) {
  preset = clamp(floor(v * NUM), 0, NUM - 1)
}

// shared color scratch
var cr = 0
var cg = 0
var cb = 0
var ch = 0
var cs = 0

// four drifting time phases for the summed-sine noise (~10 s full cycles)
var ph1 = 0
var ph2 = 0
var ph3 = 0
var ph4 = 0

export function beforeRender(delta) {
  ph1 = time(0.151)   // ~9.9 s
  ph2 = time(0.113)   // ~7.4 s
  ph3 = time(0.089)   // ~5.8 s
  ph4 = time(0.067)   // ~4.4 s
  var f = setups[preset]
  f()
}

export function render(index) {
  var f = renders[preset]
  f(index / pixelCount)
}

// Smooth 1D noise: four sine waves of position at non-harmonic spatial
// frequencies, each drifting at a slightly different rate. Result ~ -1..1.
// spd scales the drift speed per preset.
function noise1D(p, spd) {
  var n = sin((p * 13 + ph1 * spd) * PI2)
        + sin((p * 19 - ph2 * spd) * PI2)
        + sin((p * 29 + ph3 * spd) * PI2)
        + sin((p * 41 - ph4 * spd) * PI2)
  return n / 4
}

// Black-body color temperature (in hundreds of kelvins) to RGB, using the
// standard log/power-curve approximation of the black-body locus.
function blackBody(t) {
  if (t <= 66) {
    cr = 1
    cg = clamp(0.39008 * log(t) - 0.63184, 0, 1)
  } else {
    cr = clamp(1.29293 * pow(t - 60, -0.13320), 0, 1)
    cg = clamp(1.12989 * pow(t - 60, -0.07551), 0, 1)
  }
  if (t >= 66) cb = 1
  else if (t <= 19) cb = 0
  else cb = clamp(0.54321 * log(t - 10) - 1.19625, 0, 1)
}

// ---- per-frame setups ----------------------------------------------------

function setupNothing() { }
function setupWarmIncan() { blackBody(30) }    // ~3000 K
function setupSoftIncan() { blackBody(40) }    // ~4000 K
function setupCoolIncan() { blackBody(65) }    // ~6500 K
function setupHPS()      { ch = 0.045; cs = 1 }     // deep orange-amber
function setupMercury()  { ch = 0.42;  cs = 0.22 }  // pale minty green-white
function setupSodium()   { ch = 0.075; cs = 1 }     // strongly orange
function setupWarmFluor() { cr = 1;    cg = 0.87; cb = 0.82 }  // pinkish white
function setupCoolFluor() { cr = 0.85; cg = 0.93; cb = 1 }     // pale blue white
function setupUV()       { ch = 0.78;  cs = 1 }     // saturated violet-purple

// ---- per-pixel renderers ---------------------------------------------------

function renderSolidRGB(p) { rgb(cr, cg, cb) }
function renderSolidHS(p)  { hsv(ch, cs, 1) }

function renderCandle(p) {
  var n = noise1D(p, 3)                     // faster drift for flame flicker
  var v = max(0.35, (n + 1) / 2)            // floor ~1/3, never dark
  var h = 0.03 + triangle(n + p) * 0.035    // subtle shifts between oranges
  hsv(h, 1, v)
}

function renderUranium(p) {
  var n = noise1D(p, 1)
  hsv(0.26, 1, max(0.25, (n + 1) / 2))      // vivid yellow-green glow
}

function renderCherenkov(p) {
  var n = noise1D(p * 0.33, 1)              // stretched coord: broad shimmer
  hsv(0.60, 1, max(0.2, (n + 1) / 2))       // intense saturated blue
}

function renderGrowLight(p) {
  var w = sin(p * 36 * PI2)                 // several dozen cycles, static
  hsv(0.62 + w * w * 0.25, 1, 1)            // blue-to-violet/purple bands
}

// ---- dispatch tables -------------------------------------------------------

var setups = array(NUM)
var renders = array(NUM)
setups[0]  = setupNothing;   renders[0]  = renderCandle       // candlelight
setups[1]  = setupWarmIncan; renders[1]  = renderSolidRGB     // warm white
setups[2]  = setupSoftIncan; renders[2]  = renderSolidRGB     // soft white
setups[3]  = setupCoolIncan; renders[3]  = renderSolidRGB     // cool white
setups[4]  = setupNothing;   renders[4]  = renderUranium      // uranium glass
setups[5]  = setupHPS;       renders[5]  = renderSolidHS      // hp sodium
setups[6]  = setupMercury;   renders[6]  = renderSolidHS      // mercury vapor
setups[7]  = setupSodium;    renders[7]  = renderSolidHS      // sodium vapor
setups[8]  = setupWarmFluor; renders[8]  = renderSolidRGB     // warm fluor
setups[9]  = setupCoolFluor; renders[9]  = renderSolidRGB     // cool fluor
setups[10] = setupNothing;   renders[10] = renderGrowLight    // grow light
setups[11] = setupUV;        renders[11] = renderSolidHS      // uv blacklight
setups[12] = setupNothing;   renders[12] = renderCherenkov    // cherenkov
