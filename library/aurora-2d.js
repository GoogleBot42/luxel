// name: Aurora 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Aurora curtains: simplex2 draws the slowly-waving band, simplex3 adds
// vertical shimmer rays, and a palette paints black → green → violet.
// Simplex is smoother than perlin here — no axis-aligned artifacts.

// Whole-frame rendering, in two native passes and a per-cell loop.
//
// The old `render2D` recomputed the ENTIRE curtain per LED: `simplex2(x *
// 1.8, z, 5)` depends on nothing but the column, yet a 64x64 panel
// evaluated it 4096 times a frame for 64 distinct answers. `renderFrame()`
// walks the grid itself, so the band is 64 `simplex2` calls.
//
// The shimmer really is one sample per cell, and no bulk op removes that —
// but `fillNoise3D` removes the INTERPRETER around it. One call per lattice
// row fills a whole row of samples natively:
//
//     fillNoise3D(nRow, W, 1, nsx, 0, nox, y, z4, 9)
//       nRow[c] = simplex3(c * nsx + nox, y, z4, 9)
//
// with `h = 1` so the row's y is just the `oy` argument — a row buffer of
// MAXC entries, not a full-panel canvas. What used to be 4096 interpreted
// `simplex3` calls, each behind ~7 instructions of argument assembly and
// two array reads, is 64 native calls that cost nothing but the noise.
//
// A lattice is not quite the map's own normalization: a grid map puts
// column c at `round(c * 65535 / (w - 1))`, and `c * nsx + nox` can only
// reproduce that exactly where `65535 / (w - 1)` divides evenly (it does at
// 16 wide, not at 64). The shimmer is therefore sampled on an EVENLY spaced
// lattice through the same field — a sub-LSB coordinate change, quantified
// in docs/bulk-render.md. Everything the colour is built from — the band,
// the glow's `abs(y - band)`, the palette — still uses `normAxis` exactly.
//
// At Cell Size 1 a lattice cell IS a grid cell, and the colour stays the
// real `paint()`: the brush the palette produces, put down with
// `setPixel(index)`. That is byte-exact and, at one cell per pixel, cheaper
// than any canvas — `paint` + `setPixel` is ~39 interpreted instructions
// per cell less than resolving a palette into an HSV canvas, and it needs
// no canvas arrays at all.
//
// Above Cell Size 1 the lattice is coarser than the fixture, which is where
// a canvas starts paying: `paintCanvas(vC, W, H, bC)` samples the palette
// at `vC[i]`, times `bC[i]`, through every pixel's mapped (x, y) — the same
// colour `paint(v, v * v)` produces, for the whole panel, in one call
// instead of one `fillRect` per cell. Two arrays of at most CANVAS_MAX
// cells, so the lattice coarsens rather than allocating a full-resolution
// canvas (three `array(4096)` HSV channels would be 12,288 elements against
// a 10,236 budget, and ~96 KB on the S3 — docs/bulk-render.md).
//
// With no map installed a 2D pattern already got the engine's `ceil(sqrt(n))`
// default grid, and a `renderFrame` that names `gridWidth()` gets the same
// one — so a bare strip still runs the curtain across it the way it always
// did (60 px sees an 8x8 grid, 300 px an 18x17). On a fixture that is not a
// matrix at all `gridWidth()` reports 0; there are no per-pixel coordinates
// to walk, so the pattern falls back to tiling normalized coordinate space
// with a 32x32 lattice, which `paintCanvas` resolves without a grid.

setPalette([
  0.0,  0,    0,    0,
  0.25, 0,    0.07, 0.03,
  0.55, 0,    0.55, 0.18,
  0.8,  0.15, 0.95, 0.5,
  1.0,  0.75, 0.45, 0.95
])

var MAXC = 128           // widest lattice we keep column tables for
var CANVAS_MAX = 1024    // cells the coarse path's two canvases hold

var band = array(MAXC)   // curtain height per column, rebuilt every frame
var x18 = array(MAXC)    // the band noise's x argument     (x * 1.8)
var x6 = array(MAXC)     // the shimmer noise's x argument  (x * 6)
var nRow = array(MAXC)   // one lattice row of shimmer samples

// the coarse path's canvas: palette position and brightness per cell
var vC = array(CANVAS_MAX)
var bC = array(CANVAS_MAX)

var z = 0
var z4 = 0               // the shimmer's z argument, hoisted out of the loop

var gw = 0               // the installed grid, 0 when the fixture is not one
var gh = 0
var W = 0                // lattice dimensions
var H = 0
var cell = 1             // grid cells per lattice cell (the Cell Size dial)
var eff = 1              // grid cells per lattice cell, after the clamps
var exact = 0            // 1 when a lattice cell IS one pixel
var built = 0

// the shimmer lattice: cell k's x argument is k * nsx + nox
var nsx = 0
var nox = 0

// One 16.16 LSB. Written as two exact divides rather than a decimal literal
// so it is raw 1 whatever the lexer rounds.
var EPS = 1 / 256 / 256

// The engine's grid normalization, reproduced exactly: `MapData::coord`
// returns raw `(i * 65535 + (n - 1) / 2) / (n - 1)` for axis position i.
function normAxis(i, n) {
  if (n <= 1) return 0
  var d = n - 1
  return (i - i * EPS + floor(d / 2) * EPS) / d
}

function build() {
  gw = gridWidth()
  gh = gridHeight()
  if (gw > 0 && gh > 0) {
    // a grid wider than the column tables coarsens until it fits, and the
    // coarse path coarsens further until its canvas fits
    var s = max(cell, ceil(gw / MAXC))
    W = ceil(gw / s)
    H = ceil(gh / s)
    while (s > 1 && W * H > CANVAS_MAX) {
      s = s + 1
      W = ceil(gw / s)
      H = ceil(gh / s)
    }
    eff = s
    exact = s == 1
    for (var c = 0; c < W; c++) {
      var g0 = c * s
      var g1 = min(g0 + s, gw) - 1
      var x = normAxis(g0 + floor((g1 - g0) / 2), gw)
      x18[c] = x * 1.8
      x6[c] = x * 6
    }
  } else {
    // not a matrix: tile normalized coordinate space instead
    gw = 0
    W = ceil(32 / cell)
    H = W
    exact = 0
    for (var k = 0; k < W; k++) {
      var xc = (k + 0.5) / W
      x18[k] = xc * 1.8
      x6[k] = xc * 6
    }
  }
  // The linear lattice `fillNoise3D` samples, fitted to the column table's
  // two ends so the far edge lands where the table says it does.
  nox = x6[0]
  nsx = W > 1 ? (x6[W - 1] - x6[0]) / (W - 1) : 0
  built = 1
}

// How many pixels across one lattice cell is. 1 is native — every pixel gets
// its own shimmer sample and the frame is exactly what the per-pixel version
// drew. Larger trades that detail for speed: the shimmer field is only ~6
// lattice units wide across the whole panel, so it survives being sampled
// every 2-4 pixels far better than the curtain's edge does, which is what
// visibly blocks first.
//# min=1 max=4 step=1 default=1
export function sliderCellSize(v) {
  var s = clamp(floor(v), 1, 4)
  if (s != cell) {
    cell = s
    built = 0
  }
}

export function beforeRender(delta) {
  z = (z + delta * 0.00015) % 1024  // slow drift along one noise axis
  if (!built) build()
  z4 = z * 4
  for (var c = 0; c < W; c++) band[c] = 0.45 + simplex2(x18[c], z, 5) * 0.25
}

export function renderFrame() {
  if (exact) {
    // one lattice cell per pixel: the palette brush, straight onto the index
    var i = 0
    for (var r = 0; r < H; r++) {
      var yv = normAxis(r, gh)
      fillNoise3D(nRow, W, 1, nsx, 0, nox, yv * 2, z4, 9)
      for (var c = 0; c < W; c++) {
        var shimmer = 0.6 + 0.4 * nRow[c]
        var glow = saturate(1 - abs(yv - band[c]) * 2)
        var v = saturate(glow * shimmer * 1.4)
        paint(v, v * v)
        setPixel(i)
        i = i + 1
      }
    }
    return
  }
  // coarser than the fixture: build the palette canvas, then one call
  for (var row = 0; row < H; row++) {
    var yv2 = 0
    if (gw > 0) {
      var s = eff
      var h0 = row * s
      var h1 = min(h0 + s, gh) - 1
      yv2 = normAxis(h0 + floor((h1 - h0) / 2), gh)
    } else {
      yv2 = (row + 0.5) / H
    }
    fillNoise3D(nRow, W, 1, nsx, 0, nox, yv2 * 2, z4, 9)
    var base = row * W
    for (var k = 0; k < W; k++) {
      var sh2 = 0.6 + 0.4 * nRow[k]
      var glow2 = saturate(1 - abs(yv2 - band[k]) * 2)
      var v2 = saturate(glow2 * sh2 * 1.4)
      vC[base + k] = v2
      bC[base + k] = v2 * v2
    }
  }
  paintCanvas(vC, W, H, bC)
}
