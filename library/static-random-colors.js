// name: static random colors
// Clean-room reimplementation from a prose functional description of the
// community pattern "static random colors"; original source never consulted.

// A frozen field of random per-pixel colors. The trick: a deterministic PRNG
// is re-seeded to the same value at the start of every frame, so each pixel
// (drawing values in index order) sees the same numbers every frame -> a
// static image with O(1) memory and no per-pixel array.

var seed = 1 + floor(random(60000))   // true-random seed, fixed for this run
var reg = seed                        // xorshift working register

// draw the next 0..1 value from the register
function nextRand() {
  reg = reg ^ (reg << 7)
  reg = reg ^ (reg >> 9)
  reg = reg ^ (reg << 8)
  if (reg == 0) reg = seed            // never collapse to the zero fixed point
  return frac(abs(reg) / 251)
}

export function beforeRender(delta) {
  reg = seed                          // reset per frame -> stable per-pixel draws
}

export function render(index) {
  var hue = nextRand()
  var raw = nextRand()
  var sat = 1 - raw * raw * raw       // biased toward fully saturated
  hsv(hue, sat, 1)
}
