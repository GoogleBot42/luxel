// name: Spiral Galaxy
// tags: panel, nature
// Curated original (hand-written showcase of the Luxel language/builtins).
// A face-on spiral galaxy turning slowly: a yellow-white bulge, blue arms laced
// with dark dust lanes and pink star-forming knots, and a few hundred stars that
// orbit at their own speeds (inner ones faster) and flare blue as they cross an arm.
// The arms are a density wave on a 64 x 20 polar canvas built when a control moves;
// each panel cell reads it bilinearly through a packed (angle, radius) table
// with the frame's rotation added, and one paintCanvas paints it. Stars and knots
// are splats placed by hash, so they need no per-star state.

var NA = 64, NA1 = 65, NR = 20, CAP = 64
var RMAX = 1.45                   // canvas radius span, in disc radii
var P = 3600                      // star orbits repeat exactly over this many seconds
var NK = 10                       // star-forming knots

var arms = 2
var spin = 90                    // seconds per turn of the arm pattern
var pitch = 20                    // arm pitch angle, degrees (low = tightly wound)
var stars = 260
var dust = 60                     // dust-lane strength, percent

//# min=2 max=4 step=1 default=2
export function sliderArms(v) { arms = clamp(floor(v), 2, 4); fieldDirty = 1 }
//# min=30 max=600 step=10 default=90
export function sliderSpinSeconds(v) { spin = clamp(v, 30, 600) }
//# min=10 max=40 step=1 default=20
export function sliderPitchDegrees(v) { pitch = clamp(v, 10, 40); fieldDirty = 1 }
//# min=0 max=400 step=10 default=260
export function sliderStars(v) { stars = clamp(floor(v), 0, 400) }
//# min=0 max=100 step=1 default=60
export function sliderDustPercent(v) { dust = clamp(v, 0, 100); fieldDirty = 1 }

// indigo disc -> blue arms -> blue-white -> white -> warm yellow bulge
setPalette([0, 0, 0, 0, 0.12, 0.05, 0.02, 0.16, 0.35, 0.12, 0.25, 0.85,
  0.6, 0.55, 0.72, 1, 0.82, 1, 0.97, 0.9, 1, 1, 0.86, 0.5])

var pol = array(NA1 * NR)
var tab = array(CAP * CAP)        // per panel cell: angle*4096 + radius y, packed
var cv = array(CAP * CAP)

var CW = 0, CH = 0, M = 1, fieldDirty = 1
var T = 0, rot = 0, wind = 1

function buildTable() {
  for (var r = 0; r < CH; r++) {
    var dy = (r - CH * 0.5 + 0.5) / (M * 0.47)
    for (var c = 0; c < CW; c++) {
      var dx = (c - CW * 0.5 + 0.5) / (M * 0.47)
      var a = atan2(dy, dx) / PI2 + 0.5
      var q = sqrt(min(hypot(dx, dy), RMAX) / RMAX)
      tab[r * CW + c] = floor(min(a, 0.9998) * 4096) + (q * (NR - 1) + 0.5) / NR
    }
  }
}

// the arm field in the arms' own rotating frame (rows are sqrt-spaced radii)
function buildField() {
  wind = 1 / (PI2 * tan(pitch * PI / 180))
  var dl = dust * 0.01
  for (var j = 0; j < NR; j++) {
    var q = j / (NR - 1), r = q * q * RMAX
    var bulge = 1 / (1 + r * r * 160)
    var disc = saturate((1.08 - r) * 5) / (1 + r * 2.2)
    var lr = log(max(r, 0.02)) * wind
    for (var i = 0; i < NA; i++) {
      var t = i / NA, ph = arms * (t + lr)
      var arm = wave(ph)
      arm = arm * arm * arm
      var lane = wave(ph + 0.16)
      lane = lane * lane
      lane = lane * lane * saturate(r * 6 - 0.4)
      var clump = 0.65 + 0.5 * simplex2(cos(t * PI2) * r * 7, sin(t * PI2) * r * 7, 5)
      var v = disc * (0.13 + 0.9 * arm * clump) * (1 - dl * 0.95 * lane) + bulge
      pol[j * NA1 + i] = clamp(v, 0, 1)
    }
    pol[j * NA1 + NA] = pol[j * NA1]
  }
  fieldDirty = 0
}

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  var w = gridWidth() > 0 ? min(gridWidth(), CAP) : 32
  var h = gridHeight() > 0 ? min(gridHeight(), CAP) : 32
  if (w != CW || h != CH) { CW = w; CH = h; M = min(w, h); buildTable() }
  if (fieldDirty) buildField()
  T = mod(T + dt, P)
  rot = mod(rot + dt / spin, 1)
}

export function renderBulk() {
  // the arm pattern turns rigidly: shift every cell's angle by `rot`
  var n = CW * CH, K = NA / NA1 / 4096, ro = (1 - rot) * NA / NA1, XW = NA / NA1
  var x0 = 0.5 / NA1
  for (var i = 0; i < n; i++) {
    var p = tab[i]
    var x = floor(p) * K + ro
    if (x >= XW) x -= XW
    cv[i] = canvasGet(pol, NA1, x + x0, frac(p))
  }
  paintCanvas(cv, CW, CH)

  // stars on circular orbits, angular speed ~ 1/sqrt(r): k whole turns per P,
  // so the sky repeats seamlessly; a star brightens while it crosses an arm
  var sx = M * 0.47 / CW, sy = M * 0.47 / CH, sr = 1.5 / max(gridWidth(), CW)   // stars stay point-like
  for (var s = 0; s < stars; s++) {
    var u = hash(s * 1.37 + 0.5)
    var r = 0.1 + 0.95 * u * sqrt(u)
    var k = floor(P / spin * 0.75 / sqrt(r + 0.15)) + 1
    var a = hash(s * 2.11 + 7.3) + mod(k * T, P) / P
    var px = 0.5 + cos(a * PI2) * r * sx, py = 0.5 + sin(a * PI2) * r * sy
    var f = canvasGet(pol, NA1, (mod(a - rot - 0.5, 1) * NA + 0.5) / NA1, (sqrt(r / RMAX) * (NR - 1) + 0.5) / NR)
    var hc = hash(s * 3.7 + 1.1)
    var b = (0.15 + 0.85 * hc * hc) * (0.25 + f * 1.2) * (0.75 + 0.25 * wave(T * (1 + hc) + hc))
    hsv(hc < 0.8 ? 0.6 : 0.1, hc < 0.8 ? 0.3 * f + 0.05 : 0.35, min(b, 1))
    splat(px, py, sr, 1)
  }

  // pink star-forming knots ride the arms, glow for a while, then move on
  for (var j = 0; j < NK; j++) {
    var life = 9 + j % 4, g = floor((T + j * 3.1) / life), ph = mod(T + j * 3.1, life) / life
    var kr = 0.25 + 0.6 * hash2(j, g)
    var ka = (floor(hash2(g, j + 9) * arms) + 0.25) / arms - log(kr) * wind + rot
    hsv(0.92, 0.55, 0.55 * sin(ph * PI))
    splat(0.5 + cos((ka - 0.5) * PI2) * kr * sx, 0.5 + sin((ka - 0.5) * PI2) * kr * sy, 2.2 / CW, 1)
  }
}
