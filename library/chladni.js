// name: Chladni Plate
// Curated original (hand-written showcase of the Luxel language/builtins).
// Sand on a vibrating plate: it gathers on the nodal lines of a standing wave,
// cos(n pi x)cos(m pi y) -/+ cos(m pi x)cos(n pi y), and the figure melts into the
// next as the "frequency" steps through a list of modes. Field mode draws the
// lines from per-row/per-column cosine tables through a palette whose wrap does
// the |f| (one paintCanvas); grain mode lets sand diffuse in proportion to |f|
// on the plate, so the grains visibly migrate onto each new figure and settle.

var CAP = 64
var P = array(CAP * CAP)
var XA = array(CAP * 2), XB = array(CAP * 2), YA = array(CAP * 2), YB = array(CAP * 2)
var pal = array(28)
var MAXG = 1000
var gx = array(MAXG), gy = array(MAXG)
for (var k = 0; k < MAXG; k++) { gx[k] = random(1); gy[k] = random(1) }

// the mode list: n, m, sign (same parity, so every figure is mirror-symmetric)
var MODES = [1, 3, -1, 2, 4, 1, 1, 5, -1, 3, 5, 1, 2, 6, -1, 1, 3, 1, 4, 6, -1, 3, 7, 1, 1, 5, 1, 2, 4, -1, 5, 7, -1, 3, 5, -1]
var NM = 12

var mode = 0
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1) }
var holdS = 6                       // seconds each figure holds
//# min=2 max=30 step=1 default=6
export function sliderHoldS(v) { holdS = clamp(v, 2, 30) }
var palette = 0
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 2) }

// sand, plate, sheen per palette: hue, sat, value (brass | ice | ember)
var PC = [0.11, 0.35, 1, 0.07, 0.8, 0.16, 0.6, 0.6, 0.3,
          0.52, 0.25, 1, 0.62, 0.9, 0.14, 0.75, 0.7, 0.3,
          0.06, 0.8, 1, 0.98, 1, 0.1, 0.9, 1, 0.28]
var c3 = array(3), o = 0
function stop(p, j, sv) {
  hsv2rgb(PC[j], PC[j + 1], PC[j + 2] * sv, c3)
  pal[o] = p; pal[o + 1] = c3[0]; pal[o + 2] = c3[1]; pal[o + 3] = c3[2]; o += 4
}
function buildPalette(w, hum) {     // hum: the antinodes' sheen breathes as the plate rings
  var b = palette * 9
  o = 0
  stop(0, b, 1); stop(w * 0.45, b, 0.8); stop(w, b + 3, 1); stop(0.5, b + 6, hum)
  stop(1 - w, b + 3, 1); stop(1 - w * 0.45, b, 0.8); stop(1, b, 1)
  setPalette(pal)
}

var cur = 0, t = 0, clock = 0, CW = 32, CH = 32, K = 0.25
function tables(X, Y, j) {
  var n = MODES[j * 3] * PI, m = MODES[j * 3 + 1] * PI
  for (var c = 0; c < CW; c++) { var x = (c + 0.5) / CW; X[c] = cos(n * x); X[c + CAP] = cos(m * x) }
  for (var r = 0; r < CH; r++) { var y = (r + 0.5) / CH; Y[r] = cos(n * y); Y[r + CAP] = cos(m * y) }
}
function fAt(j, x, y) {
  var n = MODES[j * 3] * PI, m = MODES[j * 3 + 1] * PI
  return cos(n * x) * cos(m * y) - MODES[j * 3 + 2] * cos(m * x) * cos(n * y)
}

var dt = 0.016
export function beforeRender(delta) {
  dt = min(delta, 100) / 1000
  clock += dt
  if (clock > holdS + 1.5) { clock = 0; cur = (cur + 1) % NM }
  t = easeInOutSine(clamp((clock - holdS) / 1.5, 0, 1))
  // nodal-line half-width in palette units, ~1.3 px whatever the mode
  var gw = gridWidth() > 0 ? gridWidth() : 32
  var nm = MODES[cur * 3] + MODES[cur * 3 + 1]
  var w = clamp(1.3 * K * nm * 2 / gw, 0.02, 0.2)
  buildPalette(w, 0.7 + 0.3 * wave(time(0.02)))
}

function grains(gw, gh) {
  var nxt = (cur + 1) % NM, n = clamp(floor(gw * gh / 6), 150, MAXG)
  var amp = 0.5 * sqrt(dt)          // Brownian step at |f| = 1
  hsv(PC[palette * 9 + 3], PC[palette * 9 + 4], PC[palette * 9 + 5])
  fill()
  for (var k = 0; k < n; k++) {
    var x = gx[k], y = gy[k]
    var f = abs(fAt(cur, x, y) * (1 - t) + (t > 0 ? fAt(nxt, x, y) * t : 0)) + 0.01
    x += (random(2) - 1) * amp * f
    y += (random(2) - 1) * amp * f
    x = x < 0 ? -x : (x > 1 ? 2 - x : x)
    y = y < 0 ? -y : (y > 1 ? 2 - y : y)
    gx[k] = x; gy[k] = y
    paint(0.05 * hash(k), 1 - 0.35 * hash(k + 7))
    setPixel(gridIndex(x * gw, y * gh))
  }
}

export function renderBulk() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  if (mode == 1) { grains(gw, gh); return }
  CW = ceil(gw / ceil(gw / CAP)); CH = ceil(gh / ceil(gh / CAP))
  var nxt = (cur + 1) % NM, sA = MODES[cur * 3 + 2], sB = MODES[nxt * 3 + 2]
  tables(XA, YA, cur)
  var i = 0, r, c, a1, b1, a2, b2
  if (t > 0) {
    tables(XB, YB, nxt)
    for (r = 0; r < CH; r++) {
      a1 = K * (1 - t) * YA[r + CAP]; b1 = K * (1 - t) * sA * YA[r]
      a2 = K * t * YB[r + CAP]; b2 = K * t * sB * YB[r]
      for (c = 0; c < CW; c++) { P[i] = a1 * XA[c] - b1 * XA[c + CAP] + a2 * XB[c] - b2 * XB[c + CAP]; i++ }
    }
  } else {
    for (r = 0; r < CH; r++) {
      a1 = K * YA[r + CAP]; b1 = K * sA * YA[r]
      for (c = 0; c < CW; c++) { P[i] = a1 * XA[c] - b1 * XA[c + CAP]; i++ }
    }
  }
  paintCanvas(P, CW, CH, 1, gw > CW || gh > CH ? 1 : 0)   // bilinear only when upscaling
}
