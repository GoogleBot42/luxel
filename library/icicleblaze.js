// name: Icicleblaze
// Clean-room reimplementation from a prose functional description of the
// community pattern "Icicleblaze"; original source never consulted.

// One icicle at a time drips down the y axis (full width on a matrix; the
// 1D fallback treats strip position as height). Between drips the display
// goes fully dark for a random pause. Every icicle rolls fresh random
// parameters: duration, length, sparkle, one of four wave-tail styles,
// and one of six color schemes (cold white, fading white, warm white,
// deep blue, blue/white shimmer, candy cane).

// ---- configuration -------------------------------------------------------
var FALL_SECONDS = 3      // nominal fall duration (actual: 1x..2x this)
var LEN_FRACTION = 0.2    // nominal icicle length (actual: 1x..2x this)
var SPARKLE_PROB = 0.2    // chance an icicle sparkles
var WAVE_PROB = 0.7       // chance an icicle gets the intermittent wave tail
var MAX_GAP_MS = 4000     // dark pause between icicles: 0..this
var SCHEME_LIMIT = 6      // eligible color schemes 0..limit-1 (low = calmer)
var WAVE_CYCLES = 12      // spatial bands across the unit height

// ---- state ---------------------------------------------------------------
var progress = 0          // 0..1 fall progress of the current icicle
var prevProgress = 0
var waiting = 0           // 1 while dark between icicles
var waitAcc = 0
var waitMs = 0

// per-icicle rolls
var interval = 0          // time() interval for this icicle's duration
var phaseOffset = 0       // sawtooth phase captured at spawn
var len = 0.2
var sparkleOn = 0
var waveOn = 0
var tailPeriod = 0
var scheme = 0
var rand1 = 0
var rand2 = 0

function spawnIcicle() {
  var duration = FALL_SECONDS * (1 + random(1))   // seconds, nominal..2x
  len = LEN_FRACTION * (1 + random(1))
  sparkleOn = random(1) < SPARKLE_PROB
  waveOn = random(1) < WAVE_PROB
  var style = floor(random(4))
  // four tail-period looks: frozen stripes / bands riding along /
  // slow interference drift / fast shimmer
  if (style == 0) tailPeriod = 0
  else if (style == 1) tailPeriod = WAVE_CYCLES
  else if (style == 2) tailPeriod = WAVE_CYCLES * 0.85
  else tailPeriod = WAVE_CYCLES * 6
  scheme = floor(random(SCHEME_LIMIT))
  rand1 = random(1)
  rand2 = random(1)
  // reuse the global sawtooth: capture its phase now so this icicle's
  // progress starts at zero no matter when it spawns
  interval = duration / 65.536
  phaseOffset = time(interval)
  progress = 0
  prevProgress = 0
  waiting = 0
}

spawnIcicle()   // start with an icicle so the pattern doesn't begin dark

export function beforeRender(delta) {
  if (waiting) {
    waitAcc += delta
    if (waitAcc >= waitMs) spawnIcicle()
    return
  }
  progress = mod(time(interval) - phaseOffset, 1)
  if (progress < prevProgress) {
    // the sawtooth wrapped: this icicle is done; go dark for a random gap
    waiting = 1
    waitAcc = 0
    waitMs = random(MAX_GAP_MS)
  }
  prevProgress = progress
}

// 1D just treats position along the strip as height
export function render(index) {
  var p = index / pixelCount
  render2D(index, p, p)
}

export function render2D(index, x, y) {
  if (waiting) {
    rgb(0, 0, 0)
    return
  }
  // signed position within the icicle: head at pos == len, tail end at 0;
  // it enters from above the top and exits below the bottom
  var pos = y + len - progress * (1 + len)
  if (pos < 0 || pos > len) {   // cheap gate: skip all math outside the drip
    rgb(0, 0, 0)
    return
  }
  var toHead = pos / len        // 0 at the tail end, 1 at the head
  var v = 1

  if (waveOn) {
    // spatial sinusoid over height, phase-driven by the fall progress;
    // fade linearly toward the tail and subtract the wave scaled by the
    // squared tail fraction — solid head breaking into moving bands
    var w = wave(y * WAVE_CYCLES + progress * tailPeriod)
    var toTail = 1 - toHead
    v = max(toHead - w * toTail * toTail, 0)
  }

  if (sparkleOn) {
    // head-biased glitter: uniform draw scaled by nearness to the head,
    // lifted with a fractional power
    v = v * pow(random(1) * toHead, 0.667)
  }

  if (scheme == 0) {
    // plain cool white
    hsv(0, 0, v)
  } else if (scheme == 1) {
    // cool white dimming as it descends: two falloffs over height, only
    // reaching near-zero close to the bottom
    hsv(0, 0, v * (1 - y * 0.5) * (1 - y * 0.9))
  } else if (scheme == 2) {
    // warm whites: narrow orange-ish hue band, strong (not full) saturation
    hsv(0.05 + rand1 * 0.05, 0.75, v * (0.4 + rand2 * 0.6))
  } else if (scheme == 3) {
    // deep blues: fully saturated, quite dim
    hsv(0.58 + rand1 * 0.09, 1, v * (0.12 + rand2 * 0.1))
  } else if (scheme == 4) {
    // blue with white shimmer: hue drifts from cyan-blue along the icicle;
    // alternate pixels flip saturated-bright vs white-dim
    var h = 0.53 + toHead * 0.12
    if (mod(index, 2) < 1) hsv(h, 1, v)
    else hsv(h, 0, v * 0.3)
  } else {
    // christmas candy cane: hue from evenly spaced stops starting in the
    // blues; alternate white/colored pixels; brightness falls cubically
    // with height, white pixels slightly dimmer
    var hc = mod(0.6 + floor(rand1 * 5) / 5, 1)
    var d = 1 - y
    v = v * d * d * d
    if (mod(index, 2) < 1) hsv(hc, 1, v)
    else hsv(0, 0, v * 0.7)
  }
}
