// name: Slime mold palette
// Clean-room reimplementation from a prose functional description of the
// community pattern "Slime mold palette"; original source never consulted.
// Organic paint-blob growth on a 2D panel: from a few random seeds, color
// creeps outward one cell per placement, each new cell chosen where its
// already-painted neighbors best match a random target palette position, so
// similar colors clump into smooth amoeba regions. Fills over a few seconds,
// holds, then (auto) wipes and regrows with a fresh randomly-chosen palette.
// Simulated on a 16x16 virtual canvas; neighbors are the regular-grid 8-ring.
// Injected events seed the cell you poke, so the growth can be steered from
// outside (see the readEvent note by the controls).

var GRID = 16
var CELLS = GRID * GRID
var canvas = array(CELLS)   // -1 = unpainted, else palette position 0..1
var painted = 0

var DRAW = 0
var HOLD = 1
var phase = DRAW
var holdT = 0

var paletteIdx = 0
var NPAL = 6

// --- controls ---------------------------------------------------------
var seedCount = 3
//# min=1 max=10 step=1 default=3
export function inputNumberInitialSeedCount(v) { seedCount = max(1, floor(v)) }

var autoRedraw = 1
//# min=0 max=1 step=1 default=1
export function toggleAutoRedraw(v) { autoRedraw = v > 0.5 }

var redrawDelay = 30
//# min=1 max=120 step=1 default=30
export function inputNumberRedrawDelaySeconds(v) { redrawDelay = max(1, v) }

// The trigger below seeds a random free cell. Injected events (click/drag
// the preview, POST /api/events, or the MQTT event topic — [type, x, y,
// value]) seed the cell you poke instead, so an outside source can steer
// where the blobs start rather than rolling dice.
var ev = array(4)

export function triggerSeedAPixel(v) { plantSeed() }
export function triggerNewDrawing(v) { startDrawing() }
export function triggerRebuildNeighborMap(v) { startDrawing() }
export function triggerRandomPalette(v) {
  paletteIdx = floor(random(NPAL))
  applyPalette(paletteIdx)
}
export function showNumberPalette() { return paletteIdx }

// --- palette library (rainbow listed twice so it is picked more often) --
function applyPalette(idx) {
  if (idx == 1)
    setPalette([0,0,0,0, 0.5,1,0,0, 0.8,1,0.6,0, 1,1,1,0.7])          // lava / fire
  else if (idx == 2)
    setPalette([0,0,0,0.12, 0.5,0,0.45,0.6, 1,0.35,1,1])             // ocean / teal
  else if (idx == 3)
    setPalette([0,0.45,0,0.1, 0.5,1,0.35,0, 1,0.35,0,0.6])           // sunset
  else if (idx == 4)
    setPalette([0,0,0,0, 0.4,0.2,0,0.45, 0.7,1,0,1, 1,1,1,1])        // black-magenta-white
  else
    setPalette([0,1,0,0, 0.17,1,1,0, 0.33,0,1,0, 0.5,0,1,1,
                0.67,0,0,1, 0.83,1,0,1, 1,1,0,0])                    // rainbow (idx 0 & 5)
}

// --- growth -----------------------------------------------------------
function plantSeed() {
  var tries = 0
  while (tries < 40) {
    var ci = floor(random(CELLS))
    if (canvas[ci] < 0) { canvas[ci] = random(0.999); painted += 1; return }
    tries += 1
  }
}

// Seed the cell under a normalized (x, y). Already painted? Repaint it —
// a deliberate poke should always show, and a fresh palette position there
// is what re-colors the region as growth spreads from it.
function plantSeedAt(nx, ny) {
  var gx = clamp(floor(nx * GRID), 0, GRID - 1)
  var gy = clamp(floor(ny * GRID), 0, GRID - 1)
  var ci = gy * GRID + gx
  if (canvas[ci] < 0) painted += 1
  canvas[ci] = random(0.999)
}

function growStep() {
  var target = random(1)
  var best = -1
  var bestDist = 999
  var gy
  for (gy = 0; gy < GRID; gy++) {
    var gx
    for (gx = 0; gx < GRID; gx++) {
      var ci = gy * GRID + gx
      if (canvas[ci] >= 0) continue          // already painted
      var sum = 0
      var cnt = 0
      var dy
      for (dy = -1; dy <= 1; dy++) {
        var dx
        for (dx = -1; dx <= 1; dx++) {
          if (dx == 0 && dy == 0) continue
          var nx = gx + dx
          var ny = gy + dy
          if (nx < 0 || nx >= GRID || ny < 0 || ny >= GRID) continue
          var nv = canvas[ny * GRID + nx]
          if (nv < 0) continue
          sum += abs(nv - target)
          cnt += 1
        }
      }
      if (cnt == 0) continue                 // no painted neighbor: skip
      var d = sum / cnt + hash2(gx, gy) * 0.001   // tiny random tie-break
      if (d < bestDist) { bestDist = d; best = ci }
    }
  }
  if (best >= 0) { canvas[best] = clamp(target, 0, 0.999); painted += 1 }
  else plantSeed()                           // isolated: plant a fresh seed
}

function startDrawing() {
  var i
  for (i = 0; i < CELLS; i++) canvas[i] = -1
  painted = 0
  paletteIdx = floor(random(NPAL))
  applyPalette(paletteIdx)
  var s
  for (s = 0; s < seedCount; s++) plantSeed()
  phase = DRAW
  holdT = 0
}

export function beforeRender(delta) {
  while (readEvent(ev)) plantSeedAt(ev[1], ev[2])
  if (phase == DRAW) {
    var k
    for (k = 0; k < 4 && painted < CELLS; k++) growStep()   // budgeted per frame
    if (painted >= CELLS) { phase = HOLD; holdT = 0 }
  } else {
    holdT += delta / 1000
    if (autoRedraw && holdT > redrawDelay) startDrawing()
  }
}

export function render2D(index, x, y) {
  var ci = floor(y * 15.99) * GRID + floor(x * 15.99)
  var v = canvas[ci]
  if (v < 0) { rgb(0, 0, 0); return }
  paint(v, 1)
}

// 1D fallback so the unmapped check/run still renders something
export function render(index) {
  var v = canvas[floor(index / pixelCount * (CELLS - 1))]
  if (v < 0) { rgb(0, 0, 0); return }
  paint(v, 1)
}

startDrawing()   // seed the first drawing at load
