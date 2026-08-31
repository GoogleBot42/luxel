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
function reseedRandom() {
  setSeeds(1 + floor(random(30000)), 1 + floor(random(30000)), 1 + floor(random(30000)))
}
reseedRandom()

// --- demo renderer -------------------------------------------------------
// Tunables. The top-level values reproduce the renderer the port shipped
// with (fresh full-wheel noise every frame), so an untouched pattern renders
// exactly as before.
var flickerPeriod = 1 / 60   // seconds between redraws (60 Hz = every frame)
var baseHue = 0              // rotate the noise around the color wheel
var spread = 1               // fraction of the wheel the noise covers

var pxHue = array(pixelCount)
var pxVal = array(pixelCount)
var accum = 1        // >= the longest flicker period, so frame 1 always draws
var redraw = 1

// Seed for a reproducible sequence: 0 re-randomizes, any other value picks a
// fixed, repeatable stream (all three published seeds derived from it).
//# min=0 max=10000 step=1 default=0
export function inputNumberSeed(v) {
  var n = floor(clamp(v, 0, 10000))
  if (n < 1) reseedRandom()
  else setSeeds(n, n + 10007, n + 20011)
}

// How often the static is redrawn, in redraws per second. At 60 Hz every
// frame is fresh (the original behavior); low values hold each field of
// noise long enough to actually look at it.
//# min=1 max=60 step=1 default=60
export function sliderFlickerRateHz(v) { flickerPeriod = 1 / clamp(v, 1, 60) }

// Where the noise's color band starts, in degrees around the color wheel.
//# min=0 max=360 step=1 default=0
export function sliderBaseHueDegrees(v) { baseHue = clamp(v, 0, 360) / 360 }

// How much of the color wheel the noise spans: 100% is full rainbow static,
// 0% collapses it to single-hue brightness static.
//# min=0 max=100 step=1 default=100
export function sliderColorSpreadPercent(v) { spread = clamp(v, 0, 100) / 100 }

export function beforeRender(delta) {
  lastDraw = wichmannHill()   // one draw per frame, purely to watch it

  accum += delta / 1000
  if (accum >= flickerPeriod) {
    accum = 0
    redraw = 1
  } else {
    redraw = 0
  }
}

export function render(index) {
  if (redraw) {
    pxHue[index] = wichmannHill()
    pxVal[index] = wichmannHill()
  }
  hsv(baseHue + spread * pxHue[index], 1, pxVal[index])
}
