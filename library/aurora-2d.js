// name: Aurora 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Aurora curtains: simplex2 draws the slowly-waving band, simplex3 adds
// vertical shimmer rays, and a palette paints black → green → violet.
// Simplex is smoother than perlin here — no axis-aligned artifacts.

// Whole-frame rendering. The old `render2D` recomputed the ENTIRE curtain
// per LED: `simplex2(x * 1.8, z, 5)` depends on nothing but the column, yet
// a 64x64 panel evaluated it 4096 times a frame for 64 distinct answers.
// `renderFrame()` walks the grid itself, so the band is 64 `simplex2` calls
// and the shimmer stays one `simplex3` per pixel — the same field, sampled
// the same number of times, because dense procedural noise is the one shape
// no bulk op replaces (docs/bulk-render.md, "Scope").
//
// At Cell Size 1 — the default — a lattice cell IS a grid cell, so the
// colour is still the real `paint()`: the brush the palette produces, put
// down with `setPixel(index)`. That keeps the output **byte-identical** to
// the per-pixel original on every rig, which resolving the palette into an
// `hsv` canvas for `fillCanvas` could not: there is no palette-space bulk
// fill, and an RGB→HSV→RGB round trip moves the odd 8-bit channel by one.
// It also needs no canvas at all — three parallel `array(4096)` channels
// would be 12,288 elements against a 10,236 budget, and 96 KB on the S3.
// Above Cell Size 1 the lattice coarsens and each cell is one `fillRect`,
// which is where the throughput is (see the header of the control).
//
// A cell's coordinates are the engine's own: a grid map normalizes column
// `c` of `w` to `round(c * 65535 / (w - 1))` in 16.16, which is NOT
// `c / (w - 1)` — the two disagree on 11 of 64 columns at 64 wide and on 8
// of 17 at 17 wide. `normAxis` reproduces the rounding exactly (`i * EPS`
// is `i` raw units, so the sum is `i * 65535 + floor(d/2)` before an exact
// integer-divisor divide), which is what makes the equality byte-for-byte
// rather than approximate.
//
// With no map installed a 2D pattern already got the engine's `ceil(sqrt(n))`
// default grid, and a `renderFrame` that names `gridWidth()` gets the same
// one — so a bare strip still runs the curtain across it the way it always
// did (60 px sees an 8x8 grid, 300 px an 18x17). On a fixture that is not a
// matrix at all `gridWidth()` reports 0; there are no per-pixel coordinates
// to walk, so the pattern falls back to tiling normalized coordinate space
// with a 32x32 lattice of `fillRect`s — the curtain, at a canvas resolution
// instead of the fixture's.

setPalette([
  0.0,  0,    0,    0,
  0.25, 0,    0.07, 0.03,
  0.55, 0,    0.55, 0.18,
  0.8,  0.15, 0.95, 0.5,
  1.0,  0.75, 0.45, 0.95
])

var MAXC = 128           // widest lattice we keep column tables for

var band = array(MAXC)   // curtain height per column, rebuilt every frame
var x18 = array(MAXC)    // the band noise's x argument     (x * 1.8)
var x6 = array(MAXC)     // the shimmer noise's x argument  (x * 6)
var xa = array(MAXC)     // the cell's left / right edge in mapped x,
var xb = array(MAXC)     // used only when a cell is wider than one pixel

var z = 0
var z4 = 0               // the shimmer's z argument, hoisted out of the loop

var gw = 0               // the installed grid, 0 when the fixture is not one
var gh = 0
var W = 0                // lattice dimensions
var H = 0
var cell = 1             // grid cells per lattice cell (the Cell Size dial)
var eff = 1              // grid cells per lattice cell, after the MAXC clamp
var exact = 0            // 1 when a lattice cell IS one pixel
var built = 0

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
    // a grid wider than the column tables coarsens until it fits
    var s = max(cell, ceil(gw / MAXC))
    eff = s
    W = ceil(gw / s)
    H = ceil(gh / s)
    exact = s == 1
    for (var c = 0; c < W; c++) {
      var g0 = c * s
      var g1 = min(g0 + s, gw) - 1
      var x = normAxis(g0 + floor((g1 - g0) / 2), gw)
      x18[c] = x * 1.8
      x6[c] = x * 6
      xa[c] = normAxis(g0, gw)
      xb[c] = normAxis(g1, gw)
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
      xa[k] = k / W
      xb[k] = (k + 1) / W
    }
  }
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
      var y2 = yv * 2
      for (var c = 0; c < W; c++) {
        var shimmer = 0.6 + 0.4 * simplex3(x6[c], y2, z4, 9)
        var glow = saturate(1 - abs(yv - band[c]) * 2)
        var v = saturate(glow * shimmer * 1.4)
        paint(v, v * v)
        setPixel(i)
        i = i + 1
      }
    }
    return
  }
  // coarser than the fixture: one rectangle per cell, sampled at its middle
  for (var row = 0; row < H; row++) {
    var yv2 = 0
    var ya = 0
    var yb = 0
    if (gw > 0) {
      var s = eff
      var h0 = row * s
      var h1 = min(h0 + s, gh) - 1
      yv2 = normAxis(h0 + floor((h1 - h0) / 2), gh)
      ya = normAxis(h0, gh)
      yb = normAxis(h1, gh)
    } else {
      yv2 = (row + 0.5) / H
      ya = row / H
      yb = (row + 1) / H
    }
    var y2b = yv2 * 2
    for (var k = 0; k < W; k++) {
      var sh2 = 0.6 + 0.4 * simplex3(x6[k], y2b, z4, 9)
      var glow2 = saturate(1 - abs(yv2 - band[k]) * 2)
      var v2 = saturate(glow2 * sh2 * 1.4)
      paint(v2, v2 * v2)
      fillRect(xa[k], ya, xb[k], yb)
    }
  }
}
