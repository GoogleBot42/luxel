// name: b_lightning_flashes
// Clean-room reimplementation from a prose functional description of the
// community pattern "b_lightning_flashes"; original source never consulted.

// A mostly-dark strip. At random moments a short segment flashes on like a
// lightning strike: it swells to full brightness, crackles at its peak, dies
// away, then a random dark pause before the next strike somewhere else.
// Core reads saturated cold blue, fringes read white.

var timerMs = 0          // elapsed ms in the current strike cycle
var flashMs = 0          // duration of the bright phase (0 -> first frame rolls a strike)
var gapMs = 0            // dark pause after the flash
var center = 0           // strike center (pixel index)
var hue = 0.66           // cold blue, jittered per strike
var flicker = 0          // chaotic crackle toggle
var env = 0              // brightness envelope for this frame

var halfWidth = 8        // segment half-width in pixels
var speed = 0.5          // flash snappiness / gap stretch
var gapMaxMs = 1000      // max random dark gap

export function sliderLightningLength(v) {
  //# min=0 max=1 step=0.01 default=0.3
  // 1 pixel up to ~15% of the strip (spec suggests fraction-of-strip, not absolute)
  halfWidth = 1 + v * pixelCount * 0.15
}

export function sliderSpeed(v) {
  //# min=0 max=1 step=0.01 default=0.5
  speed = v
}

export function sliderOffDurationRandomness(v) {
  //# min=0 max=1 step=0.01 default=0.5
  gapMaxMs = 100 + v * 1900   // up to ~2 s of darkness
}

function newStrike() {
  // cold blue with a small jitter toward violet or cyan
  hue = 0.66 + random(0.1) - 0.05
  // keep the whole segment on the strip
  var hw = min(halfWidth, pixelCount / 2 - 1)
  center = hw + random(max(1, pixelCount - 2 * hw))
  // faster = snappier flash but longer dark gaps (same knob, coupled)
  var f = 0.3 + speed * 1.9
  flashMs = (220 + random(380)) / f
  gapMs = random(gapMaxMs) * f
  timerMs = 0
}

export function beforeRender(delta) {
  timerMs += delta

  // triangle envelope over the flash: 0 -> 1 at halfway -> 0
  env = triangle(clamp(timerMs / max(flashMs, 1), 0, 1))

  // chaotic crackle: small chance each frame to toggle the flicker flag;
  // near peak brightness the flag knocks the envelope down by half
  if (random(1) < 0.12) flicker = !flicker
  if (env > 0.6 && flicker) env = env * 0.5

  env = env * env   // gamma-style shaping

  if (timerMs > flashMs) {
    env = 0                                  // dark phase
    if (timerMs > flashMs + gapMs) newStrike()
  }
}

export function render(index) {
  var d = abs(index - center)
  if (d < halfWidth) {
    var v = env * (1 - d / halfWidth)
    // doubled so mid-envelope values already clip to full — strike saturates fast
    var bri = min(1, v * 2)
    // bright core = strongly saturated blue, dim fringes = pure white
    var s = v > 0.66 ? 0.85 : 0
    hsv(hue, s, bri)
  } else {
    rgb(0, 0, 0)
  }
}
