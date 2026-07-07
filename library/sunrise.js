// name: Sunrise
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sunrise"; original source never consulted.

// One-shot wake-up light: orange glow grows from the strip's center until the
// whole strip is orange (phase 1), then whiteness spreads from the center
// until the strip is pure white (phase 2) and holds. A GPIO switch can select
// a constant dim ambient-orange mode instead (disabled by default).

export var riseMinutes = 10     // nominal total show length, minutes
export var speedup = 100        // divisor: 100 => a few seconds (testing); 1 => real time
export var sunriseHue = 0.045   // warm orange, just off red toward amber
export var altModeEnable = 0    // 1 = honor the GPIO ambient-nightlight switch
export var altBrightness = 0.15
var altPin = 4

var started = 0
var startPhase = 0
var p1 = 0        // phase-one progress, latched (only ever increases)
var p2 = 0        // phase-two progress, latched
var altActive = 0

export function beforeRender(delta) {
  // free-running engine sawtooth over the full show duration
  var t = time(riseMinutes * 60 / speedup / 65.536)
  if (!started) {
    started = 1
    startPhase = t              // the clock isn't reset at load: capture zero
    pinMode(altPin, INPUT)
  }
  var progress = mod(t - startPhase, 1)
  // running maxima make the show one-shot: when the sawtooth wraps, hold
  p1 = max(p1, clamp(progress * 2, 0, 1))
  p2 = max(p2, clamp(progress * 2 - 1, 0, 1))
  altActive = altModeEnable && digitalRead(altPin) == LOW
}

export function render(index) {
  if (altActive) {
    hsv(sunriseHue, 1, altBrightness)   // ambient nightlight mode
    return
  }
  // spatial dome peaking at the strip midpoint
  var dome = triangle(index / pixelCount)
  if (p2 <= 0) {
    // rising sun: dome plus rising offset drives brightness
    hsv(sunriseHue, 1, clamp(dome - 1 + 2 * p1, 0, 1))
  } else {
    // fade to white: same construction drives desaturation at full brightness
    hsv(sunriseHue, 1 - clamp(dome - 1 + 2 * p2, 0, 1), 1)
  }
}
