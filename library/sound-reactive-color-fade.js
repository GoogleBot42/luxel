// name: Sound Reactive Color Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sound Reactive Color Fade"; original source never
// consulted.

// The whole display is one solid, fully saturated color. Idle, the hue
// creeps slowly around the wheel (minutes per lap at mid slider). Each
// detected bass beat snaps the hue forward to the next of six anchor
// hues, then the slow drift resumes. The beat detector is edge-triggered
// on the rate of rise of a fast-smoothed bass signal, normalized by a
// decaying recent maximum (auto gain control), so it is volume
// independent. With no sensor board (all-zero audio) it just fades.

// Sensor-board bindings (engine stubs these when no board is present).
export var frequencyData = array(32)
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0
export var accelerometer = array(3)
// Presence probe: impossible negative unless a sensor board overwrites it.
export var light = -1

// Six anchor hues: red, yellow, green, cyan, blue, and a magenta that
// sits a bit past pure blue rather than exactly evenly spaced.
var anchors = array(6)
anchors[0] = 0
anchors[1] = 1 / 6
anchors[2] = 2 / 6
anchors[3] = 3 / 6
anchors[4] = 4 / 6
anchors[5] = 0.75

// --- state kept between frames ---
export var hue = 0
var slowEma = 0        // very long time constant bass average
var fastEma = 0        // ~ten-frame bass average
var lastFast = 0
var maxBass = 0        // decaying recent maximum (AGC)
var debounceMs = 0

// Circular buffer of normalized frame-to-frame rises of the fast EMA.
var DBUF_MAX = 15
var dbuf = array(15)
var dlen = 4
var dpos = 0
var dsum = 0

var beatSensitivity = 0.4
var fadeSpeed = 0.5

//# min=0 max=1 step=0.01 default=0.4
export function sliderBeatSensitivity(v) {
  beatSensitivity = v
  // Quadratic map: 2 samples (twitchy) up to 15 (slow, forgiving).
  // Recompute and clear whenever the control moves (fixing the
  // original's compute-once-before-default bug).
  var n = floor(2 + v * v * (DBUF_MAX - 2))
  if (n != dlen) {
    dlen = n
    dsum = 0
    dpos = 0
    var i
    for (i = 0; i < DBUF_MAX; i++) dbuf[i] = 0
  }
}
sliderBeatSensitivity(beatSensitivity)

//# min=0 max=1 step=0.01 default=0.5
export function sliderFadeSpeed(v) {
  fadeSpeed = v
}

function nextAnchor(h) {
  var i
  for (i = 0; i < 6; i++) {
    if (anchors[i] > h) return anchors[i]
  }
  return anchors[0]
}

export function beforeRender(delta) {
  // Sound processing only when a sensor board has claimed the inputs.
  if (light >= 0) {
    // Bass energy: sum the lowest few bands, skipping the DC band.
    var bass = frequencyData[1] + frequencyData[2] + frequencyData[3]

    // Decaying recent maximum = automatic gain control.
    if (bass > maxBass) maxBass = bass
    if (maxBass > slowEma * 3 && maxBass > 0.01) maxBass = maxBass * 0.999

    slowEma = slowEma + (bass - slowEma) / 1000
    fastEma = fastEma + (bass - fastEma) / 10

    // Normalized rise of the fast average, recentered so "no change"
    // sits at one half; the buffer's running mean crossing a threshold
    // slightly over half means "bass is rising" — a beat candidate.
    var d = 0
    if (maxBass > 0) d = (fastEma - lastFast) / maxBass
    lastFast = fastEma
    var norm = clamp(0.5 + d * 4, 0, 1)
    dsum = dsum - dbuf[dpos] + norm
    dbuf[dpos] = norm
    dpos = (dpos + 1) % dlen

    debounceMs = debounceMs - delta
    if (dsum / dlen > 0.55 && debounceMs <= 0) {
      // Beat: snap to the next anchor hue. Debounce reloads with about
      // a fifth of a quarter note at ordinary dance tempo (~120 BPM),
      // so doubled kicks still retrigger but chatter is rejected.
      hue = nextAnchor(hue)
      debounceMs = 100
    }
  }

  // Independent slow drift: barely-perceptible base rate plus a
  // several-times-larger slider-scaled rate. Minutes per lap at mid.
  hue = frac(hue + delta * (0.0000015 + fadeSpeed * 0.00001))
}

export function render(index) {
  hsv(hue, 1, 1)
}

export function render2D(index, x, y) {
  hsv(hue, 1, 1)
}
