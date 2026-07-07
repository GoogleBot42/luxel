// name: SOUND - lavablob
// Clean-room reimplementation from a prose functional description of the
// community pattern "SOUND - lavablob"; original source never consulted.

// Sound-reactive lava lamp: soft blobs formed by the product of three
// interfering waves in x and y drift across the surface. A mid-spectrum
// band gates overall brightness (silence = dark, music = pulsing flares);
// overall sound energy scales the blob texture's spatial frequency. Hue
// sweeps slowly with a shallow diagonal gradient; dim fringes wash toward
// white while blob cores stay saturated. The original computed its phase
// speed once at startup by coercing the spectrum array to a number (a
// known bug); here that "speed" is taken as the sane constant 1, giving
// phase periods of several seconds — expect smoother drift than the
// original's degenerate timing.

export var frequencyData = array(32)   // sensor-board spectrum bands
export var energyAverage = 0           // overall sound energy
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0
export var accelerometer = array(3)
export var light = 0

const speed = 1        // sane stand-in for the original's startup-bug value

var t1 = 0
var t2 = 0
var t3 = 0
var zoom = 0

export function beforeRender(delta) {
  // Three mutually incommensurate sawtooth phases, so the combination
  // never visibly repeats. Periods ~7 s / ~4.6 s / ~3.1 s at speed = 1.
  t1 = time(0.11 / speed)
  t2 = time(0.07 / speed)
  t3 = time(0.047 / speed)
  // Spatial scale: tiny floor + slow breathing (tens of seconds), all
  // multiplied by loudness with a large gain. Silence collapses the field
  // to one flat blob; loud sound makes finer, busier texture.
  zoom = (0.02 + triangle(time(0.35))) * energyAverage * 40
}

export function render3D(index, x, y, z) {
  var h = t1 + (x + y + z) * 0.2
  // Product of three interfering waves, amplified ~10x: blob cores blow
  // out well past full scale on purpose.
  var i = triangle(y * zoom + wave(t1))
        * wave(y * zoom + wave(t2))
        * wave(x * zoom + wave(t3)) * 10
  // Fringe wash: low intensity -> desaturated (whitish), cores saturate.
  var s = clamp(i - 1, 0, 1)
  // A mid-spectrum band gates and pulses the whole field; squaring the
  // intensity sharpens blob cores. All-zero audio -> stays dark.
  var v = clamp(frequencyData[13] * 5 * i * i, 0, 1)
  hsv(h, s, v)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}

export function render(index) {
  render3D(index, index / pixelCount, 0, 0)
}
