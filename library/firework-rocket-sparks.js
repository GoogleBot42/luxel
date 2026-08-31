// name: firework rocket sparks
// Clean-room reimplementation from a prose functional description of the
// community pattern "firework rocket sparks"; original source never
// consulted.

// A razor-thin, intensely bright warm-colored "rocket" glides along the
// strip, wrapping every few seconds. A fixed distance behind it, sparse
// pure-white sparks strobe for single frames like sputtering embers; the
// rest of the strip is black. The rocket's hue drifts through
// reds/oranges/yellows because it is tied to absolute strip position.
// Per the spec's cleanup notes, the rocket/spark separation and the
// hue-cycle length are fractions of the strip rather than hardcoded pixel
// counts (they match the original's 10 and 20 pixels on a 60-pixel strip).

var SEPARATION = 1 / 6     // rocket leads the spark zone by this fraction
var HUE_REPEATS = 3        // warm hue ramp repeats along the strip
var WARM_BAND = 0.2        // red -> yellow fifth of the hue wheel
var ROCKET_EDGE = 0.996    // crest threshold carving the thin rocket block
var SPARK_EDGE = 0.97      // crest threshold bounding the spark zone
var SPARK_ODDS = 0.05      // per-pixel per-frame firing chance in the zone

var lapInterval = 0.05     // time() interval for one traversal (~3.3 s)

// Seconds for the rocket to travel the whole strip once.
//# min=0.5 max=10 step=0.1 default=3.3
export function sliderLapTime(v) {
  lapInterval = max(v, 0.5) / 65.536
}

// Rocket length in pixels. The rocket is the part of a broad traveling
// crest above ROCKET_EDGE, so a width w (as a fraction f = w/pixelCount of
// the strip) corresponds to the crest threshold (1 + cos(PI*f)) / 2.
//# min=0.5 max=8 step=0.1 default=2.4
export function sliderRocketWidth(v) {
  var f = clamp(v, 0.5, 8) / pixelCount
  ROCKET_EDGE = 0.5 + 0.5 * cos(PI * f)
}

// Distance in pixels from the rocket back to the spark zone.
//# min=1 max=30 step=1 default=10
export function sliderTrailGap(v) {
  SEPARATION = clamp(floor(v), 1, 30) / pixelCount
}

// Per-pixel, per-frame chance (percent) that a pixel inside the spark zone
// flashes white.
//# min=0 max=25 step=1 default=5
export function sliderSparkChance(v) {
  SPARK_ODDS = clamp(v, 0, 25) / 100
}

// How many times the red -> yellow ramp repeats along the strip.
//# min=1 max=8 step=1 default=3
export function sliderHueCycles(v) {
  HUE_REPEATS = clamp(floor(v), 1, 8)
}

var t

export function beforeRender(delta) {
  t = time(lapInterval)    // one full traversal every ~3.3 s by default
}

export function render(index) {
  var pos = index / pixelCount
  var w = wave(t + pos)                 // traveling crest: spark zone
  var wr = wave(t + pos + SEPARATION)   // shifted crest: rocket location

  if (wr > ROCKET_EDGE) {
    // only a couple of pixels sit this close to the peak: a thin solid
    // block carved out of a broad smooth wave
    hsv(frac(pos * HUE_REPEATS) * WARM_BAND, 1, 1)
  } else {
    // sparks: near the unshifted crest AND a fresh random draw clears a
    // high bar. Zero saturation makes them pure white; zero value
    // elsewhere keeps the background black.
    var spark = 0
    if (w > SPARK_EDGE && random(1) < SPARK_ODDS) spark = w
    hsv(0, 0, spark)
  }
}
