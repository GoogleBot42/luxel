// name: Color Blend
// Clean-room reimplementation from a prose functional description of the
// community pattern "Color Blend"; original source never consulted.
//
// Three smooth waves - one red, one green, one blue - drift along the strip
// at independent signed speeds and mix additively into constantly shifting
// blends (near-white where all peak, near-black where all trough). Sliders
// set wavelength count (reversed) and per-channel speed/direction (reciprocal
// mapping, center = stopped); toggles enable each channel. Gamma is a square.

var spread = 10          // wavelengths across the strip (default ~10)
var rateR = 0.065        // signed cycles/sec per channel
var rateG = 0.083
var rateB = -0.057       // one channel runs counter to the others
var onR = 1
var onG = 1
var onB = 1

var clk = 0
var phR = 0
var phG = 0
var phB = 0

export function beforeRender(delta) {
  clk = clk + delta / 1000
  phR = mod(clk * rateR, 1)   // phases wrapped to the unit interval
  phG = mod(clk * rateG, 1)
  phB = mod(clk * rateB, 1)
}

export function render(index) {
  var pos = (index / pixelCount) * spread
  var r = wave(pos + phR) * onR
  var g = wave(pos + phG) * onG
  var b = wave(pos + phB) * onB
  // gamma approximation: square each channel (fast, close enough)
  rgb(r * r, g * g, b * b)
}

// reciprocal speed mapping: center stopped, ramps sharply toward the ends;
// guards the exact center against divide-by-zero
function speedFromSlider(v) {
  var d = v - 0.5
  if (abs(d) < 0.01) return 0
  var denom = max(0.5 - abs(d), 0.001)
  return sign(d) * 0.05 / denom
}

// reversed: slider up = broader (down to 1 wavelength), down = many tight bands
//# min=0 max=1 step=0.01 default=0.77
export function sliderSpread(v) { spread = 1 + (1 - v) * 39 }

//# min=0 max=1 step=0.01 default=0.62
export function sliderRedSpeed(v) { rateR = speedFromSlider(v) }

//# min=0 max=1 step=0.01 default=0.7
export function sliderGreenSpeed(v) { rateG = speedFromSlider(v) }

//# min=0 max=1 step=0.01 default=0.35
export function sliderBlueSpeed(v) { rateB = speedFromSlider(v) }

//# min=0 max=1 step=1 default=1
export function toggleRedOn(v) { onR = v }

//# min=0 max=1 step=1 default=1
export function toggleGreenOn(v) { onG = v }

//# min=0 max=1 step=1 default=1
export function toggleBlueOn(v) { onB = v }
