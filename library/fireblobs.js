// name: fireblobs
// Clean-room reimplementation from a prose functional description of the
// community pattern "fireblobs"; original source never consulted.

// Two independent blob-generator layers composited additively. Each layer
// spawns soft half-sine bumps of light at random positions that breathe in
// (triangle envelope) then out over a few seconds. Layer one is orange-gold,
// layer two a dim ember red; their sum reads as calm, in-place fire shimmer.

var MAXBLOBS = 10
var NLAYERS = 2

// per-blob pool state, layer L occupies slots [L*MAXBLOBS .. +MAXBLOBS)
var alive = array(NLAYERS * MAXBLOBS)
var birth = array(NLAYERS * MAXBLOBS)
var bpos  = array(NLAYERS * MAXBLOBS)   // normalized center 0..1

var clock = 0            // seconds
var nextSpawn = array(NLAYERS)

// per-layer params: lifetime (s), spawn interval (s), color rgb weights
var lifetime = array(NLAYERS)
var spawnInt = array(NLAYERS)
var colR = array(NLAYERS)
var colG = array(NLAYERS)
var colB = array(NLAYERS)

lifetime[0] = 3.0;  spawnInt[0] = 0.55; colR[0] = 1.0; colG[0] = 0.5;  colB[0] = 0.0
lifetime[1] = 4.0;  spawnInt[1] = 0.45; colR[1] = 0.5; colG[1] = 0.06; colB[1] = 0.04

// buffers (reallocated lazily to match pixelCount)
var inten = array(1)     // scratch per-layer intensity
var bufR = array(1)
var bufG = array(1)
var bufB = array(1)
var nbuf = 0

function ensureBuffers() {
  if (nbuf != pixelCount) {
    nbuf = pixelCount
    inten = array(nbuf)
    bufR = array(nbuf)
    bufG = array(nbuf)
    bufB = array(nbuf)
  }
}

export function beforeRender(delta) {
  ensureBuffers()
  clock += delta / 1000

  // clear composite output
  for (var i = 0; i < nbuf; i++) {
    bufR[i] = 0; bufG[i] = 0; bufB[i] = 0
  }

  for (var L = 0; L < NLAYERS; L++) {
    var base = L * MAXBLOBS

    // clear this layer's intensity buffer
    for (var i = 0; i < nbuf; i++) inten[i] = 0

    // spawn: if due and a free slot exists
    if (clock >= nextSpawn[L]) {
      var slot = -1
      for (var s = 0; s < MAXBLOBS; s++) {
        if (alive[base + s] < 0.5) { slot = s; break }
      }
      if (slot >= 0) {
        alive[base + slot] = 1
        birth[base + slot] = clock
        bpos[base + slot] = random(1)
      }
      nextSpawn[L] = clock + spawnInt[L]
    }

    var halfw = nbuf * 0.1          // window ~1/5 of the strip wide
    if (halfw < 1) halfw = 1

    // update & paint every live blob
    for (var s = 0; s < MAXBLOBS; s++) {
      if (alive[base + s] < 0.5) continue
      var af = (clock - birth[base + s]) / lifetime[L]   // age fraction
      if (af >= 1) { alive[base + s] = 0; continue }
      var env = 1 - abs(2 * af - 1)                      // triangle: peak mid-life

      var center = bpos[base + s] * (nbuf - 1)
      var lo = ceil(center - halfw)
      var hi = floor(center + halfw)
      if (lo < 0) lo = 0
      if (hi > nbuf - 1) hi = nbuf - 1
      for (var p = lo; p <= hi; p++) {
        var f = (p - (center - halfw)) / (2 * halfw)     // 0..1 across window
        var profile = sin(PI * f)                        // half-sine bump
        inten[p] += env * profile
      }
    }

    // soft-limit: clamp to half scale then rescale to full (graceful plateau)
    for (var i = 0; i < nbuf; i++) {
      var v = min(inten[i], 0.5) * 2
      bufR[i] += v * colR[L]
      bufG[i] += v * colG[L]
      bufB[i] += v * colB[L]
    }
  }
}

export function render(index) {
  var r = bufR[index]
  var g = bufG[index]
  var b = bufB[index]
  rgb(r * r, g * g, b * b)          // squared: deep darks, soft edges
}
