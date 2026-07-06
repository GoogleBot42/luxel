// name: sound - spectromatrix render2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - spectromatrix render2D"; original source
// never consulted.

// Colored blobs flare along wavy, drifting plasma iso-bands wherever the
// spectrum has above-average energy; flashes whiten at the peak and decay
// as short trails. A PI auto-gain loop on *emitted* brightness keeps the
// visual density constant regardless of room volume. Quiet input = black.
//
// The scene is simulated on a 16x16 virtual canvas each frame; render2D
// samples it, and the 1D fallback synthesizes a serpentine square matrix
// sized from the actual pixel count (the spec's "obvious fix" for the
// original's hardcoded 8x8).

// Sensor-board inputs; the engine stubs these with zeros when no sensor
// board is present, so the pattern idles dark.
export var frequencyData = array(32)
export var energyAverage
export var maxFrequency
export var maxFrequencyMagnitude

const BANDS = 32
const CW = 16                  // virtual canvas is CW x CW
const CELLS = 256
const TARGET_FILL = 0.05       // auto-gain setpoint: avg lit fraction
const AVG_WINDOW = 1.5         // seconds; per-band EMA window
const TRAIL_RATE = 4           // 1/s; flash decay, a few tenths of a second

var bri = array(CELLS)         // per-cell brightness persistence buffer
var band = array(CELLS)        // per-cell fractional band index (for hue)
var avg = array(BANDS)         // per-band rolling average of scaled energy
var sens = 1                   // auto-gain sensitivity multiplier
var integ = 0                  // PI integral term
var emitted = 0                // brightness accumulated last frame

export function beforeRender(delta) {
  var dt = delta / 1000

  driftPhase = time(0.18)      // spatial band drift, ~12 s per cycle
  scanPhase = time(0.06)       // faster band-scan drift, ~4 s
  huePhase = time(0.15)        // slow color rotation, ~10 s

  // PI auto-gain on the brightness actually emitted last frame
  var err = TARGET_FILL - emitted / CELLS
  integ = clamp(integ + err * dt * 4, -10, 250)
  sens = clamp(1 + err * 4 + integ, 0.02, 300)
  emitted = 0

  // Fold the new spectrum frame into the per-band rolling averages
  var k = clamp(dt / AVG_WINDOW, 0, 1)
  for (var i = 0; i < BANDS; i++) {
    avg[i] = max(avg[i] * (1 - k) + frequencyData[i] * sens * k, 0.0001)
  }

  var decay = max(0, 1 - dt * TRAIL_RATE)

  // Simulate the canvas
  var n = 0
  for (var yy = 0; yy < CW; yy++) {
    var y = yy / (CW - 1)
    for (var xx = 0; xx < CW; xx++) {
      var x = xx / (CW - 1)
      // Drifting, folding iso-bands mapping the plane onto the spectrum
      var bf = triangle((wave(x + driftPhase) + wave(y - driftPhase)) / 2
                        + scanPhase) * (BANDS - 1)
      band[n] = bf

      // Linearly interpolate energy and rolling average between bands
      var i0 = floor(bf)
      var f = bf - i0
      var i1 = min(i0 + 1, BANDS - 1)
      var e = (frequencyData[i0] * (1 - f) + frequencyData[i1] * f) * sens
      var a = avg[i0] * (1 - f) + avg[i1] * f

      // Transient detector: only above-average energy shows. Quiet bands
      // get boosted (crude per-band equalizer), then square for punch.
      var d = clamp((e - a) * (0.5 + 0.05 / a), 0, 6)
      bri[n] = min(1, bri[n] * decay + d * d)
      emitted += bri[n]
      n += 1
    }
  }
}

export function render2D(index, x, y) {
  var n = min(15, floor(y * 15.99)) * 16 + min(15, floor(x * 15.99))
  var v = bri[n]
  // Spectrum spans about half the hue wheel; peaks blow out toward white
  hsv(band[n] / (BANDS * 2) + huePhase, 1 - v, v)
}

// 1D fallback: fake a serpentine square matrix sized from pixelCount
export function render(index) {
  var w = max(2, floor(sqrt(pixelCount)))
  var yy = floor(index / w)
  var xx = index % w
  if (yy % 2 == 1) xx = w - 1 - xx      // zigzag wiring
  render2D(index, xx / (w - 1), min(1, yy / (w - 1)))
}
