// name: Continuous Cellular Automata
// Clean-room reimplementation from a prose functional description of the
// community pattern "Continuous Cellular Automata"; original source never
// consulted. Concept: Wolfram-style continuous-valued cellular automata.

// A hidden grid (taller and much wider than the display) continuously
// re-derives itself from a persistent seed row: each cell is the average of
// its three parents plus a user offset, fractional part kept. One row is
// recomputed per frame (round-robin), and a display-sized exponential
// moving average hides the row scanning so structures melt smoothly.

var VIEW = 16 // displayed rows/columns (16x16 virtual canvas)
var ROWS = 32 // hidden rows: ~2x displayed
var COLS = VIEW + 2 * ROWS // margins as wide as the row count on each side
var grid = array(ROWS * COLS) // the automaton
var smooth = array(VIEW * VIEW) // display-sized EMA
var SMOOTHING = 0.1

var offsetParam = 0.31 // rule offset, added before the fractional wrap
var pan = 0.5
var depth = 0.5
var diffMode = 0
var seedMode = 0
var curRow = 0
var initialized = 0
var hueDrift = 0

//# min=0 max=1 step=0.001 default=0.31
export function sliderParam(v) {
  offsetParam = v
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderPan(v) {
  pan = v
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderDepth(v) {
  depth = v
}

//# min=0 max=1 step=1 default=0
export function sliderElimStripes(v) {
  diffMode = v > 0.5
}

//# min=0 max=1 step=1 default=0
export function sliderRandomSeeds(v) {
  var mode = v > 0.5
  if (mode != seedMode) {
    seedMode = mode
    seedTopRow()
  }
}

function seedTopRow() {
  for (var c = 0; c < COLS; c++) {
    grid[c] = seedMode ? random(1) : 0
  }
  if (!seedMode) grid[floor(COLS / 2)] = 1 // single centered dot
}

// Row r derives from row r-1: average of the three parents, plus the
// offset, fractional part kept. Edge columns use a 2-parent weighted
// average (double weight straight up), no offset or wrap.
function computeRow(r) {
  var above = (r - 1) * COLS
  var cur = r * COLS
  grid[cur] = (2 * grid[above] + grid[above + 1]) / 3
  grid[cur + COLS - 1] = (grid[above + COLS - 2] + 2 * grid[above + COLS - 1]) / 3
  for (var c = 1; c < COLS - 1; c++) {
    grid[cur + c] = frac(
      (grid[above + c - 1] + grid[above + c] + grid[above + c + 1]) / 3 +
      offsetParam)
  }
}

export function beforeRender(delta) {
  if (!initialized) {
    initialized = 1
    seedTopRow()
    for (var r = 1; r < ROWS; r++) computeRow(r) // one-time full derivation
  }

  // Amortized: recompute a single row per frame, cycling 1..ROWS-1.
  curRow += 1
  if (curRow >= ROWS) curRow = 1
  computeRow(curRow)

  hueDrift = time(0.5) // one slow hue cycle in ~33 s

  // Blend the current viewport into the smoothing grid.
  var row0 = floor(depth * (ROWS - VIEW))
  var col0 = floor(pan * (COLS - VIEW))
  for (var r = 0; r < VIEW; r++) {
    var src = (row0 + r) * COLS + col0
    var dst = r * VIEW
    for (var c = 0; c < VIEW; c++) {
      smooth[dst + c] += SMOOTHING * (grid[src + c] - smooth[dst + c])
    }
  }
}

export function render2D(index, x, y) {
  var col = floor(x * 15.99)
  var row = floor(y * 15.99)
  var i = row * VIEW + col

  var v
  if (diffMode) {
    v = col == 0 ? 0 : abs(smooth[i] - smooth[i - 1])
  } else {
    v = smooth[i]
  }

  // Hue base falls ~a third of the wheel as value rises, with a sinusoidal
  // easing that compresses the ends of the range; the whole arc drifts.
  var eased = (1 - cos(PI * clamp(v, 0, 1))) / 2
  var h = frac(hueDrift + 0.33 - eased / 3)

  hsv(h, 1, v * v)
}
