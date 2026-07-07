// name: Audio Volume Meter
// Clean-room reimplementation from a prose functional description of the
// community pattern "Audio Volume Meter"; original source never consulted.

// Multi-section VU meter: the strip splits into five (or, via toggle,
// three) equal sections, each a bar graph of one frequency range. Outer
// sections fill inward from the strip ends, middle sections fill outward
// from their centers. Five-section band order, left to right: sub-bass,
// treble, mids, low-mids, mid-bass. A small PI auto-gain loop targets
// about a third of the strip lit on average; triple smoothing (refresh
// blend + per-frame decay + render blend) makes bars glide, not step.
// Idles dark when no audio is present.

export var frequencyData = array(32)

// ---- per-pixel layout lookups, precomputed from the actual pixelCount
var sec5 = array(pixelCount)   // section id 0..4
var frac5 = array(pixelCount)  // fill fraction within the section
var sec3 = array(pixelCount)   // section id 0..2
var frac3 = array(pixelCount)

// section id -> measurement index (m[] below)
var measMap5 = array(5)
measMap5[0] = 0  // sub-bass
measMap5[1] = 4  // treble
measMap5[2] = 3  // mids
measMap5[3] = 2  // low-mids
measMap5[4] = 1  // mid-bass
var measMap3 = array(3)
measMap3[0] = 0  // sub-bass
measMap3[1] = 5  // loudest of low-mid/mid/treble
measMap3[2] = 1  // mid-bass

function fillFraction(s, nSec, pos, len) {
  if (s == 0) return (pos + .5) / len                    // start -> end
  if (s == nSec - 1) return (len - pos - .5) / len       // end -> start
  return abs(pos + .5 - len / 2) / (len / 2)             // center outward
}

var i
for (i = 0; i < pixelCount; i++) {
  // five equal sections
  var s = floor(i * 5 / pixelCount)
  if (s > 4) s = 4
  var lo = ceil(s * pixelCount / 5)
  var hi = ceil((s + 1) * pixelCount / 5)
  sec5[i] = s
  frac5[i] = fillFraction(s, 5, i - lo, max(hi - lo, 1))

  // three sections: outer fifths kept, middle three merged
  var b1 = floor(pixelCount / 5)
  var b2 = pixelCount - b1
  var s3, lo3, hi3
  if (i < b1) { s3 = 0; lo3 = 0; hi3 = b1 }
  else if (i < b2) { s3 = 1; lo3 = b1; hi3 = b2 }
  else { s3 = 2; lo3 = b2; hi3 = pixelCount }
  sec3[i] = s3
  frac3[i] = fillFraction(s3, 3, i - lo3, max(hi3 - lo3, 1))
}

// ---- audio pipeline state
var pairs = array(16)
var m = array(6)          // smoothed display measurements
var prevV = array(pixelCount)
var refreshT = 0
var NOISE = .002          // quiet-room floor per raw bin
var SQUELCH = .02
var MAXE = 1.5            // max-energy cap for rescale

// ---- auto-gain (PI controller)
var sens = 1
var integ = 1
var accum = 0             // brightness emitted last frame
var TARGET = .33
var KP = 20
var KI = .05
var MAXSENS = 160

// ---- UI state
var staticColor = 0
var colorPos = .5
var bassG = .35, lowMidG = 1.2, midG = 1.2, trebleG = 4.5
var threeSec = 0
var blend = .6
var decayF = .95

export function toggleStaticColor(v) { staticColor = v }
//# min=0 max=1 step=0.01 default=0.5
export function sliderColor(v) { colorPos = v }
//# min=0 max=1 step=0.01 default=0.2
export function sliderBass(v) { bassG = .1 + v * 1.2 }
//# min=0 max=1 step=0.01 default=0.5
export function sliderLowMid(v) { lowMidG = .4 + v * 1.6 }
//# min=0 max=1 step=0.01 default=0.5
export function sliderMid(v) { midG = .4 + v * 1.6 }
//# min=0 max=1 step=0.01 default=0.5
export function sliderTreble(v) { trebleG = 1 + v * 7 }
export function toggleThreeSections(v) { threeSec = v }
//# min=0 max=1 step=0.01 default=0.6
export function sliderBlending(v) { blend = v * .95 }
//# min=0 max=1 step=0.01 default=0.5
export function sliderDecay(v) { decayF = .9 + v * .099 }
export function gaugeSensitivity() { return sens / MAXSENS }

// hue oscillators, computed per frame
var baseHue = 0, shimmer = 0, rainDrift = 0

function refreshMeasurements() {
  var k
  // pair adjacent bins (minus noise floor), scaled by current sensitivity
  for (k = 0; k < 16; k++) {
    var a = max(frequencyData[k * 2] - NOISE, 0)
    var b = max(frequencyData[k * 2 + 1] - NOISE, 0)
    pairs[k] = (a + b) / 2 * sens
  }
  // regional EQ: bass de-emphasized, treble boosted by default
  pairs[0] *= bassG
  pairs[1] *= bassG
  pairs[2] *= lowMidG
  for (k = 3; k < 8; k++) pairs[k] *= midG
  for (k = 8; k < 16; k++) pairs[k] *= trebleG
  // squelch, coarse quantize, rescale against the energy cap
  for (k = 0; k < 16; k++) {
    var v = pairs[k]
    if (v < SQUELCH) v = 0
    v = floor(v * 100) / 100
    pairs[k] = min(v / MAXE, 1)
  }
  // collapse to five measurements (max, not average, for the upper trio)
  var n0 = pairs[0]
  var n1 = pairs[1]
  var n2 = max(pairs[2], pairs[3])
  var n3 = pairs[4]
  for (k = 5; k < 8; k++) n3 = max(n3, pairs[k])
  var n4 = pairs[8]
  for (k = 9; k < 16; k++) n4 = max(n4, pairs[k])
  // temporal smoothing, previous value weighted 3:1
  m[0] = clamp((m[0] * 3 + n0) / 4, 0, 1)
  m[1] = clamp((m[1] * 3 + n1) / 4, 0, 1)
  m[2] = clamp((m[2] * 3 + n2) / 4, 0, 1)
  m[3] = clamp((m[3] * 3 + n3) / 4, 0, 1)
  m[4] = clamp((m[4] * 3 + n4) / 4, 0, 1)
  m[5] = max(m[2], max(m[3], m[4]))
}

export function beforeRender(delta) {
  // auto-gain from last frame's emitted brightness
  var err = TARGET - accum / pixelCount
  accum = 0
  integ = clamp(integ + err, 1, 3000)
  sens = max(KP * err + KI * integ, .2)

  // measurements decay every frame so bars slide down smoothly
  var k
  for (k = 0; k < 6; k++) m[k] *= decayF

  refreshT += delta
  if (refreshT > 40) {
    refreshT = 0
    refreshMeasurements()
  }

  baseHue = triangle(time(.35))          // slow back-and-forth hue sweep
  shimmer = .04 * wave(time(.15))        // per-section near-neighbor offsets
  rainDrift = time(.4)                   // drift for rainbow mode
}

export function render(index) {
  var s, fr, mi
  if (threeSec) {
    s = sec3[index]
    fr = frac3[index]
    mi = measMap3[s]
  } else {
    s = sec5[index]
    fr = frac5[index]
    mi = measMap5[s]
  }
  var target = fr < m[mi] ? 1 : 0
  // render-side blend: soft leading/trailing bar edges
  var v = prevV[index] * blend + target * (1 - blend)
  prevV[index] = v
  accum += v          // feed the auto-gain loop before clamping
  v = clamp(v, 0, 1)

  var h
  if (staticColor) {
    if (colorPos < .05) {
      h = fr + rainDrift              // rainbow along each bar
    } else {
      if (colorPos < .3) h = 0        // red
      else if (colorPos < .55) h = .33 // green
      else if (colorPos < .8) h = .66  // blue
      else h = .8                      // purple
      h += s * shimmer
    }
  } else {
    h = baseHue + s * shimmer + v * .03
  }
  hsv(h, 1, v)
}
