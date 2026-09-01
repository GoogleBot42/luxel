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

// 16x16 virtual canvas, row-major
var W = 16
var H = 16
var bufA = array(W * H)
var bufB = array(W * H)
var bgv = array(W * H)     // static sea-floor field, -0.5..0.5
var prev = bufA            // ping-pong surfaces, swapped by reference
var cur = bufB             // newest state; render and drops both read/write it

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
  var peak = 0
  var x, y
  for (y = 0; y < H; y++) {
    var row = y * W
    var up = y > 0 ? row - W : row
    var dn = y < H - 1 ? row + W : row
    for (x = 0; x < W; x++) {
      var i = row + x
      var li = x > 0 ? i - 1 : i
      var ri = x < W - 1 ? i + 1 : i
      var p = prev[i]
      var s = prev[li] + prev[ri] + prev[up + x] + prev[dn + x]
      var v = (2 * p - cur[i] + c2 * (s - 4 * p)) * damp
      cur[i] = v
      var m = abs(v)
      if (m > peak) peak = m
    }
  }
  // Once the whole field is below the visibility floor, flatten it outright.
  // A per-cell deadzone would pump this second-order recurrence and leave the
  // pool simmering forever; a whole-field reset can only remove energy.
  if (peak < QUIET) {
    for (x = 0; x < W * H; x++) {
      prev[x] = 0
      cur[x] = 0
    }
  }
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
}

export function render2D(index, x, y) {
  var i = floor(y * 15.99) * 16 + floor(x * 15.99)
  var h = cur[i]                 // wave height; negative in troughs
  // permanent water texture: the static sea floor plus a slow shimmer that
  // drifts across it, both riding under the ripples at full panel resolution
  // (1 - x) rather than -x so both wave arguments stay positive
  var sh = (wave(x * 1.6 + y * 0.7 + shimA) + wave(y * 1.9 + (1 - x) * 0.8 + shimB)) / 2
  var tex = (bgv[i] * 1.1 + (sh - 0.5) * 1.4) * texAmt
  var v = max(REST + tex * 0.18 + h, 0)
  v = v * v                      // gamma
  // tall crests desaturate toward white foam
  hsv(hueBase + bgv[i] * 0.12, clamp(1.1 - v, 0, 1), clamp(v, 0, 1))
}
