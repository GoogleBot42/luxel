// name: Continuous Cellular Automata
// Clean-room reimplementation from a prose functional description of the
// community pattern "Continuous Cellular Automata"; original source never
// consulted. Concept credit: Wolfram's continuous-valued cellular automata.

// A hidden grid of continuous cells (0..1) re-derives itself forever: each
// cell is the average of the three cells above it, plus a user offset,
// fractional-wrapped — the wrap is what makes the fractal triangles. Only
// one row is recomputed per frame (round-robin) to stay inside the frame
// budget, and a display-sized exponential-moving-average grid melts the
// row-scan updates into a smooth continuous morph. The hidden grid is twice
// as tall as the display and carries side margins as wide as it is tall, so
// edge artifacts (which propagate one column per row) can never reach the
// viewport.

var W = 16                           // displayed columns (virtual canvas)
var H = 16                           // displayed rows
var HROWS = H * 2                    // hidden grid rows
var MARGIN = HROWS                   // side margin width (rows == diagonal reach)
var HCOLS = W + 2 * MARGIN           // hidden grid columns

var grid = array(HROWS * HCOLS)      // the automaton, row-major
var smooth = array(W * H)            // display EMA, row-major 16x16
var SMOOTHING = 0.08                 // EMA blend fraction per frame

var param = 0.31                     // rule offset added before frac()
var pan = 0.5                        // viewport horizontal shift
var depth = 0                        // viewport vertical shift
var diffMode = 0                     // show |cell - left neighbor| instead
var seedRandom = 0                   // random top row vs single center dot
var curRow = 1                       // next hidden row to recompute
var drift = 0                        // slow global hue phase

//# min=0 max=1 step=0.005 default=0.31
export function sliderParam(v) {
  param = v
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderPan(v) {
  pan = v
}

//# min=0 max=1 step=0.01 default=0
export function sliderDepth(v) {
  depth = v
}

//# min=0 max=1 step=1 default=0
export function sliderElimStripes(v) {
  diffMode = v > 0.5
}

//# min=0 max=1 step=1 default=0
export function sliderRandomSeeds(v) {
  var want = v > 0.5
  if (want != seedRandom) {          // re-seed on midpoint crossing
    seedRandom = want
    seedTopRow()
  }
}

function seedTopRow() {
  var c
  for (c = 0; c < HCOLS; c++) {
    grid[c] = seedRandom ? random(1) : 0
  }
  if (!seedRandom) grid[floor(HCOLS / 2)] = 1   // single maximal center cell
}

seedTopRow()

export function beforeRender(delta) {
  drift = time(0.5)                  // hue drift: one cycle ~33 s

  // Recompute exactly one hidden row from the row above it.
  var above = (curRow - 1) * HCOLS
  var here = curRow * HCOLS
  var c
  // Edge columns: weighted average of the two available parents (double
  // weight straight up), no offset, no wrap. They live in the margin.
  grid[here] = (2 * grid[above] + grid[above + 1]) / 3
  grid[here + HCOLS - 1] =
    (grid[above + HCOLS - 2] + 2 * grid[above + HCOLS - 1]) / 3
  for (c = 1; c < HCOLS - 1; c++) {
    grid[here + c] = frac(
      (grid[above + c - 1] + grid[above + c] + grid[above + c + 1]) / 3 + param)
  }
  curRow++
  if (curRow >= HROWS) curRow = 1    // row 0 is the persistent seed

  // Blend the current viewport into the display EMA.
  var rowOff = floor(depth * (HROWS - H))
  var colOff = floor(pan * (HCOLS - W))
  var r
  for (r = 0; r < H; r++) {
    var src = (rowOff + r) * HCOLS + colOff
    var dst = r * W
    for (c = 0; c < W; c++) {
      smooth[dst + c] += (grid[src + c] - smooth[dst + c]) * SMOOTHING
    }
  }
}

export function render2D(index, x, y) {
  var gx = floor(x * 15.99)
  var gy = floor(y * 15.99)
  var i = gy * 16 + gx
  var v = smooth[i]
  if (diffMode) {
    v = gx == 0 ? 0 : abs(v - smooth[i - 1])   // first column shows black
  }
  // Base hue falls as value rises (about a third of the wheel), plus the
  // slow global drift; smoothstep easing compresses the ends of the arc so
  // the palette reads band-limited rather than a hard rainbow.
  var h = frac(0.33 * (1 - v) + drift)
  h = h * h * (3 - 2 * h)
  hsv(h, 1, v * v)                   // brightness = value squared
}
