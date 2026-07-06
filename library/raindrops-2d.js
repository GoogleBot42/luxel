// name: Raindrops 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Raindrops 2D"; original source never consulted.

// Rain on a still pool seen from above: random bright drops spread as
// expanding circular ripples (the classic two-buffer water recurrence —
// neighbor sum over two minus self, damped), over a static mottled
// blue-green "sea floor" generated once at startup. Crests brighten and
// desaturate toward white foam; troughs darken. Fixed-timestep simulation
// with pointer-swapped buffers keeps wave speed independent of frame rate.

// 16x16 virtual canvas, row-major
var W = 16
var H = 16
var bufA = array(W * H)
var bufB = array(W * H)
var bg = array(W * H)      // static background hues
var prev = bufA            // ping-pong surfaces, swapped by reference
var cur = bufB

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
      // narrow hue band centered on aqua/blue
      bg[y * W + x] = 0.55 + (v - 0.5) * 0.12
    }
  }
}
initBackground()

var dropTimer = 0          // elapsed-time accumulator for the drop scheduler
var nextDrop = 200         // randomized countdown to the next raindrop (ms)
var simTimer = 0           // accumulator for the fixed-rate simulation step
var maxInterval = 800      // upper bound of the random inter-drop interval

//# min=0 max=1 step=0.01 default=0.5
export function sliderRaindrops(v) {
  // max rate: drops up to ~150 ms apart; min rate: up to ~1.5 s apart
  maxInterval = 150 + (1 - v) * 1350
}

function rippleStep() {
  // pointer swap, never copied
  var t = prev
  prev = cur
  cur = t
  // interior cells only — the one-cell border needs no boundary logic
  var x, y
  for (y = 1; y < H - 1; y++) {
    var row = y * W
    for (x = 1; x < W - 1; x++) {
      var i = row + x
      cur[i] = ((prev[i - 1] + prev[i + 1] + prev[i - W] + prev[i + W]) / 2
                - cur[i]) * 0.95
    }
  }
}

export function beforeRender(delta) {
  dropTimer += delta
  simTimer += delta

  if (dropTimer > nextDrop) {
    // splash one random interior cell to full height in the previous buffer
    var dx = 1 + floor(random(W - 2))
    var dy = 1 + floor(random(H - 2))
    prev[dy * W + dx] = 1
    nextDrop = random(maxInterval)
    dropTimer = 0
  }

  // fixed ~1/30 s step decouples wave speed from frame rate
  if (simTimer > 33) {
    rippleStep()
    simTimer = 0
  }
}

export function render2D(index, x, y) {
  var i = floor(y * 15.99) * 16 + floor(x * 15.99)
  var h = cur[i]                 // wave height; negative in troughs
  var v = 0.3 + h                // modest constant floor
  v = v * v                      // gamma
  // tall crests desaturate toward white foam
  hsv(bg[i], clamp(1.1 - v, 0, 1), v)
}
