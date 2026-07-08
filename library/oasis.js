// name: Oasis
// Clean-room reimplementation from a prose functional description of the
// community pattern "Oasis" (a Pacifica-inspired night-ocean effect);
// original source never consulted. Purely time-driven, no sensors.

// Soft aqua swells roll along the strip: four superimposed wave trains
// moving both directions at different speeds, crests washing toward white
// ("whitecaps"), troughs falling near-black. Scale-independent via a
// reference strip length so it looks the same on any pixel count.

var REF = 150            // reference strip length for normalization
var NUM = 4

var baseSpeed = array(NUM)   // sawtooth advance per second
var dir = array(NUM)         // +1 / -1 travel direction
var baseWL = array(NUM)      // wave cycles across the reference strip
var phase = array(NUM)       // current phase offset (0..1)
var wlCur = array(NUM)       // current (slider-scaled) wavelength divisor

// Brightness shaping LUT: a trough-started sine bump raised to the 4th power
// -> narrow bright crests, long dark troughs. Built once.
var LUTN = 256
var lut = array(LUTN)
var li
for (li = 0; li < LUTN; li++) {
  var s = (1 - cos((li / LUTN) * PI2)) / 2   // 0 at ends, peak at middle
  lut[li] = s * s * s * s
}

// ---- controls ----
var hueBase = 0.5        // aqua/teal
var speedMul = 1
var whitecaps = 1.2      // saturation threshold (>1 so only bright sums whiten)
var depth = 0.5
var wlMul = 1

function configure() {
  // Distinct hand-picked speeds/wavelengths; two each direction.
  baseSpeed[0] = 0.020; dir[0] =  1; baseWL[0] = 3
  baseSpeed[1] = 0.008; dir[1] = -1; baseWL[1] = 11
  baseSpeed[2] = 0.013; dir[2] =  1; baseWL[2] = 6
  baseSpeed[3] = 0.005; dir[3] = -1; baseWL[3] = 2
  var i
  for (i = 0; i < NUM; i++) {
    // higher wlMul -> longer waves -> fewer cycles -> smaller divisor
    wlCur[i] = baseWL[i] / wlMul
  }
}
configure()

//# min=0 max=1 step=0.01 default=0.5
export function sliderHue(v) { hueBase = v }

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  // inverted feel: higher = faster, ~4:1 range
  speedMul = 0.5 + v * 1.5
}

//# min=0 max=1 step=0.01 default=0.6
export function sliderWhitecaps(v) {
  // higher = whiter crests (lower threshold toward ~1)
  whitecaps = 1.6 - v * 0.6
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderDepth(v) { depth = v }

//# min=0 max=1 step=0.01 default=0.5
export function sliderWavelength(v) {
  // higher = longer waves, ~10:1 range; rebuilds layer config
  wlMul = 0.5 + v * 4.5
  configure()
}

var wl0eff = 3           // layer-0 wavelength with breathing applied
var hueMod = 0

export function beforeRender(delta) {
  var dt = delta / 1000
  var i
  for (i = 0; i < NUM; i++) {
    phase[i] = mod(phase[i] + baseSpeed[i] * speedMul * dir[i] * dt, 1)
  }
  // Slow triangle (~39 s cycle): subtle breathing of layer-0 wavelength and hue.
  var tri = triangle(time(0.6))
  wl0eff = wlCur[0] * (1 + (tri - 0.5) * 0.2)   // +/-10%
  hueMod = (tri - 0.5) * 0.04                   // +/-2% of the wheel
}

export function render(index) {
  var sum = 0
  var i
  for (i = 0; i < NUM; i++) {
    var off = phase[i] * pixelCount              // pixel-space phase offset
    var w = (i == 0) ? wl0eff : wlCur[i]
    // scale density with actual length so wavelength-in-pixels is constant
    var t = frac((index + off) * w / REF)
    sum += lut[floor(t * (LUTN - 0.01))]
  }
  var bri = sum / NUM

  var hue = hueBase + hueMod - bri * depth       // brighter water shifts hue
  var sat = whitecaps - bri                       // crests desaturate to foam

  hsv(hue, clamp(sat, 0, 1), clamp(bri, 0, 1))
}
