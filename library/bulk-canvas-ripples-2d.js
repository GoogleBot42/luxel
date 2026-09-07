// name: Bulk Canvas Ripples 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Simulate small, display big. A 16x16 wave field is integrated in
// `beforeRender` entirely with the bulk array builtins (a 3x3 `blur2D` box
// minus the field itself IS the discrete Laplacian, so no hand-rolled
// neighbour loop appears anywhere), and the whole-frame entry shows it with
// a single `fillCanvas()` — nearest-sampled at each pixel's mapped
// coordinate, so the same 256-cell simulation fills a 64x64 panel at the
// same cost.
//
// Note the scalar 1 in the middle of `fillCanvas`: every channel is either
// an array read per pixel or one number broadcast to all of them.

const CW = 16
const CH = 16
const N = CW * CH

var gain = 3
var rate = 1
var drop = 0
var baseHue = 0.55

var cur = array(N)        // this step's surface height
var prev = array(N)       // the previous step's (swapped, never copied)
var lap = array(N)        // scratch: the Laplacian
var disp = array(N)       // |height|, what actually gets displayed
var hueBuf = array(N)

export function beforeRender(delta) {
  var dt = delta * 0.001

  // rain: one impulse every 1/rate seconds, dropped into a single cell
  drop = drop - dt * rate
  if (drop <= 0) {
    drop = drop + 1
    var dx = random(1)
    var dy = random(1)
    // a plus-shaped deposit rather than one cell: a single-cell impulse is
    // almost pure checkerboard, the one mode a discrete wave carries worst
    canvasAdd(cur, CW, dx, dy, 1)
    canvasAdd(cur, CW, dx - 0.07, dy, 0.5)
    canvasAdd(cur, CW, dx + 0.07, dy, 0.5)
    canvasAdd(cur, CW, dx, dy - 0.07, 0.5)
    canvasAdd(cur, CW, dx, dy + 0.07, 0.5)
  }

  // lap = blur(cur) - cur   (arrayMix with t = 1 is a copy)
  arrayMix(lap, cur, 1)
  blur2D(lap, CW, CH, 1)
  arraySub(lap, cur)

  // prev = 2*cur - prev + c2*lap, damped — the wave equation, in place
  arrayScale(prev, -1)
  arrayAdd(prev, cur)
  arrayAdd(prev, cur)
  arrayScale(lap, 1.2)
  arrayAdd(prev, lap)
  feedback(prev, 0.984)

  // swap the two steps: arrays are references, so this moves no data
  var t = cur
  cur = prev
  prev = t

  arrayMapTo(cur, disp, (v) => abs(v) * gain)
  blur2D(disp, CW, CH, 1)      // display-only smoothing; the sim keeps its edge
  arrayMapTo(cur, hueBuf, (v) => baseHue + v * 0.25)
}

export function renderFrame() {
  fillCanvas(hueBuf, 1, disp, CW, CH)
}

//# min=0.5 max=8 step=0.5 default=3
export function sliderBrightness(v) { gain = v }

//# min=0.2 max=4 step=0.2 default=1
export function sliderRain(v) { rate = v }

//# min=0 max=1 step=0.01 default=0.55
export function sliderHue(v) { baseHue = v }
