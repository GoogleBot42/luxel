// name: wanderedges
// Clean-room reimplementation from a prose functional description of the
// community pattern "wanderedges"; original source never consulted.

// Soft green "fireflies": stationary additive pulses swell and fade, but
// the palette peaks at MID intensity and returns to black at MAX, so a
// strengthening pulse hollows out — its core goes dark while two bright
// green rims spread outward, then converge again as it dies. Overlapping
// pulses likewise push past the peak and appear to split into wandering
// bright edges.

var MAX_PULSES = 10
var LIFETIME = 2        // seconds each pulse lives
var SPAWN_GAP = 0.6     // seconds between spawn attempts
var FOOT = 0.2          // spatial footprint as a fraction of the strip

var alive = array(MAX_PULSES)
var birth = array(MAX_PULSES)
var center = array(MAX_PULSES)   // position in pixels (fractional)

var intensity = array(pixelCount)  // per-pixel accumulator
var green = array(pixelCount)      // per-pixel palette lookup result

var clock = 0
var nextSpawn = 0

// Single-hue palette indexed 0..1: black through the low range, rising to
// vivid pure green at the midpoint, then symmetrically back to black.
function palGreen(t) {
  var d = abs(t - 0.5)
  if (d >= 0.35) return 0
  return 1 - d / 0.35
}

export function beforeRender(delta) {
  clock += delta / 1000

  // 1. Clear the accumulator. (arrayReplace splats its value arguments
  //    starting at index 0 — it is NOT an array fill, so it has to be a loop.)
  for (var c = 0; c < pixelCount; c++) intensity[c] = 0

  // 2. Spawn: one new pulse per elapsed spawn window, if a slot is free.
  if (clock >= nextSpawn) {
    for (var s = 0; s < MAX_PULSES; s++) {
      if (!alive[s]) {
        alive[s] = 1
        birth[s] = clock
        center[s] = random(1) * pixelCount
        break
      }
    }
    nextSpawn = clock + SPAWN_GAP
  }

  // 3. Age, kill, and draw every live pulse.
  var halfWidth = FOOT * pixelCount / 2
  for (var p = 0; p < MAX_PULSES; p++) {
    if (!alive[p]) continue
    var age = clock - birth[p]
    if (age >= LIFETIME) {
      alive[p] = 0
      continue
    }
    // Half-sine temporal envelope: smooth rise to a mid-life peak, then fall.
    var env = sin(PI * age / LIFETIME)
    var lo = floor(center[p] - halfWidth)
    var hi = ceil(center[p] + halfWidth)
    if (lo < 0) lo = 0
    if (hi > pixelCount - 1) hi = pixelCount - 1
    for (var i = lo; i <= hi; i++) {
      // Triangular spatial shape: peak at center, linear falloff.
      var shape = 1 - abs(i - center[p]) / halfWidth
      if (shape > 0) intensity[i] += env * shape
    }
  }

  // 4. Clamp to two-thirds scale, stretch back to span the palette exactly,
  //    and look up the green level.
  for (var i = 0; i < pixelCount; i++) {
    var t = min(intensity[i], 2 / 3) * 1.5
    green[i] = palGreen(t)
  }
}

export function render(index) {
  // Channel squaring deepens the darks (gamma-style).
  var g = green[index]
  rgb(0, g * g, 0)
}
