// name: Synchronized Random Numbers
// Clean-room reimplementation from a prose functional description of the
// community pattern "Synchronized Random Numbers"; original source never
// consulted.

// A coding demo as much as a visual: a deterministic, seedable linear
// congruential generator built on overflow-safe fixed-point modular math,
// so multiple devices seeded identically emit identical sequences. The
// visual is a scrolling rainbow whose per-pixel hues take a tiny zero-mean
// random-walk step every frame, slowly dissolving toward confetti.

// --- Overflow-safe LCG (BSD-rand style, power-of-two modulus) ---------
// 16.16 numbers hold integers only up to 32767, so multiplier * state
// would overflow; use modular add-by-comparison and double-and-add
// (Russian peasant) multiplication instead.

var MODULUS = 16384        // power of two, fits the fixed-point int range
var MULTIPLIER = 181
var INCREMENT = 359

export var prngState = 12345   // settable from outside to synchronize

// (a + b) mod m without ever forming a sum that could overflow.
function addMod(a, b, m) {
  a = a % m
  b = b % m
  if (a >= m - b) return a - (m - b)
  return a + b
}

// (a * b) mod m via binary double-and-add: O(log b), never overflows.
function mulMod(a, b, m) {
  var result = 0
  a = a % m
  while (b > 0) {
    if (b % 2 == 1) result = addMod(result, a, m)
    a = addMod(a, a, m)
    b = floor(b / 2)
  }
  return result
}

// Advance the generator and return a value in [0, 1).
function nextRand() {
  prngState = addMod(mulMod(prngState, MULTIPLIER, MODULUS), INCREMENT, MODULUS)
  return prngState / MODULUS
}

// --- The visual --------------------------------------------------------

var offsets = array(pixelCount)  // per-pixel accumulated hue offsets
var SPARKLINESS = 0.02           // random-walk step amplitude (hue units)
var t1 = 0

export function beforeRender(delta) {
  t1 = time(0.1)   // rainbow scroll, ~6.5 s per cycle
  // One draw per pixel, in index order, so the consumed sequence is
  // deterministic given the seed and pixel count.
  for (var i = 0; i < pixelCount; i++) {
    offsets[i] += nextRand() * SPARKLINESS - SPARKLINESS / 2
  }
}

export function render(index) {
  hsv(t1 + index / pixelCount + offsets[index], 1, 1)
}
