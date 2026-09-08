// name: Raindrops 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Raindrops 2D"; original source never consulted.

// Rain on a still pool seen from above: random bright drops spread as
// expanding circular ripples (the classic two-buffer water recurrence —
// neighbor sum over two minus self, damped), over a static mottled
// blue-green "sea floor" with a slow drifting shimmer on the surface.
// Crests brighten and desaturate toward white foam; troughs darken.
// Fixed-timestep simulation with pointer-swapped buffers keeps wave speed
// independent of frame rate; ripples fade to exactly nothing, and the
// simulation runs edge to edge (mirrored boundary) so a ring reaches the
// outermost row and column instead of stopping one pixel short.

// Whole-frame rendering. The pool was always 16x16 — the water buffers, the
// sea floor and the shimmer all live on that grid — and `render2D` only
// resolved a colour out of it once per LED: a canvas read, two `wave()`s and
// an `hsv()` for every pixel, every frame. On a 64x64 panel that is 4096
// evaluations of a field that only has 256 distinct values.
//
// So the colour resolution moved into the simulation's own resolution.
// `shade()` writes the same hue/saturation/value math into three parallel
// canvas arrays — `hC`, `sC`, `vC`, one entry per CELL — and `renderFrame()`
// issues a single `fillCanvas(hC, sC, vC, W, H)`. `fillCanvas` samples that
// canvas through each pixel's mapped coordinates with nearest sampling,
// which is exactly what the old `floor(y * 15.99) * 16 + floor(x * 15.99)`
// did, so the same 256-cell pool fills a 16x16, 32x32 or 64x64 panel at one
// fixed cost. Everything the pattern *is* — the fixed 30 Hz step, the
// pointer-swapped buffers, the mirrored boundary, the whole-field flatten
// that makes ripples fade to exactly nothing, the drop scheduler and every
// dial — is untouched.
//
// The per-cell terms that never move are baked once at init: `texFloor` is
// the sea floor's contribution and `shArgA`/`shArgB` are the two shimmer
// wave arguments minus their drifting phase, so a frame's work is two
// `wave()`s and a dozen ops per cell. And a pool that has gone exactly flat
// stops costing anything at all: the recurrence over two all-zero buffers
// would only reproduce zero, so it is skipped, and with it the repaint —
// leaving a still pool at a couple of hundred interpreted instructions a
// frame until the next drop (with Shimmer above 0, plus the shading).
//
// Two honest differences from the per-pixel original:
//
//   * The shimmer used to be evaluated at each PIXEL's coordinates, so on a
//     panel bigger than the pool it carried full-panel detail riding over
//     the blocky ripples. It is now a per-cell field like everything else,
//     so above 16x16 it blocks with the water. At 16x16 — the pool's own
//     resolution — the two are the same picture.
//   * A cell's coordinates here are `cx / (W - 1)`, which is the position
//     the engine hands `render2D` for that column only because W is 16.
//     `MapData::coord` is `round(c * 65535 / (W - 1))`, and the naive
//     divide agrees with it just where `65535 / (W - 1)` divides evenly —
//     it does at 16, not at 64 (11 columns differ) or 17 (8 of 17); see
//     `normAxis()` in `aurora-2d.js` for the exact form. Even at 16 the
//     far edge differs: a map normalizes it to 65535/65536, not 1, so the
//     last row's and last column's shimmer arguments are one 16.16 LSB
//     off. That is far below a quantization step and was verified, not
//     assumed.
//
// With no map installed a 2D pattern already got the engine's `ceil(sqrt(n))`
// default grid, and a `renderFrame` that names a coordinate-space builtin
// gets the same one — so a bare strip still runs the pool out across it the
// way it always did (60 px sees an 8x8 grid, 300 px an 18x17).

// 16x16 virtual canvas, row-major
var W = 16
var H = 16
var N = W * H
var bufA = array(N)
var bufB = array(N)
var lap = array(N)         // scratch for one step's mirrored Laplacian
var bgv = array(N)         // static sea-floor field, -0.5..0.5
var prev = bufA            // ping-pong surfaces, swapped by reference
var cur = bufB             // newest state; render and drops both read/write it

// the display canvas: one HSV triple per pool cell, handed to fillCanvas
var hC = array(N)
var sC = array(N)
var vC = array(N)

// per-cell shading constants, baked once at init
var texFloor = array(N)    // the sea floor's contribution to the texture
var shArgA = array(N)      // shimmer wave arguments, minus their drifting
var shArgB = array(N)      // phases (which are added back each frame)

// unique sea floor each boot: one random directional-wave slope
var slope = 0.3 + random(1.4)

function initBackground() {
  var x, y
  for (y = 0; y < H; y++) {
    for (x = 0; x < W; x++) {
      var nx = x / W
      var ny = y / H
      // average several cheap waves: position sum, a random-slope
      // directional wave, radial from a corner, radial from the center
      var v = (triangle((nx + ny) * 1.5)
             + wave(nx * slope + ny)
             + triangle(hypot(nx, ny) * 1.3)
             + wave(hypot(nx - 0.5, ny - 0.5) * 2)) / 4
      bgv[y * W + x] = v - 0.5
    }
  }
}
initBackground()

// The static half of the per-cell shading. `px`/`py` are the cell's own
// normalized coordinates — what the engine handed render2D for that column
// and row — so the shimmer keeps the shape it had, just sampled per cell.
function initShading() {
  var x, y
  for (y = 0; y < H; y++) {
    var py = y / (H - 1)
    for (x = 0; x < W; x++) {
      var px = x / (W - 1)
      var i = y * W + x
      texFloor[i] = bgv[i] * 1.1
      // (1 - px) rather than -px so both wave arguments stay positive
      shArgA[i] = px * 1.6 + py * 0.7
      shArgB[i] = py * 1.9 + (1 - px) * 0.8
    }
  }
}
initShading()

// The simulation runs at a fixed 30 Hz regardless of frame rate; wave speed
// and fade time are expressed against that clock so both read in real units.
var STEP_MS = 33.333
var STEP_HZ = 30
var MAX_CATCHUP = 4        // sim steps a single frame may run (slow-frame guard)
var REST = 0.30            // still-water brightness before gamma
var QUIET = 0.006          // below this peak the pool is declared still

var dropTimer = 0          // elapsed-time accumulator for the drop scheduler
var nextDrop = 0           // randomized countdown to the next raindrop (ms)
var simTimer = 0           // accumulator for the fixed-rate simulation step
var shimA = 0              // drifting phases of the surface shimmer
var shimB = 0.37

// control state — initialized to the declared defaults so an untouched
// pattern renders exactly what the dials say it should
var meanGap = 333.33       // 1000 / (drops per second), ms
var dropSize = 1.2         // splash radius, pixels
var c2 = 0.2844            // (rippleSpeed / STEP_HZ)^2, the wave coefficient
var damp = 0.8778          // per-step energy factor from the fade time
var hueBase = 0.55         // 198 degrees
var texAmt = 0.45          // surface texture, 0..1
var shimRate = 0.2         // shimmer cycles per second

// the canvas is repainted only when something that feeds it moved, and the
// recurrence is skipped outright once the pool is provably at rest
var flat = 0              // both buffers are exactly zero
var shadeDirty = 1
var shadedShimA = -1
var shadedShimB = -1
var shadedTex = -1

// The hue channel is the sea floor plus the dial, and neither animates —
// so it is painted at init and whenever the dial moves, never per frame.
function paintHue() {
  for (var i = 0; i < N; i++) hC[i] = hueBase + bgv[i] * 0.12
}
paintHue()

// Drops per second. The gaps stay random — this sets their mean.
//# min=0.3 max=12 step=0.1 default=3
export function sliderRaindrops(v) {
  meanGap = 1000 / max(v, 0.05)
}

// How fast a ring front travels across the grid, in pixels per second.
// c = speed / stepRate is the CFL number; 0.5 (c^2) is the 2D stability limit.
//# min=2 max=24 step=0.5 default=16
export function sliderRippleSpeed(v) {
  var c = v / STEP_HZ
  c2 = clamp(c * c, 0.0005, 0.5)
}

// Seconds for a ripple to fade from full height to invisible. The oscillating
// modes of this recurrence decay by sqrt(damp) per step, hence the 2/.
//# min=0.4 max=6 step=0.1 default=2
export function sliderRippleFade(v) {
  damp = pow(0.02, 2 / (max(v, 0.2) * STEP_HZ))
}

// Radius of the splash a single drop makes, in pixels.
//# min=0.5 max=2 step=0.1 default=1.2
export function sliderDropSize(v) {
  dropSize = max(v, 0.1)
}

// Base water hue in degrees; the sea floor still mottles a few degrees around it.
//# min=0 max=360 step=1 default=198
export function sliderWaterHue(v) {
  hueBase = v / 360
  paintHue()
}

// How strongly the permanent surface texture (static mottle + shimmer) shows.
//# min=0 max=100 step=1 default=45
export function sliderTexture(v) {
  texAmt = v / 100
}

// Drift rate of the shimmer riding on top of the still water, cycles/minute.
//# min=0 max=20 step=0.5 default=12
export function sliderShimmer(v) {
  shimRate = v / 60
}

function rippleStep() {
  // A pool that is exactly at rest stays there: the recurrence over two
  // all-zero buffers reproduces zero in every cell and the swap exchanges two
  // identical buffers, so this is a skip, not an approximation.
  if (flat) return
  // pointer swap, never copied: prev = newest state, cur = state to overwrite
  var t = prev
  prev = cur
  cur = t
  // Generalized two-buffer water recurrence:
  //   next = 2*now - before + c2 * laplacian(now)
  // c2 = 0.5 collapses to the classic "neighbor sum over two minus self";
  // smaller c2 slows the front without changing the frame rate.
  // Neighbors are MIRRORED at the border (clamped index) rather than the grid
  // being left with a dead one-cell frame, so ripples run to the outermost
  // row and column and reflect off the wall of the pool.
  //
  // Nine native passes instead of 56.5 interpreted instructions per cell.
  // `stencil2D` is the mirrored-border neighbour sum: into a zeroed buffer
  // with kSelf = -4 and kEdge = 1 it forms EXACTLY the `s - 4*p` the hand
  // loop formed, in the same order, so the rest is fixed-point-identical
  // arithmetic rather than an approximation of it. Folding c2 into the
  // stencil (kSelf = 2 - 4*c2) would save three passes and would NOT be:
  // it splits one multiply into two, each with its own 16.16 rounding, and
  // a second-order recurrence integrates that difference frame after frame.
  feedback(lap, 0)
  stencil2D(lap, prev, W, H, -4, 1, 0)
  feedback(lap, c2)
  feedback(cur, -1)
  arrayAdd(cur, prev)
  arrayAdd(cur, prev)
  arrayAdd(cur, lap)
  feedback(cur, damp)
  // the peak the loop used to carry along with it, as its own native pass
  var peak = arrayMaxAbs(cur)
  // Once the whole field is below the visibility floor, flatten it outright.
  // A per-cell deadzone would pump this second-order recurrence and leave the
  // pool simmering forever; a whole-field reset can only remove energy.
  if (peak < QUIET) {
    feedback(prev, 0)
    feedback(cur, 0)
    flat = 1
  }
  shadeDirty = 1
}

function splash(dx, dy) {
  // deposit a drop into the NEWEST buffer, so the next step reads it as source
  // (and it shows as a bright splash pixel this very frame)
  var ox, oy
  for (oy = -2; oy <= 2; oy++) {
    var py = dy + oy
    if (py >= 0 && py < H) {
      for (ox = -2; ox <= 2; ox++) {
        var px = dx + ox
        if (px >= 0 && px < W) {
          var d = hypot(ox, oy)
          if (d <= dropSize) {
            cur[py * W + px] += 1 - d / dropSize * 0.9
          }
        }
      }
    }
  }
  flat = 0
  shadeDirty = 1
}

// The old `render2D` body, run once per CELL into the canvas arrays instead
// of once per pixel: identical colour math, `hC[i]/sC[i]/vC[i]` where it
// called `hsv()`, with the static terms already folded into `texFloor` and
// `shArg*`. The hue channel is `paintHue`'s job and is not touched here.
function shade() {
  for (var i = 0; i < N; i++) {
    var sh = (wave(shArgA[i] + shimA) + wave(shArgB[i] + shimB)) / 2
    var tex = (texFloor[i] + (sh - 0.5) * 1.4) * texAmt
    var v = max(REST + tex * 0.18 + cur[i], 0)
    v = v * v                    // gamma
    // tall crests desaturate toward white foam
    sC[i] = clamp(1.1 - v, 0, 1)
    vC[i] = clamp(v, 0, 1)
  }
}

export function beforeRender(delta) {
  dropTimer += delta
  simTimer += delta

  // random gaps with the requested mean; a while loop so a high rate is not
  // silently clipped to one drop per frame
  var made = 0
  while (dropTimer > nextDrop && made < 4) {
    // anywhere on the grid, edges included
    splash(floor(random(W)), floor(random(H)))
    dropTimer -= nextDrop
    nextDrop = random(2 * meanGap)
    made++
  }
  if (made == 4) dropTimer = 0

  // fixed 30 Hz step decouples wave speed from frame rate
  var steps = 0
  while (simTimer >= STEP_MS && steps < MAX_CATCHUP) {
    rippleStep()
    simTimer -= STEP_MS
    steps++
  }
  if (steps == MAX_CATCHUP) simTimer = 0

  // shimmer phases, wrapped to keep them in range forever
  shimA += delta * shimRate / 1000
  shimB += delta * shimRate * 0.63 / 1000
  if (shimA > 1) shimA -= 1
  if (shimB > 1) shimB -= 1

  // the shimmer drifting and the Texture dial are the two inputs to shade()
  // that no drop or sim step announces
  if (shimA != shadedShimA || shimB != shadedShimB || texAmt != shadedTex) {
    shadeDirty = 1
  }
  if (shadeDirty) {
    shade()
    shadeDirty = 0
    shadedShimA = shimA
    shadedShimB = shimB
    shadedTex = texAmt
  }
}

// One call, one frame: nearest-sampled through the map, exactly the cell
// the old render2D resolved per pixel.
export function renderFrame() {
  fillCanvas(hC, sC, vC, W, H)
}
