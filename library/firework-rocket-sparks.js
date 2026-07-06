// name: firework rocket sparks
// Clean-room reimplementation from a prose functional description of the
// community pattern "firework rocket sparks"; original source never
// consulted.

// A small, intensely bright warm-colored "rocket" glides along the strip
// (one full wrapping pass every few seconds). A fixed distance behind it,
// sparse pure-white sparks strobe for single frames like sputtering
// embers. The rocket's hue drifts through reds/oranges/yellows because it
// is tied to its absolute position on the strip. Per the spec's cleanup
// notes, the rocket/spark separation and the hue-cycle length are
// expressed as fractions of the strip rather than hardcoded pixel counts.

var SEPARATION = 0.15    // rocket leads the spark zone by this fraction
var HUE_REPEATS = 3      // warm-band hue ramp repeats along the strip
var WARM_BAND = 0.18     // red -> yellow slice of the hue wheel
var ROCKET_EDGE = 0.998  // wave threshold carving the razor-thin rocket
var SPARK_EDGE = 0.97    // wave threshold for the spark zone
var SPARK_ODDS = 0.05    // per-pixel per-frame chance inside the zone

var t = 0

export function beforeRender(delta) {
  t = time(0.055)                      // ~3.6 s per traversal
}

export function render(index) {
  var pos = index / pixelCount
  var w = wave(t + pos)                // traveling crest: spark zone
  var wr = wave(t + pos + SEPARATION)  // shifted crest: rocket location

  if (wr > ROCKET_EDGE) {
    // Only a couple of pixels sit this close to the crest's peak.
    var hue = frac(pos * HUE_REPEATS) * WARM_BAND
    hsv(hue, 1, 1)
  } else {
    // Sparks: near the unshifted crest AND a fresh random draw clears a
    // high bar. Zero saturation makes them pure white; zero value
    // elsewhere keeps the rest of the strip black.
    var spark = 0
    if (w > SPARK_EDGE && random(1) < SPARK_ODDS) spark = w
    hsv(0, 0, spark)
  }
}
