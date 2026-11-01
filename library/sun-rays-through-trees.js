// name: God Rays
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/sun-rays-through-trees.md — reimagined, not a replica.
// Low sun just above the top edge, glaring through a band of silhouetted leaves:
// warm shafts fan across the whole panel, swaying and flickering as the canopy
// drifts, over a soft golden haze, with dust motes glinting as they float through.
// Per frame the canopy is 64 angular bins of two-octave simplex; a 64 x 16 polar
// canvas (angle x sqrt-distance) multiplies it by the falloff and adds haze and
// sun glow; each panel cell reads it bilinearly through a packed table, and one
// paintCanvas through a warm amber-to-white ramp paints it. Motes are splats.

var NA = 64, NR = 16, CAP = 64
var FAN = 1.45                    // half-angle of the fan, radians from straight down
var SUNY = -0.1                   // sun height, panel heights (above the top edge)
var DMAX = 1.5                    // farthest distance the canvas covers
var CANOPY = 0.32                 // depth of the leafy band below the sun
var NM = 18                       // dust motes

var shafts = 12                   // light shafts across the fan
var drift = 14                    // seconds for the canopy to drift one shaft
var haze = 25                     // ambient scattered light, percent

//# min=4 max=32 step=1 default=12
export function sliderShafts(v) { shafts = clamp(floor(v), 4, 32) }
//# min=2 max=60 step=1 default=14
export function sliderDriftSeconds(v) { drift = clamp(v, 2, 60) }
//# min=0 max=60 step=1 default=25
export function sliderHazePercent(v) { haze = clamp(v, 0, 60) }

setPalette([0, 0, 0, 0, 0.12, 0.13, 0.04, 0.01, 0.35, 0.55, 0.22, 0.03,
  0.6, 0.95, 0.6, 0.16, 0.85, 1, 0.87, 0.58, 1, 1, 0.97, 0.9])

var S = array(NA)                 // canopy openness per angular bin
var pol = array(NA * NR)          // polar light canvas, 0..1
var tab = array(CAP * CAP)        // per panel cell: angle x*4096 + distance y, packed
var cv = array(CAP * CAP)
var mx = array(NM), my = array(NM), mph = array(NM)
for (var m = 0; m < NM; m++) { mx[m] = random(1); my[m] = random(1); mph[m] = random(PI2) }

var CW = 0, CH = 0, M = 1
var T = 0, T2 = 0, slide = 0

function buildTable() {
  var sxp = CW * 0.5 - 0.5, syp = SUNY * CH - 0.5
  for (var r = 0; r < CH; r++) {
    var dy = (r - syp) / M
    for (var c = 0; c < CW; c++) {
      var dx = (c - sxp) / M
      var a = clamp((atan2(dx, dy) + FAN) / (2 * FAN), 0, 1)
      var d = sqrt(min(hypot(dx, dy), DMAX) / DMAX)
      tab[r * CW + c] = floor((a * (NA - 1) + 0.5) / NA * 4096) + (d * (NR - 1) + 0.5) / NR
    }
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  var w = gridWidth() > 0 ? min(gridWidth(), CAP) : 32
  var h = gridHeight() > 0 ? min(gridHeight(), CAP) : 32
  if (w != CW || h != CH) { CW = w; CH = h; M = min(w, h); buildTable() }

  slide = mod(slide + dt / drift, 4096)   // the canopy slides sideways...
  T = mod(T + dt * 0.11, 4096)            // ...while its gaps open and close
  T2 = mod(T2 + dt * 0.37, 4096)          // and the leaves flutter

  var k = shafts * 0.62
  for (var i = 0; i < NA; i++) {
    var u = i / (NA - 1) * k + slide * 0.62
    var n = simplex2(u, T) + 0.45 * simplex2(u * 2.7, T2, 3)
    S[i] = smoothstep(-0.05, 0.55, n)
  }

  var hz = haze * 0.01
  for (var j = 0; j < NR; j++) {
    var q = j / (NR - 1), d = q * q * DMAX
    var fall = 1 / (1 + d * 1.3)
    var glow = 1 / (1 + d * d * 90)
    var amb = hz * (0.2 + 0.5 * fall)
    var row = j * NA
    // the canopy itself near the top: leaves silhouetted against the glare,
    // densest where the shafts below are blocked
    var leafy = d < CANOPY ? saturate((CANOPY - d) * 8) : 0
    for (i = 0; i < NA; i++) {
      var v = S[i] * fall * 0.8 + amb + glow
      if (leafy > 0) {
        var lf = saturate((1 - S[i]) * 1.3 + 0.6 * simplex3(i * 0.45 + slide * 6, d * 14, T2 * 0.5) - 0.2)
        v = v * (1 - lf * leafy * 0.9)
      }
      pol[row + i] = min(1, v)
    }
  }

  for (m = 0; m < NM; m++) {                // motes sink slowly and sway
    my[m] += dt * 0.03
    mx[m] += sin(mph[m] + T2 * 3) * dt * 0.01
    if (my[m] > 1.02) { my[m] = -0.02; mx[m] = random(1) }
  }
}

export function renderBulk() {
  var n = CW * CH
  for (var i = 0; i < n; i++) {
    var p = tab[i]
    cv[i] = canvasGet(pol, NA, floor(p) * 0.000244140625, frac(p))
  }
  paintCanvas(cv, CW, CH)

  // a mote only glints where a shaft actually reaches it
  var r = 0.9 / CW
  for (var m = 0; m < NM; m++) {
    var dx = (mx[m] - 0.5) * CW / M, dy = (my[m] - SUNY) * CH / M
    var a = clamp((atan2(dx, dy) + FAN) / (2 * FAN), 0, 1)
    var b = S[floor(a * (NA - 1))] * (0.4 + 0.6 * wave(mph[m] + T2))
    if (b < 0.1) continue
    rgb(b, b * 0.85, b * 0.6)
    splat(mx[m], my[m], r * 1.6, 1)
  }
}
