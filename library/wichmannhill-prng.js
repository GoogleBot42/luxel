// name: Wichmann–Hill PRNG
// Clean-room reimplementation from a prose functional description of the
// community pattern "Wichmann–Hill PRNG"; original source never consulted.

// The classic Wichmann–Hill (1982) seedable PRNG, in the published
// low-precision-safe form (multiply-of-remainder minus multiple-of-quotient,
// safe in 16.16 fixed point), plus a throwaway demo renderer: fresh random
// hue and brightness per pixel per frame.

export var seed1 = 0
export var seed2 = 0
export var seed3 = 0

// debug surface
export var drawCount = 0      // rolls over at 32000 to dodge overflow
export var drawCountHigh = 0  // number of rollovers
export var lastDraw = 0       // most recent per-frame draw
export var minSeen = 1        // running min of every value drawn
export var maxSeen = 0        // running max of every value drawn

// Set all three seeds explicitly for a reproducible sequence.
// Seeds should be integers in 1..30000.
function setSeeds(a, b, c) {
  seed1 = a
  seed2 = b
  seed3 = c
}

// One draw: returns a uniform value in [0, 1).
function wichmannHill() {
  // published moduli 30269 / 30307 / 30323, multipliers 171 / 172 / 170
  seed1 = 171 * (seed1 % 177) - 2 * floor(seed1 / 177)
  if (seed1 < 0) seed1 += 30269
  seed2 = 172 * (seed2 % 176) - 35 * floor(seed2 / 176)
  if (seed2 < 0) seed2 += 30307
  seed3 = 170 * (seed3 % 178) - 63 * floor(seed3 / 178)
  if (seed3 < 0) seed3 += 30323

  var v = frac(seed1 / 30269 + seed2 / 30307 + seed3 / 30323)

  drawCount += 1
  if (drawCount >= 32000) {
    drawCount = 0
    drawCountHigh += 1
  }
  minSeen = min(minSeen, v)
  maxSeen = max(maxSeen, v)
  return v
}

// pattern load: independent random seeds in 1..30000
setSeeds(1 + floor(random(30000)), 1 + floor(random(30000)), 1 + floor(random(30000)))

export function beforeRender(delta) {
  lastDraw = wichmannHill()   // one draw per frame, purely to watch it
}

export function render(index) {
  var h = wichmannHill()
  var v = wichmannHill()
  hsv(h, 1, v)
}
