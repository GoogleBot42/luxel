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

var relSpeed = array(NUM)    // travel speed of each train, relative to train 0
var dir = array(NUM)         // +1 / -1 travel direction
var baseWL = array(NUM)      // wave cycles across the reference strip
var phase = array(NUM)       // current phase offset (0..1 of a strip length)
var wlCur = array(NUM)       // current (slider-scaled) wavelength divisor
var wEff = array(NUM)        // divisor actually in force this frame
var wlPx = array(NUM)        // that divisor's wavelength, in pixels
var phOffPx = array(NUM)     // phase offset in pixels, reduced to one wavelength

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
var speedPx = 10         // travel speed of the fastest train, pixels/second
var whitecaps = 1.2      // saturation threshold (>1 so only bright sums whiten)
var depth = 0.5
var wlMul = 1

function configure() {
  // Distinct hand-picked speeds/wavelengths; two each direction.
  relSpeed[0] = 1.00; dir[0] =  1; baseWL[0] = 3
  relSpeed[1] = 0.40; dir[1] = -1; baseWL[1] = 11
  relSpeed[2] = 0.65; dir[2] =  1; baseWL[2] = 6
  relSpeed[3] = 0.25; dir[3] = -1; baseWL[3] = 2
  var i
  for (i = 0; i < NUM; i++) {
    // higher wlMul -> longer waves -> fewer cycles -> smaller divisor
    wlCur[i] = baseWL[i] / wlMul
  }
}
configure()

//# min=0 max=1 step=0.01 default=0.5
export function sliderHue(v) { hueBase = v }

// Swell travel speed in PIXELS PER SECOND. The old dial was a 0.5x..2x multiplier
// on a hard-coded rate that worked out to ~1.5 px/s at 60 px, so every setting
// was below the ~8-bit motion floor (the whole range read as a still image) and
// the actual speed scaled with strip length. Real units fix both.
//# min=0 max=40 step=0.5 default=10
export function sliderSpeed(v) { speedPx = v }

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
  // Slow triangle (~39 s cycle): subtle breathing of layer-0 wavelength and hue.
  var tri = triangle(time(0.6))
  wl0eff = wlCur[0] * (1 + (tri - 0.5) * 0.2)   // +/-10%
  hueMod = (tri - 0.5) * 0.04                   // +/-2% of the wheel

  var i
  for (i = 0; i < NUM; i++) {
    // phase is a fraction of a strip length, so advancing it by speed/pixelCount
    // moves the train exactly `speedPx * relSpeed` pixels per second on any rig.
    phase[i] = mod(phase[i] + speedPx * relSpeed[i] * dir[i] * dt / pixelCount, 1)
    wEff[i] = (i == 0) ? wl0eff : wlCur[i]
    wlPx[i] = REF / wEff[i]                        // one wave, in pixels
    // Reduce the pixel-space offset (phase * pixelCount, up to a whole strip)
    // to a single wavelength once per frame. Carrying it at full size into
    // render() and multiplying by the divisor there is what left 16.16 range on
    // long strips, wrapping negative and indexing lut[] below zero (Gitea #197).
    phOffPx[i] = mod(phase[i] * pixelCount, wlPx[i])
  }
}

export function render(index) {
  var sum = 0
  var i
  for (i = 0; i < NUM; i++) {
    // Both terms are under one wavelength, so the product with the divisor
    // stays under 2 * REF however long the strip is.
    var t = frac((mod(index, wlPx[i]) + phOffPx[i]) * wEff[i] / REF)
    sum += lut[floor(t * (LUTN - 0.01))]
  }
  var bri = sum / NUM

  var hue = hueBase + hueMod - bri * depth       // brighter water shifts hue
  var sat = whitecaps - bri                       // crests desaturate to foam

  hsv(hue, clamp(sat, 0, 1), clamp(bri, 0, 1))
}
