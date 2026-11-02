// name: Line Dancer
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/line-dancer-2d.md — reimagined, not a replica.
// A rainbow ribbon whips and coils round the centre: a radius-dependent shear smears
// the picture into a dancing line, a breathing zoom widens and tightens the stripes
// (floored, so it never melts into a flat wash), and Reflections folds it into a
// seamless mirrored kaleidoscope. Each pixel's (folded) angle and radius are tabled
// once; the shear depends on radius alone, so it is a small table rebuilt per frame.
// Above 4,096 px a grid renders at <= 4,096 cells (64x64 on 128x128), expanded
// bilinearly by the engine (setRenderSize): a quarter of the work, no 2x2 blocks.

var TN = min(pixelCount, 4096), NR = 256
var A = array(TN), R = array(TN)  // per pixel: folded angle (rad), radius in RL bins
var RL = array(NR + 1), RH = array(NR + 1)  // per radius bin: shear angle, hue offset
var mode = 0, cw = 0, ch = 0, TS = 0, key = -1, aspX = 1, aspY = 1
var rs = 0, rsW = 0, rsH = 0      // 1 once a big grid renders at rsW x rsH

var speed = 1                     // x the base dance rate
var twist = 1.75                  // shear reach
var refl = 1                      // mirrored folds; 1 = off
var clock = 0, spin = 0, ph = 0, kz = 0, hz = 0, hc = 0

//# min=0.2 max=3 step=0.1 default=1
export function sliderSpeed(v) { speed = clamp(v, 0.2, 3) }

// How far out from the centre the coiling reaches, percent of its range.
//# min=0 max=100 step=1 default=67
export function sliderTwistPercent(v) { twist = 1.25 + clamp(v, 0, 100) * 0.0075 }

// Kaleidoscope: 1 is off, n folds the picture into n mirrored pairs of wedges.
//# min=1 max=7 step=1 default=1
export function sliderReflections(v) { refl = clamp(floor(v), 1, 7); key = -1 }

function polar(x, y) {             // -> angle (rad, folded) and radius bin
  x = (x - 0.5) * aspX; y = (y - 0.5) * aspY
  var a = atan2(y, x) + PI
  if (refl > 1) {                   // mirror alternate wedges: seamless at every fold
    var wedge = PI / refl, k = floor(a / wedge)
    a = a - k * wedge
    if (k % 2) a = wedge - a
  }
  ph = a
  return min(hypot(x, y), 0.999) * NR
}

export function beforeRender(delta) {
  if (!rs && pixelCount > TN && has2DMap() && gridWidth() > 0 && gridHeight() > 1) {
    var gw = gridWidth(), gh = gridHeight()
    rsW = min(gw, floor(64 * sqrt(gw / gh) + 0.001)); rsH = min(gh, floor(TN / rsW))
    setRenderSize(rsW, rsH, 1)      // pixelCount is rsW * rsH from here on
    rs = 1
  }
  var w = rs ? rsW : gridWidth(), h = rs ? rsH : gridHeight()
  if (w * 4096 + h != key) {
    key = w * 4096 + h
    var m = max(max(w, h), 1)
    aspX = w > 0 ? w / m : 1; aspY = h > 0 ? h / m : 1
    // 1: table per pixel by map; 2: render-size table built by cell; 0: no table
    mode = rs ? 2 : (pixelCount <= TN ? 1 : 0)
    cw = w; ch = h
    TS = mode == 1 ? pixelCount : (mode == 2 ? cw * ch : 0)
    for (var i = 0; i < TS; i++) {
      if (mode == 1) R[i] = polar(pixelCoord(i, 0), pixelCoord(i, 1))
      else R[i] = polar((i % cw + 0.5) / cw, (floor(i / cw) + 0.5) / ch)
      A[i] = ph
    }
  }
  var dt = min(delta, 100) / 1000
  clock = mod(clock + dt * 4.6 * speed, PI2 * 64)
  spin = mod(spin + dt * 0.15, PI2)
  var z = triangle(time(0.076))      // ~5 s breath, floored above the old melt
  var zoom = 0.4 + 0.6 * z * z * (3 - 2 * z)
  kz = 5 * zoom / NR; hz = zoom / NR; hc = zoom
  for (i = 0; i <= NR; i++) {
    var d = twist - i / NR * 2.2
    var ang = d * d * sin(d + clock)
    RL[i] = ang + spin
    RH[i] = ang / PI2 + hc
  }
}

export function render2D(index, x, y) {
  var rb, a
  if (mode) { rb = R[index]; a = A[index] }
  else { rb = polar(x, y); a = ph }
  var tx = rb * cos(a + RL[rb])
  var v = wave(tx * kz)
  hsv(tx * hz + RH[rb], 1, v * v)
}
