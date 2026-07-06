// name: aurorashivers
// Clean-room reimplementation from a prose functional description of the
// community pattern "aurorashivers"; original source never consulted.

// Two families of soft drifting blooms (violet drifting up-strip, aqua
// drifting down-strip) that quiver with a tiny per-frame gaussian-ish
// jitter, over a pure-blue afterglow that tracks the running max of pulse
// activity and decays exponentially.

const SLOTS = 4            // pulse pool per instance
const LIFETIME = 2.5       // seconds a pulse lives
const DRIFT = 0.2          // fraction of strip drifted over a full lifetime
const HALFW = 0.075        // half-width of the spatial bump (~15% total)
const GLOW_HALFLIFE = 2.5  // seconds
const SPAWN_BASE = 0.8     // next spawn = clock + SPAWN_BASE + random(SPAWN_JITTER)
const SPAWN_JITTER = 0.4

var clock = 0
var nextSpawn = array(2)

var aliveA = array(SLOTS)
var birthA = array(SLOTS)
var baseA = array(SLOTS)
var aliveB = array(SLOTS)
var birthB = array(SLOTS)
var baseB = array(SLOTS)

var pulseA = array(pixelCount)   // violet family field
var pulseB = array(pixelCount)   // aqua family field
var glow = array(pixelCount)     // blue afterglow

// inst 0: spawns in the lower four-fifths, drifts toward the far end.
// inst 1: spawns in the upper four-fifths, drifts toward the start.
function updateInstance(inst, alive, birth, base, buf) {
  var dir = 1
  var spawnLo = 0
  if (inst == 1) {
    dir = -1
    spawnLo = 0.2
  }

  // spawn if due and a slot is free
  if (clock >= nextSpawn[inst]) {
    var s
    for (s = 0; s < SLOTS; s++) {
      if (!alive[s]) {
        alive[s] = 1
        birth[s] = clock
        base[s] = spawnLo + random(0.8)
        nextSpawn[inst] = clock + SPAWN_BASE + random(SPAWN_JITTER)
        break
      }
    }
  }

  var k
  for (k = 0; k < SLOTS; k++) {
    if (!alive[k]) continue
    var age = clock - birth[k]
    if (age >= LIFETIME) {
      alive[k] = 0
      continue
    }
    var life = age / LIFETIME
    var env = triangle(life)  // linear rise to mid-life, linear fall to death

    // small zero-mean roughly-gaussian shiver from summed uniforms
    var jit = (random(1) + random(1) + random(1) - 1.5) * 0.006
    var p = base[k] + dir * DRIFT * life + jit

    // triangular spatial bump; overlapping pulses of one instance sum
    var lo = floor((p - HALFW) * pixelCount)
    var hi = ceil((p + HALFW) * pixelCount)
    if (lo < 0) lo = 0
    if (hi > pixelCount - 1) hi = pixelCount - 1
    var i
    for (i = lo; i <= hi; i++) {
      var amt = 1 - abs(i / pixelCount - p) / HALFW
      if (amt > 0) buf[i] += env * amt
    }
  }
}

export function beforeRender(delta) {
  var dt = delta / 1000
  clock += dt

  // Afterglow first: envelope the previous frame's peak pulse activity and
  // let it decay with a fixed half-life.
  var decay = pow(0.5, dt / GLOW_HALFLIFE)
  var i
  for (i = 0; i < pixelCount; i++) {
    glow[i] = max(glow[i] * decay, max(pulseA[i], pulseB[i]))
  }

  arrayReplace(pulseA, 0)
  arrayReplace(pulseB, 0)
  updateInstance(0, aliveA, birthA, baseA, pulseA)
  updateInstance(1, aliveB, birthB, baseB, pulseB)
}

export function render(index) {
  var a = pulseA[index]   // violet: strong blue, moderate red, no green
  var c = pulseB[index]   // aqua: full blue, strong green, no red
  var w = glow[index]     // pure-blue afterglow
  var r = 0.45 * a
  var g = 0.7 * c
  var b = 0.9 * a + c + 0.7 * w
  // squaring each channel deepens the fades and smooths the triangle ramps
  rgb(r * r, g * g, b * b)
}
