// name: chill confetti
// Clean-room reimplementation from a prose functional description of the
// community pattern "chill confetti"; original source never consulted.
//
// Gentle confetti: pixels pop to full brightness in a tight family of hues
// around a slowly-drifting base hue and fade exponentially over a second or
// two. Two delta-accumulated timers drive fading (~10/s) and spawning
// (~12/s); the base hue circles the wheel in a few tens of seconds.

var FADE_TICK = 0.1        // seconds between fade steps (~10/s)
var FADE_DECAY = 0.833     // ~5/6 brightness per tick -> ~1-2 s visible fade
var DRAW_TICK = 0.08       // seconds between spawns (~12.5/s)
var HUE_STEP = 0.0025      // base-hue advance per spawn -> full wheel ~32 s
var JITTER = 0.04          // +/- fraction of the wheel around the base hue

var spawnRate = 12.5       // sparks per second (DRAW_TICK is its reciprocal)
var driftSec = 32          // seconds for the base hue to walk the whole wheel

// Spawn rate and drift time together set the per-spark hue advance.
function recompute() {
  DRAW_TICK = 1 / spawnRate
  HUE_STEP = 1 / (driftSec * spawnRate)
}

// Time for a spark to fade to ~5% of full brightness.
//# min=0.2 max=6 step=0.1 default=1.6
export function sliderFadeSeconds(v) {
  FADE_DECAY = pow(0.833, 1.6 / max(0.1, v))
}

//# min=1 max=60 step=0.5 default=12.5
export function sliderSparksPerSecond(v) {
  spawnRate = max(0.5, v)
  recompute()
}

//# min=2 max=180 step=1 default=32
export function sliderHueDriftSeconds(v) {
  driftSec = max(0.5, v)
  recompute()
}

// Spread of spark hues around the drifting base hue, in degrees either side.
//# min=0 max=180 step=1 default=14
export function sliderHueJitter(v) { JITTER = max(0, v) / 360 }

var hues = array(pixelCount)
var brights = array(pixelCount)
var fadeAcc = 0
var drawAcc = 0
var curHue = random(1)     // random starting point on the wheel

export function beforeRender(delta) {
  var dt = delta / 1000
  fadeAcc = fadeAcc + dt
  drawAcc = drawAcc + dt

  var guard = 0
  while (fadeAcc >= FADE_TICK && guard < 8) {
    fadeAcc = fadeAcc - FADE_TICK
    feedback(brights, FADE_DECAY)
    guard = guard + 1
  }

  guard = 0
  while (drawAcc >= DRAW_TICK && guard < 8) {
    drawAcc = drawAcc - DRAW_TICK
    var p = floor(random(pixelCount))
    if (p > pixelCount - 1) p = pixelCount - 1
    brights[p] = 1
    hues[p] = mod(curHue + (random(1) - 0.5) * 2 * JITTER, 1)
    curHue = mod(curHue + HUE_STEP, 1)
    guard = guard + 1
  }
}

export function render(index) {
  hsv(hues[index], 1, brights[index])
}
