// name: Cellular Automata 1D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Cellular Automata 1D"; original source never consulted.

// An elementary (Wolfram-style) 1D cellular automaton: each pixel is a
// cell, stepping ~10 generations per second. In age mode long-lived cells
// grow brighter and shift hue with age; rule-index mode paints each cell
// by which of the eight rule cases fired. Periodically reseeds itself.

const stepInterval = 100   // ms between generations (~10/s)

var cellsA = array(pixelCount)
var cellsB = array(pixelCount)
var cur = cellsA          // current generation (reference)
var nxt = cellsB          // next generation (reference, swapped each step)
var hueAge = array(pixelCount)

var maxAge = 1            // running normalization maximum
var stepAcc = 0           // step-timer accumulator (ms)
var lifeAcc = 0           // accumulated lifetime (seconds)

var seeds = 1             // random seed cells at reset (1 = center only)
var rule = 90             // Wolfram rule number, 0..255
var lifetime = 20         // seconds before auto-reseed; 0 = forever
var ruleIndexMode = 0     // 0 = age coloring, 1 = rule-index coloring
var palWidth = 1          // fraction of the hue wheel spanned
var palOffset = 0         // starting hue

function reseed() {
  arrayReplace(cur, 0)
  arrayReplace(hueAge, 0)
  maxAge = 1
  lifeAcc = 0
  if (seeds <= 1) {
    cur[floor(pixelCount / 2)] = 1
  } else {
    for (var i = 0; i < seeds; i++) {
      cur[floor(random(pixelCount))] = 1
    }
  }
}

reseed()   // seed on startup

function step() {
  var newMax = 0
  for (var i = 0; i < pixelCount; i++) {
    // 3-bit neighborhood code, left neighbor as the high bit, wrapping
    var code = cur[(i + pixelCount - 1) % pixelCount] * 4
             + cur[i] * 2
             + cur[(i + 1) % pixelCount]
    // the rule number's binary representation is the transition table
    var alive = (rule & (1 << code)) != 0
    nxt[i] = alive

    if (ruleIndexMode) {
      hueAge[i] = alive ? code / 7 : 0
    } else {
      hueAge[i] = alive ? hueAge[i] + 1 : 0
      if (hueAge[i] > newMax) newMax = hueAge[i]
    }
  }
  maxAge = ruleIndexMode ? 1 : max(newMax, 1)

  // double-buffer swap by reference
  var tmp = cur
  cur = nxt
  nxt = tmp
}

//# min=0 max=1 step=0.01 default=0
export function sliderStartingCells(v) {
  seeds = 1 + floor(v * (pixelCount / 2 - 1))
  reseed()   // touching it forces an immediate restart
}

//# min=0 max=1 step=0.004 default=0.353
export function sliderRule(v) {
  rule = floor(v * 255)
}

//# min=0 max=1 step=0.01 default=0.67
export function sliderLifetime(v) {
  lifetime = v * 30   // seconds; 0 = run forever
}

//# min=0 max=1 step=1 default=0
export function sliderColorMode(v) {
  ruleIndexMode = v >= 0.5
}

//# min=0 max=1 step=0.01 default=1
export function sliderPaletteWidth(v) {
  palWidth = v
}

//# min=0 max=1 step=0.01 default=0
export function sliderPaletteOffset(v) {
  palOffset = v
}

export function beforeRender(delta) {
  if (lifetime > 0) {
    lifeAcc += delta / 1000
    if (lifeAcc > lifetime) reseed()
  }
  stepAcc += delta
  if (stepAcc >= stepInterval) {
    stepAcc = 0
    step()
  }
}

export function render(index) {
  var q = hueAge[index] / maxAge
  // brightness-squared dims young cells and makes veterans pop
  hsv(palOffset + q * palWidth, 1, q * q)
}
