// name: Fractal Flower
// tags: panel, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/fractal-flower.md — reimagined, not a replica.
// A ring of recursive binary-tree petals drawn as glowing dots. Two branch angles flex
// on incommensurate clocks, so the flower folds between ferns, stars and pinwheels;
// hue runs along branch depth and overlapping deposits blend hues on the wheel.
// Dots land on a canvas sized from the grid (cap 64); trails fade with a half-life and
// an auto-exposure rescales the canvas in place, so one native fill shows it.

var CAP = 64
var bri = array(CAP * CAP), hue = array(CAP * CAP)
var CW = 0, CH = 0

var depth = 6                     // recursion levels
var petals = 6
var ringR = 18                    // ring radius, percent of the panel
var halfLife = 0.4                // trail half-life, seconds
var speed = 1                     // x the morph rate
var pinwheel = 1

//# min=2 max=7 step=1 default=6
export function sliderDepth(v) { depth = clamp(floor(v), 2, 7) }
//# min=1 max=9 step=1 default=6
export function sliderPetals(v) { petals = clamp(floor(v), 1, 9) }
//# min=0 max=45 step=1 default=18
export function sliderRingRadiusPercent(v) { ringR = clamp(v, 0, 45) }
//# min=0.05 max=3 step=0.05 default=0.4
export function sliderTrailHalfLifeSeconds(v) { halfLife = clamp(v, 0.05, 3) }
//# min=0.2 max=3 step=0.1 default=1
export function sliderSpeed(v) { speed = clamp(v, 0.2, 3) }
// On: petals spin in place on the ring. Off: the whole ring revolves.
//# default=1
export function togglePinwheel(v) { pinwheel = v }

var ang1 = -1, ang2 = 0.35, step = 0.03, baseHue = 0, dep = 0.3, peak = 0
var t1 = 0, t2 = 0, t3 = 0, th = 0

// step, deposit, then branch twice; steps shrink toward the tips
function branch(x, y, heading, d) {
  var s = d * step
  x += cos(heading) * s
  y += sin(heading) * s
  if (x >= 0 && x < 1 && y >= 0 && y < 1) {
    var ci = floor(y * CH) * CW + floor(x * CW)
    var b0 = bri[ci], nh = baseHue + d * 0.06
    var dh = mod(nh - hue[ci] + 0.5, 1) - 0.5   // brightness-weighted circular mix
    hue[ci] = hue[ci] + dh * dep / (b0 + dep)
    b0 += dep
    bri[ci] = b0
    if (b0 > peak) peak = b0
  }
  if (d > 1) {
    branch(x, y, heading + ang1, d - 1)
    branch(x, y, heading + ang2, d - 1)
  }
}

export function beforeRender(delta) {
  var w = gridWidth(), h = gridHeight()
  w = w > 0 ? min(w, CAP) : 32; h = h > 0 ? min(h, CAP) : 32
  if (w != CW || h != CH) { CW = w; CH = h; feedback(bri, 0) }

  var dt = min(delta, 100) / 1000, sp = dt * speed
  feedback(bri, pow(0.5, dt / halfLife))
  t1 = mod(t1 + sp / 21, 1); t2 = mod(t2 + sp / 31, 1); t3 = mod(t3 + sp / 47, 1)
  th = mod(th + sp / 7, 1)
  ang1 = -1 + sin(t1 * PI2) * 0.7             // wider swings: ferns and stars, not a donut
  ang2 = 0.35 + sin(t2 * PI2) * 0.6
  var head = sin(t3 * PI2) * PI
  baseHue = th
  step = 0.42 / (depth * (depth + 1) / 2)     // same overall reach at every depth

  peak = 0
  var ring = ringR / 100
  for (var r = 0; r < petals; r++) {
    var slot = r / petals * PI2, a = pinwheel ? slot : slot + head
    var cx = petals > 1 ? 0.5 + cos(a) * ring : 0.5
    var cy = petals > 1 ? 0.5 + sin(a) * ring : 0.5
    var hd = pinwheel ? head + slot : a
    branch(cx, cy, hd + ang1, depth)
    branch(cx, cy, hd + ang2, depth)
  }
  // auto-exposure: never let the hottest cell clip, relax up slowly so density reads
  if (peak > 1.6) { feedback(bri, 1.6 / peak); dep = dep * 1.6 / peak }
  else dep = clamp(dep * (1 + min(dt, 0.1) * 2 * (1.5 - peak)), 0.05, 0.8)
  setGlow(0.55)                               // dots bloom into soft stars
}

export function renderBulk() {
  fillCanvas(hue, 1, bri, CW, CH)
}
