// name: Sunrise
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sunrise"; original source never consulted.

// One-shot wake-up light: warm orange grows outward from the strip center
// until everything is orange, then whiteness spreads the same way until the
// whole strip is full white, and holds. A GPIO pin can select a constant dim
// ambient-orange mode instead (disabled by default).

// configuration (the original exposes these as constants, not sliders)
export var speedUp = 100        // divisor: 1 = real time, 100 = seconds-long test run
export var riseSeconds = 600    // nominal total show length (10 minutes)
export var sunHue = 0.04        // classic sunrise orange, just off red
export var altModeEnable = 0    // 1 = allow the GPIO ambient mode
export var altBrightness = 0.25
var altPin = 26

pinMode(altPin, INPUT)

var started = 0
var t0 = 0     // engine clock sample captured at startup
var p1 = 0     // phase-one progress, latched with a running max (one-shot)
var p2 = 0     // phase-two progress, same latch
var altActive = 0

export function beforeRender(delta) {
  // the free-running sawtooth is NOT reset on pattern load: capture its
  // value once and use the wrapped difference as zero-based progress
  var t = time(riseSeconds / speedUp / 65.536)
  if (!started) { t0 = t; started = 1 }
  var p = t - t0
  if (p < 0) p += 1

  // running maxima: when the sawtooth wraps, these hold — never replays
  p1 = max(p1, clamp(p * 2, 0, 1))        // completes in the first half
  p2 = max(p2, clamp(p * 2 - 1, 0, 1))    // runs during the second half

  altActive = altModeEnable && !digitalRead(altPin)
}

export function render(index) {
  if (altActive) {
    // ambient nightlight mode: constant dim orange
    hsv(sunHue, 1, altBrightness)
    return
  }

  // spatial dome: peaks at the strip midpoint, zero at both ends
  var dome = triangle(index / pixelCount)

  if (p2 <= 0) {
    // phase one: dome + rising offset drives brightness (sun growing out
    // from the center until the offset swamps the dome)
    hsv(sunHue, 1, clamp(dome - 1 + 2 * p1, 0, 1))
  } else {
    // phase two: same construction drives DEsaturation at full brightness
    // (white spreads from the center, then holds)
    hsv(sunHue, 1 - clamp(dome - 1 + 2 * p2, 0, 1), 1)
  }
}
