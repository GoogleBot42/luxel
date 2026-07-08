// name: sound - spectroblots - pow fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - spectroblots - pow fade"; original source never
// consulted. A folded fractal-noise field partitions the mapped space into
// contiguous amoeba blobs, one per spectrum band; each band's blob flares when
// that band spikes over its running average and decays with a frame-rate-
// independent "pow fade". Mirror-symmetric, slowly hue-rotating and breathing.
// Native renderer is 3D; 2D and 1D entry points delegate with axes zeroed.
// When no sensor board is present (all-zero frequencyData) it drives itself
// from an internal 120 BPM four-on-the-floor simulator so it never sits dead.

var BANDS = 32
export var frequencyData = array(BANDS)   // 32-band spectrum from the board
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0
export var accelerometer = array(3)
export var light = 0                       // board-presence sentinel (unused visually)

var avg = array(BANDS)          // per-band running average
var excite = array(BANDS)       // per-band excitement
var spectrum = array(BANDS)     // this frame's readings (real or simulated)
var sim = array(BANDS)          // simulator working buffer
var persist = array(pixelCount) // per-pixel brightness persistence

var sensitivity = 40            // fixed constant gain (AGC output overridden)
var hueClock = 0                // slow global hue rotation
var driftZ = 0                  // very slow noise-field drift on the 3rd axis
var zoom = 2                    // breathing scale
var retain = 0.5                // this frame's persistence factor
var simAccum = 0
var simClock = 0

// read-only monitor gauges
export function gaugeFeedback() { return 0 }
export function gaugeGain() { return sensitivity / 100 }
export function gaugeComplexity() { return zoom / 3 }

// four-on-the-floor dance-loop simulator (~40 Hz updates)
function simulate(t) {
  var i
  for (i = 0; i < BANDS; i++) sim[i] *= 0.55
  var beat = frac(t * 2)          // 120 BPM -> 2 beats/s
  var idx = floor(t * 2)
  var inMeasure = idx % 4
  // kick: every beat, splayed across the lowest bands
  if (beat < 0.12) {
    var b
    for (b = 0; b < 5; b++) sim[b] += 0.9 * (1 - b * 0.12)
  }
  // clap: offbeats on beats 2 & 4, low-mid bands
  if ((inMeasure == 1 || inMeasure == 3) && frac(t * 2 + 0.5) < 0.1) {
    var c
    for (c = 8; c < 12; c++) sim[c] += 0.5
  }
  // hi-hat: beats 2 & 4, upper-mid bands
  if ((inMeasure == 1 || inMeasure == 3) && beat < 0.08) {
    var h
    for (h = 18; h < 24; h++) sim[h] += 0.35
  }
  // wandering lead synth (meanders over a long phrase)
  var lead = floor(12 + 8 * wave(t * 0.03))
  if (lead >= 0 && lead < BANDS) sim[lead] += 0.6
  if (random(1) < 0.3) {
    var l2 = lead + 3
    if (l2 < BANDS) sim[l2] += 0.4
  }
}

export function beforeRender(delta) {
  var dt = delta / 1000

  zoom = mix(1.2, 3, wave(time(1.5)))   // breathing, ~a couple minutes/cycle
  hueClock += dt * 0.03                 // tens of seconds per hue lap
  driftZ += dt * 0.01                   // minutes-scale blob morph

  // board present iff any band is nonzero; otherwise self-drive the simulator
  var hasBoard = 0
  var bi
  for (bi = 0; bi < BANDS; bi++) { if (frequencyData[bi] != 0) hasBoard = 1 }

  if (hasBoard) {
    for (bi = 0; bi < BANDS; bi++) spectrum[bi] = frequencyData[bi]
  } else {
    simAccum += dt
    while (simAccum >= 0.025) {
      simAccum -= 0.025
      simClock += 0.025
      simulate(simClock)
    }
    for (bi = 0; bi < BANDS; bi++) spectrum[bi] = sim[bi]
  }

  // per-band running average + excess-over-average excitement
  var smooth = 1 - exp(-dt / 3)   // ~3 s time constant, frame-compensated
  for (bi = 0; bi < BANDS; bi++) {
    var reading = spectrum[bi] * sensitivity
    avg[bi] += (reading - avg[bi]) * smooth
    var e = (reading - avg[bi] * 3) * (1 + avg[bi] * 2)   // excess, loud-boosted
    if (e < 0) e = 0
    e = (e + excite[bi]) * 0.5                            // one-frame smoothing
    excite[bi] = clamp(e, 0, 4)                           // generous overdrive
  }

  // pow fade: ~0.6 kept per 0.1 s, exponent from frame time
  retain = pow(0.6, dt / 0.1)
}

export function render3D(index, x, y, z) {
  var xx = abs(x - 0.5) * 2                    // cheap mirror symmetry
  var nx = (xx - 0.5) * zoom
  var ny = (y - 0.5) * zoom
  var nz = (z - 0.5) * zoom + driftZ

  var n = perlinFbm(nx, ny, nz, 2, 0.5, 3)     // multi-octave noise field
  var v = triangle(n)                          // fold to a well-spread 0..1
  var band = floor(v * BANDS)
  if (band < 0) band = 0
  if (band >= BANDS) band = BANDS - 1

  var nv = persist[index] * retain + excite[band] * (1 - retain)
  persist[index] = nv

  var bright = clamp(nv, 0, 1)
  var sat = clamp(1.3 - nv, 0, 1)              // overdrive desaturates to white
  hsv(v + hueClock, sat, bright * bright)      // square for gamma
}

export function render2D(index, x, y) { render3D(index, x, y, 0) }
export function render(index) { render3D(index, index / pixelCount, 0, 0) }
