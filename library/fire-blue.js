// name: fire - blue
// Clean-room reimplementation from a prose functional description of the
// community pattern "fire - blue"; original source never consulted. The
// classic Fire2012-style 1D "heat" cellular simulation recolored cold:
// icy blue tongues lick up the strip (white-hot base -> cyan -> deep blue
// -> black). The sim steps on a fixed ~50 Hz tick decoupled from the
// render frame rate, so flicker speed is frame-rate independent.

var PSIZE = 200               // palette entries
var STEP = 20                 // ms per simulation tick (~50 Hz)

// --- tunable constants (could be sliders) ---
var COOL = 0.022              // heat lost per cell per tick (max of a uniform draw)
var SPARK = 0.5               // per-tick probability of a new spark

// --- state ---
var inited = 0
var N = 0
var heat = 0
var accum = 0
var palR = array(PSIZE)
var palG = array(PSIZE)
var palB = array(PSIZE)

//# min=0.005 max=0.08 step=0.001 default=0.022
export function sliderCooling(v) { COOL = v }

//# min=0 max=1 step=0.02 default=0.5
export function sliderSparking(v) { SPARK = v }

//# min=5 max=60 step=1 default=20
export function sliderTickMs(v) { STEP = v }

function buildPalette() {
  var i = 0
  while (i < PSIZE) {
    var t = i / (PSIZE - 1)          // 0..1 heat
    // each channel ramps over one third then holds full
    palB[i] = clamp(t * 3, 0, 1)                 // blue in first third
    palG[i] = clamp((t - 0.3333) * 3, 0, 1)      // cyan in second third
    palR[i] = clamp((t - 0.6667) * 3, 0, 1)      // white-hot in last third
    i += 1
  }
}

function step() {
  var i = 0
  // 1: cooling
  while (i < N) {
    heat[i] = clamp(heat[i] - random(COOL), 0, 1)
    i += 1
  }
  // 2: upward drift (cell two-below counted twice)
  i = N - 1
  while (i >= 2) {
    heat[i] = (heat[i - 1] + heat[i - 2] + heat[i - 2]) / 3
    i -= 1
  }
  // 3: sparking in the bottom tenth
  if (random(1) < SPARK) {
    var y = floor(random(max(1, N * 0.1)))
    heat[y] = clamp(heat[y] + 0.6 + random(0.4), 0, 1)
  }
}

export function beforeRender(delta) {
  if (!inited) {
    N = max(1, pixelCount)
    heat = array(N)
    buildPalette()
    inited = 1
  }
  accum += delta
  var guard = 0
  while (accum >= STEP && guard < 8) {   // cap catch-up work per frame
    step()
    accum -= STEP
    guard += 1
  }
}

export function render(index) {
  var pi = clamp(floor(heat[index] * (PSIZE - 1)), 0, PSIZE - 1)
  rgb(palR[pi], palG[pi], palB[pi])
}
