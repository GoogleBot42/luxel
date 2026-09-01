// name: slowflies
// Clean-room reimplementation from a prose functional description of the
// community pattern "slowflies"; original source never consulted.
//
// Soft green fireflies drift slowly along the strip (one generator drifts
// right, one drifts left), swelling and fading over ~3 s, and leave dim
// blue-violet ghost trails that decay over several more seconds.

var POOL = 8            // live-pulse slots per generator
var LIFE = 3            // pulse lifetime, seconds
var HUMP = 0.1          // spatial hump width, fraction of strip
var SPAWN_BASE = 0.8    // seconds between spawns: base + random jitter
var SPAWN_JIT = 0.4     // -> ~1 s +/- 20%
var VEL_MIN = 0.02      // drift speed band, strip lengths per second
var VEL_BAND = 0.04
var TRAIL_HL = 3        // trail half-life, seconds

var flies = array(pixelCount)   // per-frame sum of both generators
var trail = array(pixelCount)   // peak-hold with exponential decay

// pulse pool: slots 0..POOL-1 drift right, POOL..2*POOL-1 drift left
var alive = array(2 * POOL)
var pStart = array(2 * POOL)
var pVel = array(2 * POOL)
var pBirth = array(2 * POOL)
var nextSpawn = array(2)
var now = 0

export function beforeRender(delta) {
  var dt = delta / 1000
  now += dt

  // spawn: each generator emits roughly one fly per second, pool permitting
  for (var g = 0; g < 2; g++) {
    if (now >= nextSpawn[g]) {
      for (var s = 0; s < POOL; s++) {
        var k = g * POOL + s
        if (!alive[k]) {
          alive[k] = 1
          pBirth[k] = now
          pStart[k] = random(1)
          var spd = VEL_MIN + random(VEL_BAND)
          pVel[k] = g == 0 ? spd : -spd
          break
        }
      }
      nextSpawn[g] = now + SPAWN_BASE + random(SPAWN_JIT)
    }
  }

  // rebuild the flies buffer from every live pulse
  feedback(flies, 0)   // clear the buffer (arrayReplace is a splat, not a fill)
  for (var k = 0; k < 2 * POOL; k++) {
    if (!alive[k]) continue
    var age = now - pBirth[k]
    if (age >= LIFE) { alive[k] = 0; continue }
    var c = pStart[k] + age * pVel[k]        // current center, normalized
    // cull once fully off the edge it drifts toward
    if (pVel[k] > 0 && c - HUMP / 2 > 1) { alive[k] = 0; continue }
    if (pVel[k] < 0 && c + HUMP / 2 < 0) { alive[k] = 0; continue }
    var env = triangle(age / LIFE)           // linear rise, peak at half-life
    var lo = floor((c - HUMP / 2) * pixelCount)
    var hi = ceil((c + HUMP / 2) * pixelCount)
    if (lo < 0) lo = 0
    if (hi > pixelCount - 1) hi = pixelCount - 1
    for (var i = lo; i <= hi; i++) {
      var u = (i / pixelCount - (c - HUMP / 2)) / HUMP
      if (u > 0 && u < 1) flies[i] += env * sin(PI * u)  // half-sine arch
    }
  }

  // trails: peak-hold seeded at the flies' brightness, halving every TRAIL_HL s
  var decay = pow(0.5, dt / TRAIL_HL)
  for (var i = 0; i < pixelCount; i++) {
    var d = trail[i] * decay
    trail[i] = d > flies[i] ? d : flies[i]
  }
}

export function render(index) {
  var f = min(flies[index], 1)
  var t = min(trail[index], 1)
  // channelwise max: pure green flies vs dim blue-violet trail (no red)
  var g = max(f, t * 0.08)
  var b = t * 0.85
  rgb(0, g * g, b * b)   // squared channels = simple gamma; trails read faint
}
