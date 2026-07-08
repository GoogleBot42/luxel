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
