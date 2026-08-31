// name: sound - spectromatrix render2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - spectromatrix render2D"; original source never consulted.

// Sensor bindings (engine stubs these with zeros when no sensor board)
export var frequencyData = array(32)
export var energyAverage
export var maxFrequency
export var maxFrequencyMagnitude

var NBANDS = 32
var TOPBAND = NBANDS - 1

// per-band rolling average of the gain-scaled spectrum (~1.5 s EMA)
var bandAvg = array(NBANDS)
// per-pixel brightness persistence buffer (16x16 virtual canvas)
var persist = array(256)

// automatic gain: PI controller on emitted brightness
var TARGET = 0.05          // want ~5% average lit fraction
var integral = 10
var gain = 10
var briAccum = 0

var slowPhase = 0
var fastPhase = 0

export function beforeRender(delta) {
  slowPhase = time(0.18)   // spatial drift + hue rotation, ~12 s
  fastPhase = time(0.07)   // band-scan drift, ~4.6 s

  // PI update against the brightness actually emitted last frame
  var emitted = briAccum / pixelCount
  briAccum = 0
  var err = TARGET - emitted
  integral = clamp(integral + err * delta * 0.02, 0.05, 1000)
  gain = max(0, integral + err * 30)

  // fold the new spectrum frame into the rolling averages
  var k = clamp(delta / 1500, 0, 1)
  for (var i = 0; i < NBANDS; i++) {
    var v = bandAvg[i] * (1 - k) + frequencyData[i] * gain * k
    bandAvg[i] = max(0.00005, v)   // tiny floor so ratio math never /0
  }
}

export function render2D(index, x, y) {
  var slot = floor(y * 15.99) * 16 + floor(x * 15.99)

  // drifting plasma-like iso-bands mapped onto the spectrum
  var band = (wave(x + slowPhase) + wave(y - slowPhase)) / 2 + fastPhase
  band = triangle(band) * TOPBAND

  var b0 = floor(band)
  var f = band - b0
  var b1 = min(b0 + 1, TOPBAND)
  var inst = (frequencyData[b0] * (1 - f) + frequencyData[b1] * f) * gain
  var av = bandAvg[b0] * (1 - f) + bandAvg[b1] * f

  // transient detector: only above-average energy shows; dividing by the
  // band's own average boosts quiet bands (crude per-band equalizer)
  var v = (inst - av) / (av * 6 + 0.0001)
  v = max(0, v)
  v = v * v                         // punchy contrast

  // decay-trail persistence buffer
  persist[slot] = persist[slot] * 0.9 + v
  var out = min(1, persist[slot])
  briAccum += out                   // feed the auto-gain loop

  // spectrum spans ~half the hue wheel; slow overall color rotation;
  // hottest peaks desaturate toward white
  hsv(band / TOPBAND * 0.5 + slowPhase, 1 - out, out)
}

// 1D fallback: synthesize a serpentine matrix sized from the pixel count.
// The row count must come from ceil(pixelCount / width), not from the width:
// on any count that isn't a perfect square the last partial row sits at
// row >= fbWidth, and dividing it by fbWidth hands render2D a y >= 1, which
// walks the 16x16 persist[] index past 255 (60 px: 7 wide, 9 rows).
var fbWidth = max(1, floor(sqrt(pixelCount)))
var fbHeight = max(1, ceil(pixelCount / fbWidth))
export function render(index) {
  var row = floor(index / fbWidth)
  var col = index % fbWidth
  if (row % 2 == 1) col = fbWidth - 1 - col   // zigzag wiring
  render2D(index, col / fbWidth, row / fbHeight)
}
