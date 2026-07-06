// name: heatshivers
// Clean-room reimplementation from a prose functional description of the
// community pattern "heatshivers"; original source never consulted.

// Two generators of warm amber pulses: each spawns roughly one pulse per
// second into a small slot pool. A pulse rises and falls on a triangular
// envelope over ~2 s while drifting sideways (one generator drifts right,
// the other left) with a fresh tiny Gaussian-ish jitter every frame — the
// per-frame re-roll is the "shiver" and must not be smoothed. Wherever a
// pulse passes, a heat buffer is floored up to the pulse level and then
// decays exponentially (half-life ~1.5 s, frame-rate independent), leaving
// pure-red trails behind the amber cores with no explicit trail keeping.
// Slot handling here is done correctly (skip dead slots), per the spec's
// recommendation — the original's contiguous-scan bugs are not reproduced.

var SLOTS = 4                        // pulse pool size per generator
var LIFETIME = 2.0                   // seconds per pulse
var DRIFT = 0.4                      // strip-fraction drifted over a lifetime
var HALF_WIDTH = 1 / 12              // spatial triangle half-width (~1/6 total)
var HALF_LIFE = 1.5                  // afterglow half-life, seconds
var JITTER = 0.01                    // scales centered 3-uniform sum (sd ~0.005)

var pulseA = array(pixelCount)       // per-generator intensity fields
var pulseB = array(pixelCount)
var heat = array(pixelCount)         // afterglow
var chR = array(pixelCount)          // composited channels (blue is always 0)
var chG = array(pixelCount)

var aliveA = array(SLOTS)
var birthA = array(SLOTS)
var baseA = array(SLOTS)
var aliveB = array(SLOTS)
var birthB = array(SLOTS)
var baseB = array(SLOTS)

var now = 0
var nextSpawnA = 0
var nextSpawnB = 0.5                 // offset so the two aren't in lockstep

// Advance one generator: spawn, age, and draw its live pulses into buf.
// dir = +1 drifts rightward (spawns in the left ~4/5), -1 the reverse.
function runGenerator(alive, birth, base, buf, dir) {
  var s, i
  arrayReplace(buf, 0)
  for (s = 0; s < SLOTS; s++) {
    if (!alive[s]) continue          // correct slot handling: skip dead slots
    var age = now - birth[s]
    if (age >= LIFETIME) {
      alive[s] = 0
      continue
    }
    var env = 1 - abs(2 * age / LIFETIME - 1)    // triangular rise/fall
    var jitter = (random(1) + random(1) + random(1) - 1.5) * JITTER
    var pos = base[s] + dir * DRIFT * age / LIFETIME + jitter
    var lo = max(0, ceil((pos - HALF_WIDTH) * pixelCount))
    var hi = min(pixelCount - 1, floor((pos + HALF_WIDTH) * pixelCount))
    for (i = lo; i <= hi; i++) {
      var tri = 1 - abs(i / pixelCount - pos) / HALF_WIDTH
      if (tri > 0) buf[i] += env * tri           // overlapping pulses add
    }
  }
}

function spawn(alive, birth, base, leftBiased) {
  var s
  for (s = 0; s < SLOTS; s++) {
    if (!alive[s]) {
      alive[s] = 1
      birth[s] = now
      base[s] = leftBiased ? random(0.8) : 0.2 + random(0.8)
      return 1
    }
  }
  return 0
}

export function beforeRender(delta) {
  var i
  now += delta / 1000

  if (now >= nextSpawnA) {
    spawn(aliveA, birthA, baseA, true)
    nextSpawnA = now + 0.7 + random(0.6)         // ~one pulse per second
  }
  if (now >= nextSpawnB) {
    spawn(aliveB, birthB, baseB, false)
    nextSpawnB = now + 0.7 + random(0.6)
  }

  runGenerator(aliveA, birthA, baseA, pulseA, 1)
  runGenerator(aliveB, birthB, baseB, pulseB, -1)

  // Afterglow: exponential decay floored at the instantaneous pulse max.
  var keep = pow(0.5, delta / (HALF_LIFE * 1000))
  for (i = 0; i < pixelCount; i++) {
    heat[i] = max(heat[i] * keep, max(pulseA[i], pulseB[i]))
    var amber = pulseA[i] + pulseB[i]
    chR[i] = amber + heat[i]         // heat adds pure red only
    chG[i] = amber * 0.8             // amber: full red, ~4/5 green, no blue
  }
}

export function render(index) {
  var r = chR[index]
  var g = chG[index]
  rgb(min(1, r * r), min(1, g * g), 0)           // square = cheap gamma pop
}
