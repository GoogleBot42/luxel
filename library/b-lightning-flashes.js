// name: b_lightning_flashes
// Clean-room reimplementation from a prose functional description of the
// community pattern "b_lightning_flashes"; original source never consulted.

// Mostly-dark strip. At random moments a bluish-white segment flashes on:
// swells to peak, crackles erratically near its top, then dies out and is
// followed by a random dark gap before the next strike lands elsewhere.
// Bright core reads saturated cold blue; the dim fringes read white.

var halfWidth = 1 + 0.5 * pixelCount * 0.15   // segment half-width in pixels
var speed = 1                                  // >1 = snappier flashes, longer gaps
var maxGapMs = 1000                            // max dark pause between strikes

var timer = 0          // ms into the current strike cycle
var center = 0         // strike center (pixel index)
var hue = 0.66         // per-strike cold-blue hue
var flashMs = 300      // duration of the visible flash
var gapMs = 400        // dark pause after the flash
var flicker = 0        // chaotic crackle toggle
var env = 0            // brightness envelope for this frame

//# min=0 max=1 step=0.01 default=0.3
export function sliderLightningLength(v) {
  // from a single pixel up to ~15% of the strip either side of center
  halfWidth = 1 + v * pixelCount * 0.15
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  // one knob, two effects: shorter flashes AND proportionally longer gaps
  speed = 0.25 + v * 2
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderOffDurationRandomness(v) {
  maxGapMs = 100 + v * 1900   // up to ~2 s of darkness
}

function newStrike() {
  hue = 0.66 + random(0.12) - 0.06                    // blue-violet .. blue-cyan
  center = halfWidth + random(pixelCount - 2 * halfWidth)
  flashMs = (200 + random(300)) / speed
  gapMs = random(maxGapMs) * speed
  timer = 0
}

export function beforeRender(delta) {
  timer += delta

  // triangle envelope: 0 -> 1 at the halfway point -> 0
  env = timer < flashMs ? triangle(0.5 * timer / flashMs) : 0

  // chaotic crackle: small per-frame chance to toggle, bites only near peak
  if (random(1) < 0.15) flicker = !flicker
  if (env > 0.6 && flicker) env *= 0.5

  env *= env   // gamma-style shaping

  if (timer > flashMs + gapMs) newStrike()
}

export function render(index) {
  var d = abs(index - center)
  if (d < halfWidth) {
    var k = env * (1 - d / halfWidth)      // linear falloff to the segment edge
    var s = k > 0.66 ? 0.85 : 0            // bright core blue, dim fringe white
    hsv(hue, s, clamp(k * 2, 0, 1))        // doubled so mid-envelope saturates
  } else {
    rgb(0, 0, 0)
  }
}
