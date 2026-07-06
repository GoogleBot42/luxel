// name: SOUND - lavablob
// Clean-room reimplementation from a prose functional description of the
// community pattern "SOUND - lavablob"; original source never consulted.

// Sound-reactive lava lamp: soft organic blobs drift and undulate, made by
// multiplying three interfering waves in x and y. Silence goes dark; music
// makes the blobs flare and pulse with a mid-spectrum band. Overall
// loudness stretches the blob texture (louder = finer, busier). Hues sweep
// slowly with a shallow diagonal gradient; dim fringes wash toward white
// while bright cores stay saturated.
//
// The original computed a "speed" divisor once at startup by coercing the
// spectrum array itself to a number — a bug that made the phases run
// degenerately fast. This reimplementation takes the documented sane fix:
// speed is a constant of about unity, giving phase periods of several
// seconds (visibly slower and smoother than the buggy original).

// Sensor-board inputs; the engine stubs these with zeros when no sensor
// board is present, so the pattern idles dark.
export var frequencyData = array(32)
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0
export var accelerometer = array(3)
export var light = 0

const SPEED = 1     // sane-fix stand-in for the original's startup bug

var t1 = 0
var t2 = 0
var t3 = 0
var zoom = 0

export function beforeRender(delta) {
  // Three mutually incommensurate sawtooth phases — their combination
  // never visibly repeats.
  t1 = time(0.11 / SPEED)
  t2 = time(0.067 / SPEED)
  t3 = time(0.089 / SPEED)

  // Spatial scale: a tiny floor plus a ~20 s triangle breath, multiplied
  // by overall loudness and a large gain. Silence collapses it toward
  // zero (one huge flat field); loud sound raises it (busier texture).
  zoom = (0.02 + triangle(time(0.3))) * energyAverage * 60
}

export function render3D(index, x, y, z) {
  // Shallow diagonal rainbow gradient drifting over time.
  var h = t1 + (x + y + z) / 5

  // Product of three interfering waves, amplified about tenfold — bright
  // blob-shaped regions where the waves align. Deliberately allowed to
  // blow far past 1 so cores clamp hard-edged and white-hot.
  var i = triangle(y * zoom + wave(t1)) * wave(y * zoom + wave(t2)) *
          wave(x * zoom + wave(t3)) * 10

  // Low intensity -> desaturated (pale fringe); above the threshold the
  // cores are fully saturated. This is the molten-rim look.
  var s = clamp(i - 1, 0, 1)

  // One mid-spectrum band gates the whole field: silent band = display
  // off; the pattern pulses with it. Squared intensity sharpens cores.
  var v = clamp(frequencyData[15] * 5 * i * i, 0, 1)

  hsv(h, s, v)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}

export function render(index) {
  render3D(index, index / pixelCount, 0, 0)
}
