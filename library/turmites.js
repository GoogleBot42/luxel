// name: Turmites
// Curated original (hand-written showcase of the Luxel language/builtins).
// Langton's ant and its many-coloured cousins: one walker reads its cell's
// colour, turns left or right as the rule string says, bumps the colour and
// steps on. Some rules grow symmetric blooms, some chaotic clouds, some
// suddenly lay an endless diagonal highway. Runs end when the ant walks off
// the panel (or times out), hold, dissolve, and the next rule starts with a
// fresh colour arc. Only the cell that changed is drawn — the frame persists.

var CAP = 64
var st = array(CAP * CAP)       // colour index per cell
var turn = array(12)            // 1 = turn right on this colour
var cr = array(12)
var cg = array(12)
var cb = array(12)
var rgbOut = array(3)
// rule strings as bit masks (bit j set = R on colour j) and their lengths:
// RL (Langton), RLR (chaos), LLRR (bloom), RLLR (brain), LRRRRRLLR (square),
// LLRRRLRLRLLR (twisting highway), RRLLLRLLLRRR (growing triangle)
var MASKS = [1, 5, 12, 9, 318, 2396, 3619]
var LENS = [2, 3, 4, 4, 9, 12, 12]

var BW = 32
var BH = 32
var cs = 1                       // panel pixels per cell
var gw = 0
var gwSeen = -1
var K = 2
var ax = 0
var ay = 0
var dir = 0                      // 0 up, 1 right, 2 down, 3 left
var phase = 0                    // 0 walking, 1 holding, 2 dissolving
var phaseT = 0
var runT = 0
var cur = floor(random(7))
var budget = 0
var fadeK = 0.88

var ruleSel = 0
//# min=0 max=7 step=1 default=0
export function sliderRule(v) {
  ruleSel = clamp(floor(v), 0, 7)       // 0 tours the list, 1..7 pins one
  gwSeen = -1
}

var stepsPerSec = 6000
//# min=100 max=30000 step=100 default=6000
export function sliderStepsPerSecond(v) { stepsPerSec = clamp(v, 100, 30000) }

var holdSec = 3
//# min=0 max=20 step=0.5 default=3
export function sliderHoldSeconds(v) { holdSec = clamp(v, 0, 20) }

function newRun() {
  gw = gridWidth()
  gwSeen = gw
  cs = gw > 0 ? ceil(max(gw, gridHeight()) / CAP) : 1
  BW = gw > 0 ? floor(gw / cs) : 32
  BH = gw > 0 ? floor(gridHeight() / cs) : 32
  cur = ruleSel > 0 ? ruleSel - 1 : (cur + 1) % 7
  K = LENS[cur]
  var hue = random(1)
  var arc = K > 2 ? 0.55 / (K - 2) : 0
  for (var j = 0; j < K; j++) {
    turn[j] = (MASKS[cur] >> j) & 1
    hsv2rgb(hue + (j - 1) * arc, 0.85 - 0.25 * (j % 2), 1, rgbOut)
    cr[j] = rgbOut[0]; cg[j] = rgbOut[1]; cb[j] = rgbOut[2]
  }
  hsv2rgb(hue, 0.7, 0.1, rgbOut)        // colour 0, once walked on, is a dark tint
  cr[0] = rgbOut[0]; cg[0] = rgbOut[1]; cb[0] = rgbOut[2]
  feedback(st, 0)
  ax = floor(BW / 2)
  ay = floor(BH / 2)
  dir = 0
  phase = 0
  runT = 0
  clear()
}

function paintCell(x, y) {
  if (gw <= 0) {
    fillRect(x / BW, y / BH, (x + 0.9) / BW, (y + 0.9) / BH)
  } else if (cs == 1) {
    setPixel(gridIndex(x, y))
  } else {
    for (var dy = 0; dy < cs; dy++) {
      for (var dx = 0; dx < cs; dx++) setPixel(gridIndex(x * cs + dx, y * cs + dy))
    }
  }
}

function walk(n) {
  for (var k = 0; k < n; k++) {
    var i = ay * BW + ax
    var s = st[i]
    dir = (dir + (turn[s] ? 1 : 3)) % 4
    s = (s + 1) % K
    st[i] = s
    rgb(cr[s], cg[s], cb[s])
    paintCell(ax, ay)
    if (dir == 0) ay--
    else if (dir == 1) ax++
    else if (dir == 2) ay++
    else ax--
    if (ax < 0 || ay < 0 || ax >= BW || ay >= BH) { phase = 1; phaseT = 0; return }
  }
}

export function beforeRender(delta) {
  budget = min(budget + stepsPerSec * (delta / 1000), 2000)   // no 16.16 overflow
  phaseT += delta / 1000
  runT += delta / 1000
  fadeK = pow(0.88, delta / 16)
}

export function renderBulk() {
  if (gridWidth() != gwSeen) newRun()
  if (phase == 0) {
    var n = floor(budget)
    budget -= n
    walk(n)
    // a run that never leaves the panel still ends: a minute is plenty
    if (runT > 60) { phase = 1; phaseT = 0 }
    if (phase == 0) { rgb(1, 1, 1); paintCell(ax, ay) }
  } else {
    budget = 0
    if (phase == 1 && phaseT > holdSec) { phase = 2; phaseT = 0 }
    if (phase == 2) {
      fade(fadeK)
      if (phaseT > 0.8) newRun()
    }
  }
}
