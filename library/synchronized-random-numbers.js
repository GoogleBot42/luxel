// name: Synchronized Random Numbers
// Clean-room reimplementation from a prose functional description of the
// community pattern "Synchronized Random Numbers"; original source never
// consulted.

// A coding demo as much as a visual: a deterministic, seedable linear
// congruential generator built on overflow-safe fixed-point modular math,
// so identically seeded devices emit identical sequences. Visually, a
// scrolling rainbow whose per-pixel hues random-walk apart over time.

// LCG parameters: power-of-two modulus small enough that every
// intermediate (< 2*M) fits comfortably in 16.16 integer range.
const M = 16384        // modulus (2^14)
const A = 1373         // multiplier
const C = 101          // increment

// Exported so external code can inspect/seed it — identical seeds on
// synchronized devices yield identical sequences.
export var rngState = 0

// Overflow-safe modular add: detect "sum would exceed the modulus" by
// comparing against the difference instead of forming a possibly
// overflowing sum.
function modAdd(a, b, m) {
  a = a % m
  b = b % m
  if (a >= m - b) return a - (m - b)
  return a + b
}

// Overflow-safe modular multiply: binary double-and-add
// (Russian-peasant), accumulating with modAdd. Logarithmic in b.
function modMul(a, b, m) {
  a = a % m
  b = b % m
  var acc = 0
  while (b >= 1) {
    if (b % 2 >= 1) acc = modAdd(acc, a, m)
    a = modAdd(a, a, m)
    b = floor(b / 2)
  }
  return acc
}

// Advance the generator; returns a fraction in [0, 1)
function nextRandom() {
  rngState = modAdd(modMul(A, rngState, M), C, M)
  return rngState / M
}

// Per-pixel accumulated hue offsets: an unbounded zero-mean random walk.
// Never damped or wrapped — the pattern dissolving toward confetti over
// minutes is the point of the demo. (Hue itself wraps, so it never breaks.)
var offsets = array(pixelCount)

const SPARKLE = 0.01   // walk step: ~1% of the hue wheel per frame

var t = 0

export function beforeRender(delta) {
  t = time(0.06)       // rainbow scrolls in ~4 s
  // one PRNG draw per pixel per frame, in index order, so the consumed
  // sequence is deterministic given seed and pixel count
  for (var i = 0; i < pixelCount; i++) {
    offsets[i] += nextRandom() * SPARKLE - SPARKLE / 2
  }
}

export function render(index) {
  hsv(t + index / pixelCount + offsets[index], 1, 1)
}
