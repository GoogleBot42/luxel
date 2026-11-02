// name: Julia Morph
// Clean-room reimplementation from the prose spec docs/pattern-specs/mandelbrot-2d.md
// — reimagined, not a replica.
// An animated Julia set whose constant c rides just around the main cardioid's
// edge of the Mandelbrot set, so every frame is a rich connected fractal that
// morphs from cauliflower to spirals to dendrites. Escape counts are smoothed
// (log LUT) and painted through a deep cyclic palette, brightest at the
// filaments; the interior keeps a dim texture. Iterated once per frame on a
// small canvas (<= 64 wide) and put on the frame with paintCanvas, bilinear
// whenever the canvas is smaller than the grid.

var iters = 20
//# min=8 max=32 step=1 default=20
export function sliderIterations(v) { iters = clamp(floor(v), 8, 32) }

// Seconds for c to travel once round the cardioid.
var lapS = 90
//# min=20 max=300 step=5 default=90
export function sliderMorphSeconds(v) { lapS = clamp(v, 20, 300) }

var MAXC = 4096
var cv = array(MAXC), cb = array(MAXC)
var CW = 32, CH = 32, passes = 0, nf = 2

// smooth-count correction log2(log2(m2) / 2) for |z|^2 = 16 .. 300, step 4
var LUT = array(72)
for (var k = 0; k < 72; k++) LUT[k] = log2(log2(16 + k * 4) / 2)

setPalette([0, 0.02, 0.02, 0.12,  0.16, 0.12, 0.03, 0.42,  0.34, 0.62, 0.05, 0.55,
  0.5, 1, 0.32, 0.08,  0.64, 1, 0.78, 0.22,  0.76, 1, 0.97, 0.86,  0.88, 0.1, 0.55, 0.8,
  1, 0.02, 0.02, 0.12])

var th = 0.6, zp = 0, ph = 0, ang = 0, fld = 0
var cr = 0, ci = 0, scl = 1.5

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  th = mod(th + PI2 * dt / lapS, PI2)
  zp = mod(zp + dt / 37, 1)
  ph = mod(ph + dt / 23, 1)
  ang = mod(ang + dt * 0.03, PI2)
  // multiplier mu = r e^(i th) on (just around) the unit circle; c = mu/2 - mu^2/4
  var r = 1.0 + 0.05 * sin(th * 3 + 1)
  cr = r * cos(th) / 2 - r * r * cos(2 * th) / 4
  ci = r * sin(th) / 2 - r * r * sin(2 * th) / 4
  scl = 1.4 - 0.5 * easeInOutSine(triangle(zp))

  var gw = gridWidth(), gh = gridHeight()
  if (gw < 1) { gw = 32; gh = 32 }
  CW = gw
  CH = gh
  passes = 0
  while (CW * CH > MAXC || CW > 64) {
    CW = ceil(CW / 2)
    CH = ceil(CH / 2)
    passes++
  }
  // interlace: each frame iterates every nf-th row, ~600 cells
  nf = clamp(ceil(CW * CH / 600), 1, 8)
}

export function renderBulk() {
  var asp = CW / CH
  var ca = cos(ang) * scl, sa = sin(ang) * scl
  var du = 2 * asp / CW, dv = 2 / CH
  var lim = iters
  fld = (fld + 1) % nf
  for (var row = fld; row < CH; row += nf) {
    var j = row * CW
    var v = (row + 0.5) * dv - 1
    var u = 0.5 * du - asp
    var zr0 = u * ca - v * sa, zi0 = u * sa + v * ca
    var sr = du * ca, si = du * sa
    for (var col = 0; col < CW; col++) {
      var zr = zr0, zi = zi0, i = 0, m2 = 0, trap = 16
      while (i < lim) {
        var r2 = zr * zr, i2 = zi * zi
        m2 = r2 + i2
        if (m2 > 16) break
        if (i > 0) trap = min(trap, m2)
        zi = 2 * zr * zi + ci
        zr = r2 - i2 + cr
        i++
      }
      if (i < lim) {
        var q = min((m2 - 16) / 4, 70), qk = floor(q)
        var nu = i + 1 - LUT[qk] - (LUT[qk + 1] - LUT[qk]) * (q - qk)
        cv[j] = mod(nu * 0.1 + ph, 1)
        cb[j] = 0.12 + 0.88 * smoothstep(1, 10, nu)
      } else {
        // interior: an orbit trap at 0 shows the Fatou bubbles
        var tr = sqrt(trap)
        cv[j] = mod(tr * 1.2 + ph + 0.4, 1)
        cb[j] = 0.05 + 0.45 * (1 - smoothstep(0, 0.8, tr))
      }
      zr0 += sr
      zi0 += si
      j++
    }
  }
  paintCanvas(cv, CW, CH, cb, passes > 0 ? 1 : 0)   // 1:1 stays nearest
}
