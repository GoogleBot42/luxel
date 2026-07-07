// name: aurorashivers
// Clean-room reimplementation from a prose functional description of the
// community pattern "aurorashivers"; original source never consulted.

// Two families of soft triangular blobs bloom, drift and shiver along the
// strip — one violet drifting up-strip, one aqua drifting down-strip —
// while a pure-blue afterglow (running max with exponential decay) leaves
// ghostly trails wherever blobs have been.

const SLOTS = 4          // pulse slots per instance
const LIFE = 2           // pulse lifetime, seconds
const DRIFT = 0.2        // total drift over a lifetime, strip fractions
const HALFW = 0.075      // half-width of a blob (~15% of strip total)
const JITTER = 0.006     // per-frame shiver amplitude

// instance A (violet, drifts +): slot state
var aAlive = array(SLOTS)
var aBirth = array(SLOTS)
var aBase = array(SLOTS)
var aNextSpawn = 0

// instance B (aqua, drifts -): slot state
var bAlive = array(SLOTS)
var bBirth = array(SLOTS)
var bBase = array(SLOTS)
var bNextSpawn = 0.5

var pulseA = array(pixelCount)
var pulseB = array(pixelCount)
var glow = array(pixelCount)

var clock = 0

// zero-mean roughly-gaussian shiver from summed uniforms
function shiver() {
  return (random(1) + random(1) + random(1) - 1.5) * JITTER
}

// stamp a triangle bump of height env centered at pos into buf
function stampBump(buf, pos, env) {
  var lo = floor((pos - HALFW) * pixelCount)
  var hi = ceil((pos + HALFW) * pixelCount)
  if (lo < 0) lo = 0
  if (hi > pixelCount - 1) hi = pixelCount - 1
  for (var j = lo; j <= hi; j++) {
    var d = abs(j / pixelCount - pos)
    if (d < HALFW) buf[j] += env * (1 - d / HALFW)
  }
}

export function beforeRender(delta) {
  var dt = delta / 1000
  clock += dt

  // afterglow first: decay + envelope last frame's pulse activity
  var decay = exp(-0.3466 * dt)  // ~2 s half-life
  for (var j = 0; j < pixelCount; j++) {
    glow[j] = max(glow[j] * decay, max(pulseA[j], pulseB[j]))
  }

  // --- instance A: spawns in lower 4/5, drifts toward far end ---
  if (clock >= aNextSpawn) {
    for (var s = 0; s < SLOTS; s++) {
      if (!aAlive[s]) {
        aAlive[s] = 1
        aBirth[s] = clock
        aBase[s] = random(0.8)
        aNextSpawn = clock + 0.8 + random(0.4)
        break
      }
    }
  }
  arrayReplace(pulseA, 0)
  for (var s = 0; s < SLOTS; s++) {
    if (aAlive[s]) {
      var age = clock - aBirth[s]
      if (age > LIFE) {
        aAlive[s] = 0
      } else {
        var u = age / LIFE
        stampBump(pulseA, aBase[s] + DRIFT * u + shiver(), triangle(u))
      }
    }
  }

  // --- instance B: spawns in upper 4/5, drifts toward the start ---
  if (clock >= bNextSpawn) {
    for (var s = 0; s < SLOTS; s++) {
      if (!bAlive[s]) {
        bAlive[s] = 1
        bBirth[s] = clock
        bBase[s] = 0.2 + random(0.8)
        bNextSpawn = clock + 0.8 + random(0.4)
        break
      }
    }
  }
  arrayReplace(pulseB, 0)
  for (var s = 0; s < SLOTS; s++) {
    if (bAlive[s]) {
      var age = clock - bBirth[s]
      if (age > LIFE) {
        bAlive[s] = 0
      } else {
        var u = age / LIFE
        stampBump(pulseB, bBase[s] - DRIFT * u + shiver(), triangle(u))
      }
    }
  }
}

export function render(index) {
  var a = pulseA[index]
  var q = pulseB[index]
  // violet tint from A, aqua tint from B, pure blue from the afterglow
  var r = 0.45 * a
  var g = 0.7 * q
  var b = 0.9 * a + q + glow[index]
  if (r > 1) r = 1
  if (g > 1) g = 1
  if (b > 1) b = 1
  // squaring deepens the fades so triangle envelopes read smooth
  rgb(r * r, g * g, b * b)
}
