// name: heatshivers
// Clean-room reimplementation from a prose functional description of the
// community pattern "heatshivers"; original source never consulted.

// Warm amber pulses bloom at random spots, swell, drift sideways with a
// tiny per-frame tremble, and fade — leaving a pure-red heat afterglow
// that decays with a ~1.5 s half-life. Two generators drift in opposite
// directions so embers slide past each other.

var SLOTS = 4 // pulse slots per generator
var LIFE = 2.2 // pulse lifetime, seconds
var DRIFT = 0.4 // fraction of strip covered over a full lifetime
var HALF_LIFE = 1.5 // afterglow half-life, seconds
var BUMP_HALF = pixelCount / 12 // spatial triangle half-width (~1/6 total)

// Slot state: 2 generators x SLOTS, packed [gen * SLOTS + slot]
var alive = array(2 * SLOTS)
var birth = array(2 * SLOTS)
var basePos = array(2 * SLOTS)
var nextSpawn = array(2)

var pulseA = array(pixelCount)
var pulseB = array(pixelCount)
var heat = array(pixelCount)
var chR = array(pixelCount)
var chG = array(pixelCount)

var clock = 0

function stepGen(g, buf) {
  var o = g * SLOTS

  // Spawn when the deadline passes and a slot is free (skip live slots).
  if (clock >= nextSpawn[g]) {
    for (var s = 0; s < SLOTS; s++) {
      if (!alive[o + s]) {
        alive[o + s] = 1
        birth[o + s] = clock
        // Generator 0 spawns in the left ~4/5 and drifts right; generator 1
        // spawns in the right ~4/5 and drifts left.
        basePos[o + s] = g == 0
          ? random(0.8 * pixelCount)
          : 0.2 * pixelCount + random(0.8 * pixelCount)
        break
      }
    }
    nextSpawn[g] = clock + 0.7 + random(0.6) // ~one pulse per second
  }

  var dir = g == 0 ? 1 : -1
  for (var s = 0; s < SLOTS; s++) {
    if (!alive[o + s]) continue
    var age = clock - birth[o + s]
    if (age >= LIFE) {
      alive[o + s] = 0
      continue
    }
    // Triangular time envelope: peak at mid-life.
    var env = 1 - abs(2 * age / LIFE - 1)
    // Fresh approximately-Gaussian jitter every frame: the "shiver"
    // (centered sum of three uniforms, sd ~0.5% of the strip).
    var jitter = (random(1) + random(1) + random(1) - 1.5) * 0.01 * pixelCount
    var pos = basePos[o + s] + dir * DRIFT * pixelCount * (age / LIFE) + jitter

    // Triangular spatial bump, clipped at the strip ends; overlaps add.
    var lo = max(0, ceil(pos - BUMP_HALF))
    var hi = min(pixelCount - 1, floor(pos + BUMP_HALF))
    for (var i = lo; i <= hi; i++) {
      var d = 1 - abs(i - pos) / BUMP_HALF
      if (d > 0) buf[i] += env * d
    }
  }
}

export function beforeRender(delta) {
  clock += delta / 1000
  arrayReplace(pulseA, 0)
  arrayReplace(pulseB, 0)
  stepGen(0, pulseA)
  stepGen(1, pulseB)

  // Afterglow: exponential decay (frame-rate independent), floored at the
  // instantaneous pulse maximum — trails with no extra bookkeeping.
  var decay = pow(0.5, delta / (HALF_LIFE * 1000))
  for (var i = 0; i < pixelCount; i++) {
    heat[i] = max(heat[i] * decay, max(pulseA[i], pulseB[i]))
    var p = pulseA[i] + pulseB[i]
    chR[i] = p + heat[i] // pulses + afterglow are both full red
    chG[i] = p * 0.8 // pulses alone add amber warmth
  }
}

export function render(index) {
  var r = chR[index]
  var g = chG[index]
  rgb(r * r, g * g, 0) // square = simple gamma; overlaps clip white-hot
}
