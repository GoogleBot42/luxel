// name: Reaction Diffusion
// Curated original (hand-written showcase of the Luxel language/builtins).
// Gray-Scott chemistry on a canvas sized to the panel (up to 48 a side):
// chemical B feeds on A, both diffuse, and spots, worms, dividing cells or
// branching coral organise themselves out of noisy seed blobs. A is stored
// as its depletion D = 1 - A, so feed and kill are plain bulk decays; both
// Laplacians are native stencil2D passes and only the a*b*b reaction is an
// interpreted loop. A culture that dies or has run its course is reseeded.
// A canvas smaller than the panel is upscaled bilinearly.
// Above 4,096 px the grid renders at the canvas size (48x48) and the engine expands
// it bilinearly (setRenderSize): the same picture, without a full-panel paintCanvas.

var CAP = 48
var D = array(CAP * CAP)         // depletion of A (A = 1 - D)
var B = array(CAP * CAP)
var L = array(CAP * CAP)         // Laplacian scratch
var R = array(CAP * CAP)         // reaction a*b*b of the current step
var pal = array(32)
var W = 32
var H = 32
var N = 1024
var gwSeen = -1
var filt = 0
var f = 0.0545
var k = 0.062
var acc = 0
var due = 0
var age = 0
var checkT = 0

var PH = [0.84, 0.46, 0.04, 0.95]   // palette hue per preset
var PS = [0, 0.12, 0.24, 0.36, 0.46, 0.8, 0.92, 1]
var rgbOut = array(3)
setPalette(pal)

// dark -> deep -> vivid -> pale -> white for B in 0..0.46; the top of the
// stop list returns to dark, because paint() wraps and a rounding-negative B
// lands just under 1
function setPal() {
  var h = PH[preset]
  for (var j = 0; j < 8; j++) {
    var q = j < 5 ? j : (j == 5 ? 4 : 0)
    hsv2rgb(h + q * 0.04, 1 - q * 0.18, q == 0 ? 0.04 : min(1, 0.05 + q * 0.32), rgbOut)
    pal[j * 4] = PS[j]
    pal[j * 4 + 1] = rgbOut[0]
    pal[j * 4 + 2] = rgbOut[1]
    pal[j * 4 + 3] = rgbOut[2]
  }
}

var preset = 3
// spots | worms | mitosis | coral
//# min=0 max=3 step=1 default=3
export function sliderPreset(v) {
  preset = clamp(floor(v), 0, 3)
  gwSeen = -1
}

var stepsPerSec = 180
//# min=30 max=480 step=10 default=180
export function sliderStepsPerSecond(v) { stepsPerSec = clamp(v, 30, 480) }

var runSec = 60
//# min=10 max=300 step=5 default=60
export function sliderReseedSeconds(v) { runSec = clamp(v, 10, 300) }

function setPreset() {
  if (preset == 0) { f = 0.03; k = 0.062 }
  else if (preset == 1) { f = 0.046; k = 0.063 }
  else if (preset == 2) { f = 0.034; k = 0.0618 }
  else { f = 0.0545; k = 0.062 }
}

// a few soft blobs of B with ragged noisy edges
function drops(n) {
  for (var d = 0; d < n; d++) {
    var cx = random(W)
    var cy = random(H)
    var r = 2 + random(3)
    for (var y = floor(cy - r); y <= cy + r; y++) {
      for (var x = floor(cx - r); x <= cx + r; x++) {
        if (x >= 0 && y >= 0 && x < W && y < H && hypot(x - cx, y - cy) < r * (0.7 + random(0.5))) {
          B[y * W + x] = 0.5 + random(0.25)
          D[y * W + x] = 0.5
        }
      }
    }
  }
  age = 0
}

function setup() {
  var gw = gridWidth()
  gwSeen = gw
  W = gw > 0 ? min(gw, CAP) : 32
  H = gw > 0 ? min(gridHeight(), CAP) : 32
  N = W * H
  filt = gw > W || gridHeight() > H ? 1 : 0   // 1:1 stays nearest
  setPreset()
  setPal()
  feedback(D, 0)
  feedback(B, 0)
  drops(3 + floor(N / 250))
}

// one explicit Euler step, every term from the old fields:
// D' = D(1 - f) + lap(D) / 2 + r,  B' = B(1 - f - k) + lap(B) / 4 + r
// (half the textbook diffusion: finer features on a small canvas)
function step() {
  for (var i = 0; i < N; i++) {
    var b = B[i]
    R[i] = (1 - D[i]) * b * b
  }
  feedback(L, 0)
  stencil2D(L, D, W, H, -1, 0.2, 0.05)  // 9-point Laplacian, weights sum to 0
  feedback(L, 0.5)
  feedback(D, 1 - f)
  arrayAdd(D, L)
  arrayAdd(D, R)
  feedback(L, 0)
  stencil2D(L, B, W, H, -1, 0.2, 0.05)
  feedback(L, 0.25)
  feedback(B, 1 - f - k)
  arrayAdd(B, L)
  arrayAdd(B, R)
}

var rs = 0                          // 1 once a big grid renders at the canvas size
function bigGrid() {                // above 4,096 px: render at the canvas size, engine expands bilinearly
  if (rs || pixelCount <= 4096 || !has2DMap() || gridWidth() < 1 || gridHeight() < 2) return
  setRenderSize(min(gridWidth(), CAP), min(gridHeight(), CAP), 1)   // gridWidth/gridHeight follow at once
  rs = 1
}

export function beforeRender(delta) {
  bigGrid()
  acc += stepsPerSec * (delta / 1000)
  due = min(floor(acc), 6)
  acc = min(acc - due, 1)
  age += delta / 1000
  checkT += delta / 1000
}

export function renderBulk() {
  if (gridWidth() != gwSeen) setup()
  for (var s = 0; s < due; s++) step()
  if (checkT > 2) {
    checkT = 0
    var mass = arraySum(B) / N
    // a dead culture gets fresh drops; an old one starts over, densely seeded
    if (age > runSec) { feedback(D, 0); feedback(B, 0); drops(6 + floor(N / 120)) }
    else if (mass < 0.002) drops(3 + floor(N / 250))
  }
  paintCanvas(B, W, H, 1, filt)
}
