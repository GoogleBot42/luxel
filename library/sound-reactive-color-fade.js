// name: Sound Reactive Color Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sound Reactive Color Fade"; original source never
// consulted.

// The whole display is one solid, fully saturated color. The hue creeps
// slowly around the wheel; every detected bass beat snaps it forward to
// the next of six anchor hues. Beat detection is edge-triggered on the
// rate of rise of a fast-smoothed bass signal, normalized by the recent
// peak level, so it is volume-independent.

// Sensor bindings (engine stubs these with zeros when no sensor board).
export var frequencyData = array(32)
export var energyAverage
export var maxFrequency
export var maxFrequencyMagnitude
export var accelerometer = array(3)
// Presence probe: initialized to an impossible negative value; only a real
// sensor board ever overwrites it.
export var light = -1

// Six anchor hues: red, yellow, green, cyan, blue, magenta (the magenta
// anchor sits a bit past pure blue rather than exactly evenly spaced).
var ANCHORS = 6
var anchor = array(ANCHORS)
anchor[0] = 0
anchor[1] = 0.167
anchor[2] = 0.333
anchor[3] = 0.5
anchor[4] = 0.667
anchor[5] = 0.78

var hue = 0

// Bass detector state.
var slowEma = 0            // very long time constant baseline
var fastEma = 0            // ~ten-frame smoothing
var recentMax = 0.01       // decaying recent peak (auto gain control)
var NOISE_FLOOR = 0.02

// Circular buffer of recent "fast EMA rose this frame" indicators; a beat
// candidate fires when its average exceeds a threshold slightly over half.
var MAXBUF = 16
var riseBuf = array(MAXBUF)
var bufLen = 4
var bufPos = 0
var RISE_THRESHOLD = 0.55

// Debounce: about a fifth of a quarter-note at an ordinary dance tempo
// (~128 bpm quarter = ~469 ms), so fast doubled kicks can re-trigger but
// chatter is rejected.
var DEBOUNCE_MS = 94
var debounce = 0

var beatSens = 0.3
var fadeSpeed = 0.5

// Beat sensitivity: quadratic map from a couple of samples (twitchy) up to
// about fifteen (slow to react, better for sparse bass). Recomputed live
// whenever the slider moves — resizing clears the buffer.
//# min=0 max=1 step=0.01 default=0.3
export function sliderBeatSensitivity(v) {
  beatSens = v
  var n = floor(2 + v * v * 13)
  if (n != bufLen) {
    bufLen = n
    bufPos = 0
    for (var i = 0; i < MAXBUF; i++) riseBuf[i] = 0
  }
}

export function showNumberBeatBuffer() {
  return bufLen
}

// Fade speed: multiplies the idle hue-drift rate severalfold.
//# min=0 max=1 step=0.01 default=0.5
export function sliderFadeSpeed(v) {
  fadeSpeed = v
}

export function showNumberFadeSpeed() {
  return fadeSpeed
}

export function beforeRender(delta) {
  // --- Sound processing (skipped when no sensor board is present) ---
  if (light >= 0) {
    // Bass energy: sum the lowest few bands, skipping the DC band —
    // kick-drum fundamentals.
    var bass = frequencyData[1] + frequencyData[2] + frequencyData[3]

    // Auto gain control: track the recent peak, decay it slowly while it
    // sits far above the long-term average.
    if (bass > recentMax) recentMax = bass
    if (recentMax > slowEma * 2 && recentMax > NOISE_FLOOR) {
      recentMax = recentMax * 0.999
    }

    // Slow (~thousand-frame) and fast (~ten-frame) moving averages.
    slowEma += (bass - slowEma) / 1000
    var prevFast = fastEma
    fastEma += (bass - prevFast) / 10

    // Rate-of-rise, normalized by the recent peak (volume-independent):
    // record whether the fast average rose meaningfully this frame.
    var rise = 0
    if (recentMax > NOISE_FLOOR && (fastEma - prevFast) / recentMax > 0.01) {
      rise = 1
    }
    riseBuf[bufPos] = rise
    bufPos = (bufPos + 1) % bufLen

    var sum = 0
    for (var i = 0; i < bufLen; i++) sum += riseBuf[i]

    if (debounce > 0) debounce -= delta

    // Beat: bass has been rising for most of the window and the debounce
    // countdown has expired. Snap the hue forward to the next anchor
    // strictly greater than the current hue, wrapping past the last.
    if (sum / bufLen > RISE_THRESHOLD && debounce <= 0) {
      debounce = DEBOUNCE_MS
      var next = anchor[0]
      for (var i = 0; i < ANCHORS; i++) {
        if (anchor[i] > hue + 0.001) {
          next = anchor[i]
          break
        }
      }
      hue = next
    }
  }

  // --- Idle drift, always on: a small base rate plus a several-times ---
  // larger slider-scaled rate. Minutes per lap at mid slider.
  hue = frac(hue + (0.002 + 0.01 * fadeSpeed) * delta / 1000)
}

export function render(index) {
  hsv(hue, 1, 1)
}

// The output is a single solid color, so the 2D renderer is trivially
// identical — offered for mapped fixtures.
export function render2D(index, x, y) {
  hsv(hue, 1, 1)
}
