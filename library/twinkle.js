// name: Twinkle
// Clean-room reimplementation from a prose functional description of the
// community pattern "Twinkle"; original source never consulted.

// Icy star-field: sparse blue-white glints flare fast and die slow.
// State: per-pixel brightness + age; a spawn accumulator; a round-robin
// segment counter that keeps new stars spread evenly along the strip.

var LIFETIME = 2       // seconds a twinkle lives before reclamation
var SPAWN_EVERY = 0.12 // seconds between spawn bursts
var MAX_BURST = 12     // up to this many new stars per burst
var CURVE_END = 8      // envelope parameter at death (deep in exp tail)
var PEAK = 0.5413      // max of x*x*exp(-x) (at x = 2), for normalization

var bri = array(pixelCount)
var age = array(pixelCount)
var spawnAccum = 0
var segment = 0
var segWidth = pixelCount / MAX_BURST

export function beforeRender(delta) {
  var dt = delta / 1000
  var i
  for (i = 0; i < pixelCount; i++) {
    if (age[i] > LIFETIME) {
      // Dead: reclaim.
      age[i] = 0
      bri[i] = 0
    } else if (age[i] > 0) {
      // Alive: fast-attack / slow-decay envelope (gamma-like bump).
      var x = age[i] / LIFETIME * CURVE_END
      bri[i] = clamp(x * x * exp(-x) / PEAK, 0, 1)
      age[i] += dt
    }
  }

  // Pace spawning: every SPAWN_EVERY seconds, a burst of 0..MAX_BURST stars,
  // each placed randomly inside the next segment (round-robin) so twinkles
  // spread evenly instead of clumping.
  spawnAccum += dt
  if (spawnAccum > SPAWN_EVERY) {
    spawnAccum = 0
    var count = floor(random(MAX_BURST + 1))
    for (i = 0; i < count; i++) {
      var p = floor(segment * segWidth + random(segWidth))
      if (p < pixelCount) {
        age[p] = 0.01  // tiny nonzero age marks it alive (restart if lit)
        bri[p] = 0
      }
      segment = (segment + 1) % MAX_BURST
    }
  }
}

export function render(index) {
  // Cool blue at low saturation: frosty blue-white stars on black.
  hsv(0.63, 0.3, bri[index])
}
