// name: Loop Ring
// tags: panel, ambient
// Curated original (hand-written showcase of the Luxel language/builtins).
// Concentric luminous rings whose edges boil with noise — and the whole thing is a
// perfect loop: each ring's noise is read on a circle in space AND a circle in time,
// so after Loop seconds every ring is exactly where it began. The rings breathe out
// of phase like an iris. Noise is only sampled per angle bucket (48 per ring); the
// rings are deposited on a small polar canvas, read per panel cell through a packed
// (angle, radius) table and painted through a palette with one paintCanvas.

var CAP = 64, NA = 48, NA1 = NA + 1, NR = 36, RMAX = 1.45, MAXK = 6

var rings = 4
//# min=1 max=6 step=1 default=4
export function sliderRings(v) { rings = clamp(floor(v), 1, MAXK) }

// seconds before the animation repeats exactly
var loopSec = 10
//# min=4 max=30 step=1 default=10
export function sliderLoopSeconds(v) { loopSec = clamp(v, 4, 30) }

// how far the edge wanders, percent of the ring spacing
var wobble = 60
//# min=0 max=100 step=1 default=60
export function sliderWobblePercent(v) { wobble = clamp(v, 0, 100) }

// noise lobes around each ring
var detail = 2
//# min=1 max=6 step=0.5 default=2
export function sliderDetail(v) { detail = clamp(v, 1, 6) }

// ring core thickness in panel pixels
var widthPx = 1.5
//# min=0.5 max=4 step=0.1 default=1.5
export function sliderWidthPixels(v) { widthPx = clamp(v, 0.5, 4) }

var hueDeg = 190
//# min=0 max=360 step=1 default=190
export function sliderHueDegrees(v) { hueDeg = clamp(v, 0, 360) }

// seconds for the palette to travel round the colour wheel; 0 holds it
var driftSec = 60
//# min=0 max=240 step=1 default=60
export function sliderHueDriftSeconds(v) { driftSec = clamp(v, 0, 240) }

var tab = array(CAP * CAP), cv = array(CAP * CAP), pol = array(NA1 * NR)
var cs = array(NA), sn = array(NA)
var pal = array(20), c3 = array(3)
setPalette(pal)
var CW = 0, CH = 0, ph = 0, drift = 0, half = 32

for (var a = 0; a < NA; a++) { cs[a] = cos(a * PI2 / NA); sn[a] = sin(a * PI2 / NA) }

function buildTable() {
  half = min(CW, CH) * 0.5
  for (var r = 0; r < CH; r++) {
    var dy = (r - CH * 0.5 + 0.5) / half
    for (var c = 0; c < CW; c++) {
      var dx = (c - CW * 0.5 + 0.5) / half
      var u = mod(atan2(dy, dx) * 0.15915494, 1), q = min(hypot(dx, dy) / RMAX, 1)
      tab[r * CW + c] = floor((u * NA + 0.5) / NA1 * 4096) + (q * (NR - 1) + 0.5) / NR
    }
  }
}

function stop(j, pos, h, s, v) {
  hsv2rgb(h, s, v, c3)
  pal[j] = pos; pal[j + 1] = c3[0]; pal[j + 2] = c3[1]; pal[j + 3] = c3[2]
}

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  var gw = gridWidth(), gh = gridHeight()
  var w = gw > 0 ? min(gw, CAP) : 32, h = gh > 0 ? min(gh, CAP) : 32
  if (w != CW || h != CH) { CW = w; CH = h; buildTable() }
  ph = mod(ph + dt / loopSec, 1)
  if (driftSec > 0) drift = mod(drift + dt / driftSec, 1)

  var hh = hueDeg / 360 + drift
  stop(0, 0, hh, 1, 0)
  stop(4, 0.3, hh, 1, 0.35)
  stop(8, 0.6, hh + 0.12, 0.9, 0.8)
  stop(12, 0.85, hh + 0.2, 0.45, 1)
  stop(16, 1, hh + 0.2, 0.1, 1)

  // deposit every ring on the polar canvas
  feedback(pol, 0)
  var fi = ph * PI2, lx = sin(fi) * 0.9, lz = cos(fi) * 0.9
  var gap = 1.3 / (rings + 0.4), amp = gap * wobble * 0.006, nd = detail * 0.45
  var rowR = RMAX / (NR - 1), wr = widthPx / half / rowR    // core width in rows
  for (var k = 0; k < rings; k++) {
    var r0 = (0.6 + k) * gap * (1 + 0.08 * sin(fi + k * 2.1)), seed = k * 5 + 3
    var lift = 1 - k * 0.1
    for (a = 0; a < NA; a++) {
      var n = simplex3(cs[a] * nd + lx + k * 3.7, sn[a] * nd, lz, seed)
      var rr = max(0.02, r0 + amp * n) / rowR                 // ring centre, in rows
      var j0 = max(0, floor(rr - wr * 3)), j1 = min(NR - 1, ceil(rr + wr * 3))
      for (var j = j0; j <= j1; j++) {
        var d = (j - rr) / wr, i = j * NA1 + a
        pol[i] = min(1, pol[i] + lift / (1 + d * d))
      }
    }
  }
  for (j = 0; j < NR; j++) pol[j * NA1 + NA] = pol[j * NA1]
}

export function renderBulk() {
  var n = CW * CH
  for (var i = 0; i < n; i++) {
    var p = tab[i], ai = floor(p)
    cv[i] = canvasGet(pol, NA1, ai * 0.000244140625, p - ai)
  }
  paintCanvas(cv, CW, CH, 1, gridWidth() > CW || gridHeight() > CH ? 1 : 0)   // bilinear only when upscaling
}
