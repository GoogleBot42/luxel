// name: Cellular Automata
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/continuous-cellular-automata.md,
// docs/pattern-specs/cellular-automata-1d.md — reimagined, not a replica.
// One-dimensional automata as a live spacetime waterfall: each generation is
// born on the bottom row of a panel and the history rises, one column per
// cell (up to 64), through a dark-to-bright palette that drifts by
// generation. Continuous mode averages three parents plus a wandering offset
// (nested cones); Elementary mode runs Wolfram rules, touring a curated list
// or pinned by number. A watchdog reseeds anything that dies or freezes. On a
// strip the current generation glows along the LEDs.

var CAP = 64
var cur = array(1024)            // the live generation
var nxt = array(1024)
var tgt = array(1024)            // strip: palette position per cell
var cv = array(CAP * CAP)        // panel: history canvas; strip: smoothed cells
var tbl = array(8)               // elementary rule table by neighbourhood code
var pal = array(20)
var rgbOut = array(3)
var TOUR = [30, 90, 110, 184, 45, 150, 73, 126, 54, 22, 105, 60]
setPalette(pal)

var N = 32                       // cells per generation
var H = 32                       // panel rows of history
var panel = 1
var gwSeen = -1
var ruleNow = 30
var tourAt = 0
var gens = 0
var flat = 0
var acc = 0
var due = 0
var mixK = 1
var hueGen = 0

var mode = 0
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1); gwSeen = -1 }

var curated = 4
// 30 | 90 | 110 | 184 | tour of twelve rules
//# min=0 max=4 step=1 default=4
export function sliderCurated(v) { curated = clamp(floor(v), 0, 4); gwSeen = -1 }

var ruleIn = 0
// any Wolfram rule; 0 defers to Curated
//# min=0 max=255 step=1 default=0
export function inputNumberRule(v) { ruleIn = clamp(floor(v), 0, 255); gwSeen = -1 }

var gensPerSec = 8
//# min=1 max=30 step=1 default=8
export function sliderGenerationsPerSecond(v) { gensPerSec = clamp(v, 1, 30) }

var baseOffset = 0.09
//# min=0.005 max=0.995 step=0.005 default=0.09 unitless-ok
export function sliderRuleOffset(v) { baseOffset = clamp(v, 0.005, 0.995) }

var driftAmt = 0.06
//# min=0 max=0.4 step=0.005 default=0.06
export function sliderRuleDrift(v) { driftAmt = clamp(v, 0, 0.4) }

var sparksPerMin = 24
//# min=0 max=120 step=1 default=24
export function sliderSparksPerMinute(v) { sparksPerMin = clamp(floor(v), 0, 120) }

var baseHue = 0.95
var sat = 1
export function hsvPickerColor(h, s, v) {
  baseHue = h
  sat = s                        // v is ignored: brightness is a device setting
}

function pickRule() {
  if (ruleIn > 0) ruleNow = ruleIn
  else if (curated < 4) ruleNow = TOUR[curated]
  else { ruleNow = TOUR[tourAt]; tourAt = (tourAt + 1) % 12 }
  for (var c = 0; c < 8; c++) tbl[c] = (ruleNow >> c) & 1
}

// a strip (and the rules that need it) starts from noise; a panel shows
// the classic single-cell cones and triangles
function seed() {
  var rnd = !panel || (mode == 1 && (ruleNow == 110 || ruleNow == 184 ||
    ruleNow == 54 || ruleNow == 73 || ruleNow == 105))
  for (var c = 0; c < N; c++) cur[c] = rnd ? (mode == 0 ? random(1) : random(1) < 0.5) : 0
  if (!rnd) cur[floor(N / 2)] = 1
  gens = 0
  flat = 0
}

function setup() {
  var gw = gridWidth()
  gwSeen = gw
  panel = has2DMap()
  N = panel ? (gw > 0 ? min(gw, CAP) : 32) : min(pixelCount, 1024)
  H = panel ? (gw > 0 ? min(gridHeight(), CAP) : 32) : 1
  feedback(cv, 0)
  pickRule()
  seed()
  if (panel) for (var r = 0; r < H; r++) step(r)   // open on a full panel
}

// palette position of a cell: continuous = eased value, elementary = lit by
// which of the eight neighbourhood cases fired
function shade(v, code) {
  if (mode == 0) return v * v * (0.3 + 0.7 * v)
  return v ? 0.5 + code / 14 : 0
}

// one generation; `at` >= 0 writes that panel row directly (the opening fill)
function step(at) {
  var offset = clamp(baseOffset + driftAmt * ((wave(time(1.7)) - 0.5) +
    0.6 * (wave(time(0.61)) - 0.5)) / 1.6, 0.005, 0.995)
  var row = panel ? (H - 1) * N : 0
  var dst = panel ? cv : tgt
  if (at >= 0) row = at * N
  else if (panel) for (var i = 0; i < row; i++) cv[i] = cv[i + N]   // history rises
  var churn = 0
  var sum = 0
  for (var c = 0; c < N; c++) {
    var l = cur[c > 0 ? c - 1 : N - 1]
    var m = cur[c]
    var r = cur[c < N - 1 ? c + 1 : 0]
    var v = 0
    var code = 0
    if (mode == 0) v = frac((l + m + r) / 3 + offset)
    else { code = l * 4 + m * 2 + r; v = tbl[code] }
    nxt[c] = v
    churn += abs(v - m)
    sum += v
    dst[row + c] = shade(v, code)
  }
  if (mode == 0 && random(1) < sparksPerMin / (60 * gensPerSec)) nxt[floor(random(N))] = 1
  var t = cur
  cur = nxt
  nxt = t
  gens++
  // watchdog: frozen, empty or saturated for three generations -> reseed;
  // the elementary tour also moves on after a few screenfuls
  if (churn < N * 0.01 || sum < N * 0.02 || sum > N * 0.98) flat++
  else flat = 0
  if (flat >= 3 || (mode == 1 && ruleIn == 0 && curated == 4 && gens > max(3 * H, 120))) {
    if (mode == 1) pickRule()
    seed()
  }
}

export function beforeRender(delta) {
  var tick = 1000 / gensPerSec
  acc += delta
  due = 0
  while (acc >= tick && due < 4) { acc -= tick; due++ }
  if (acc > tick) acc = tick
  mixK = 1 - pow(0.05, delta / tick)
  // the palette's hue walks slowly with the generations
  hueGen = mod(hueGen + due * 0.003, 1)
  var h = baseHue + hueGen
  for (var k = 0; k < 4; k++) {
    hsv2rgb(h + k * 0.07, sat * (1 - k * 0.22), 0.22 + k * 0.26, rgbOut)
    pal[4 + k * 4] = 0.22 + k * 0.26
    pal[5 + k * 4] = rgbOut[0]
    pal[6 + k * 4] = rgbOut[1]
    pal[7 + k * 4] = rgbOut[2]
  }
  pal[16] = 1                    // last stop at 1: nothing paints black above it
}

export function render(index) {}  // a strip stays mapless (renderBulk paints)

export function renderBulk() {
  if (gridWidth() != gwSeen || has2DMap() != panel) setup()
  for (var k = 0; k < due; k++) step(-1)
  if (panel) paintCanvas(cv, N, H)
  else {
    arrayMix(cv, tgt, mixK)
    paintCanvas(cv, N, 1)
  }
}
