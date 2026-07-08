// name: coolaura
// Clean-room reimplementation from a prose functional description of the
// community pattern "coolaura"; original source never consulted.

// Calm aquatic aura on a 1D strip: soft blue-green pulses appear at random
// places, swell, spread outward, and fade; several are alive at once and
// their light adds where they overlap. The whole strip also breathes in and
// out over several seconds. Slow, ambient, never black (background is a dim
// blue). Behavior reimplemented from prose, not the node-graph structure.

const POOL = 10
const LIFETIME = 4        // seconds each pulse lives
const SPAWN_BASE = 1.5    // seconds between spawns (jittered)

var clock = 0             // wall-clock accumulator, seconds
var nextSpawn = 0

var alive = array(POOL)
var birth = array(POOL)
var center = array(POOL)  // 0..1 fraction of strip

var intensity = array(pixelCount)
var rbuf = array(pixelCount)
var gbuf = array(pixelCount)
var bbuf = array(pixelCount)

export function beforeRender(delta) {
  var dt = delta / 1000
  clock += dt

  // 1. clear intensity buffer
  var j = 0
  for (j = 0; j < pixelCount; j++) intensity[j] = 0

  // 2. spawn: if due and a free slot exists, activate it
  if (clock >= nextSpawn) {
    var slot = -1
    var s = 0
    for (s = 0; s < POOL; s++) {
      if (!alive[s]) { slot = s; break }
    }
    if (slot >= 0) {
      alive[slot] = 1
      birth[slot] = clock
      center[slot] = random(1)
    }
    // approx-normal jitter: sum of three uniforms (mean 1.5), centered
    var jitter = (random(1) + random(1) + random(1) - 1.5) * 0.5
    nextSpawn = clock + SPAWN_BASE + jitter
  }

  // 3. update every live pulse (iterate all slots, skip dead ones)
  var i = 0
  for (i = 0; i < POOL; i++) {
    if (!alive[i]) continue
    var age = clock - birth[i]
    if (age >= LIFETIME) { alive[i] = 0; continue }

    var na = age / LIFETIME              // normalized age 0..1
    var tEnv = sin(na * PI)              // half-sine temporal envelope
    var width = 0.1 + na * 1.1           // grows from ~tenth to > whole strip
    var c = center[i]
    var lo = c - width / 2
    var hi = c + width / 2

    for (j = 0; j < pixelCount; j++) {
      var pos = j / pixelCount
      if (pos < lo || pos > hi) continue
      var profile = sin((pos - lo) / width * PI)   // half-sine spatial bump
      intensity[j] += tEnv * profile
    }
  }

  // 4. global breathing envelope (raised cosine, ~8 s, 0..1)
  var breath = (1 - cos(clock / 8 * PI2)) / 2

  // 5. colorize through a two-stop gradient over (breathed) intensity
  for (j = 0; j < pixelCount; j++) {
    var t = clamp(intensity[j] * breath, 0, 1)
    // stop 0: medium blue-cyan   stop 1: green-teal
    rbuf[j] = 0
    gbuf[j] = mix(0.4, 0.9, t)
    bbuf[j] = mix(1.0, 0.3, t)
  }
}

export function render(index) {
  // square each channel: gamma-like curve that deepens darks, softens pulses
  var r = rbuf[index]
  var g = gbuf[index]
  var b = bbuf[index]
  rgb(r * r, g * g, b * b)
}
