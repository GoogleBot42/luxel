// name: sound - spectro kalidastrip
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - spectro kalidastrip"; original source never
// consulted.

// Music-reactive spectrum painted along the strip through a kaleidoscope
// fold: normalized position is folded twice with triangle waves (with a
// slowly scrolling phase between the folds), so the spectrum mirrors around
// moving fold points. Per band, the rolling average is subtracted from the
// live level, turning the display into an onset detector; hits flare, decay
// through a persistence buffer, and bloom toward white. A PI auto-gain
// controller targets roughly a fifth of the strip lit, so it works at any
// room volume without a knob. Idles dark in silence.

export var frequencyData = array(32)
export var energyAverage
export var maxFrequency
export var maxFrequencyMagnitude

var nBands = arrayLength(frequencyData)

var targetFill = 0.2   // aim: ~1/5 of the strip lit
var kp = 0.4           // proportional gain (a bit larger than integral)
var ki = 0.25          // integral gain
var errSum = 30        // accumulated error; starts moderately positive
var sensitivity = 10
var feedback = 0       // sum of clamped per-pixel brightness, last frame

var avg = array(nBands)          // sensitivity-scaled EMA per band
var pix = array(pixelCount)      // per-pixel persistence buffer (trails)
var scroll = 0

var bi
for (bi = 0; bi < nBands; bi++) avg[bi] = 0.001

export function beforeRender(delta) {
  // PI auto-gain from last frame's lit fraction
  var err = targetFill - feedback / pixelCount
  errSum = clamp(errSum + err, 0, 400)
  sensitivity = max(0, kp * err + ki * errSum)
  feedback = 0

  // frequency-to-position mapping slides over a couple of seconds
  scroll = time(2.2 / 65.536)

  // rolling average: EMA over ~1.5 s of sensitivity-scaled energy
  var alpha = min(1, delta / 1500)
  var i
  for (i = 0; i < nBands; i++) {
    var a = avg[i] + (frequencyData[i] * sensitivity - avg[i]) * alpha
    avg[i] = max(0.0002, a)
  }
}

export function render(index) {
  var pos = index / pixelCount

  // double triangle fold = the kaleidoscope; scroll slides the mirrors
  var band = triangle(triangle(pos * 2) + scroll) * (nBands - 1.001)
  var b0 = floor(band)
  var f = band - b0
  var b1 = b0 + 1
  if (b1 >= nBands) b1 = nBands - 1

  var live = frequencyData[b0] * (1 - f) + frequencyData[b1] * f
  var norm = avg[b0] * (1 - f) + avg[b1] * f

  // transient above the recent norm, boosted where the band is usually busy
  var boost = 0.1 + 3 * min(norm, 1)
  var v = (live * 3 * sensitivity - norm) * boost
  if (v < 0) v = 0
  v = min(v, 2)
  v = v * v // square for contrast: punchy peaks

  // fast attack, slow decay trails
  var p = pix[index] * 0.75 + v
  pix[index] = p

  feedback += min(p, 2) // gain-controller feedback, clamped small

  // rainbow keyed to frequency + a quarter-turn gradient along the strip
  var hue = band / nBands + pos * 0.25
  var sat = clamp(2 - p, 0, 1) // overdriven peaks whiten
  hsv(hue, sat, min(p, 1))
}
