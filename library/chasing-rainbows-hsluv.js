// name: Chasing Rainbows & HSLuv
// Clean-room reimplementation from a prose functional description of the
// community pattern "Chasing Rainbows & HSLuv"; original source never consulted.

// One slowly scrolling rainbow across the strip; a mode number switches
// between six position→color mappings so you can compare perceptual
// evenness on real LEDs:
//   1 plain HSV hue wheel        2 half-sinusoid warp
//   3 exponential "gain" warp    4 classic 9-stop table lerped in HSV
//   5 same 9 stops lerped in RGB 6 HSLuv (perceptually uniform)
// Modes 1-5 are dimmed to ~2/3 so the comparison with HSLuv is fair.
// The HSLuv conversion is far too slow to run per pixel per frame, so
// mode 6 renders from three per-pixel RGB cache arrays covering one full
// hue revolution, refreshed only ~10 times per second; scrolling stays
// smooth because the phase is applied in the table *index*, not the table.

var mode = 1
//# min=1 max=6 step=1 default=1
export function inputNumberRainbowMode(v) { mode = clamp(floor(v), 1, 6) }

var speedV = 0.3
//# min=0 max=1 step=0.01 default=0.3
export function sliderSpeed(v) { speedV = v }        // bottom freezes

var shiftV = 0
//# min=0 max=1 step=0.01 default=0
export function sliderColorShift(v) { shiftV = v }   // static hue offset

var gainE = 2
//# min=0 max=1 step=0.01 default=0.5
export function sliderGainAmount(v) { gainE = 1 + v * 2 }   // exponent 1..3

var stretchOff = 0
//# min=0 max=1 step=0.01 default=0
export function sliderHueToStretch(v) { stretchOff = v }

var luvSat = 1
//# min=0 max=1 step=0.01 default=1
export function sliderHSLuvSaturation(v) { luvSat = v; dirty = 1 }

var luvLight = 55
//# min=0 max=1 step=0.01 default=0.55
export function sliderHSLuvLightness(v) { luvLight = v * 100; dirty = 1 }

var phase = 0
var acc = 1000     // delta accumulator for cache refresh (start due)
var dirty = 0

export function beforeRender(delta) {
  phase = (phase + delta * speedV * speedV / 3000) % 1
  acc += delta
  if (mode == 6 && (acc > 100 || dirty)) {
    buildTable()
    acc = 0
    dirty = 0
  }
}

export function render(index) {
  var f = mod(index / pixelCount - phase - shiftV, 1)
  if (mode == 1) {
    hsv(f, 1, 0.65)
  } else if (mode == 2) {
    // sine-based smoothstep: stretches red/orange and the pink end,
    // compresses the over-wide cyan/green midsection
    hsv((1 - cos(f * PI)) / 2, 1, 0.65)
  } else if (mode == 3) {
    var g = gainWarp(mod(f + stretchOff, 1))
    hsv(mod(g - stretchOff, 1), 1, 0.65)
  } else if (mode == 4) {
    stopLerpHSV(f)
  } else if (mode == 5) {
    stopLerpRGB(f)
  } else {
    var i = min(floor(f * pixelCount), pixelCount - 1)
    rgb(rT[i], gT[i], bT[i])
  }
}

// symmetric two-halved power-curve easing; identity at exponent 1
function gainWarp(x) {
  if (x < 0.5) return 0.5 * pow(2 * x, gainE)
  return 1 - 0.5 * pow(2 * (1 - x), gainE)
}

// ---- classic 9-stop rainbow table (red..pink..red) -------------------------

var sR = array(9)
var sG = array(9)
var sB = array(9)
function setStop(i, r, g, b) { sR[i] = r; sG[i] = g; sB[i] = b }
setStop(0, 1, 0, 0)      // red
setStop(1, 1, 0.5, 0)    // orange
setStop(2, 1, 1, 0)      // yellow
setStop(3, 0, 1, 0)      // green
setStop(4, 0, 1, 1)      // aqua
setStop(5, 0, 0, 1)      // blue
setStop(6, 0.5, 0, 1)    // purple
setStop(7, 1, 0, 0.5)    // pink
setStop(8, 1, 0, 0)      // back to red

// converted once at startup to HSV
var sH = array(9)
var sS = array(9)
var sV = array(9)
var cvt = array(3)
var si = 0
for (si = 0; si < 9; si++) {
  rgb2hsv(sR[si], sG[si], sB[si], cvt)
  sH[si] = cvt[0]
  sS[si] = cvt[1]
  sV[si] = cvt[2]
}
sH[8] = 1   // wrap: final red sits a full turn up so segment 8 lerps forward

function stopLerpHSV(f) {
  var seg = f * 8
  var i = min(floor(seg), 7)
  var t = seg - i
  var h1 = sH[i]
  var h2 = sH[i + 1]
  if (h2 < h1) h2 += 1
  hsv(mix(h1, h2, t), mix(sS[i], sS[i + 1], t), mix(sV[i], sV[i + 1], t) * 0.65)
}

function stopLerpRGB(f) {
  var seg = f * 8
  var i = min(floor(seg), 7)
  var t = seg - i
  rgb(mix(sR[i], sR[i + 1], t) * 0.65,
      mix(sG[i], sG[i + 1], t) * 0.65,
      mix(sB[i], sB[i + 1], t) * 0.65)
}

// ---- HSLuv: fixed-point port of the reference implementation ---------------
// Constants from the published reference are scaled by 1e-5 where the
// originals (284517, 838422, ...) would overflow 16.16 range; the scale
// cancels in every slope/intercept quotient.

// XYZ -> linear sRGB matrix, row-major (R, G, B rows)
var M = array(9)
M[0] = 3.2409699; M[1] = -1.5373832; M[2] = -0.4986108
M[3] = -0.9692436; M[4] = 1.8759675; M[5] = 0.0415551
M[6] = 0.0556301; M[7] = -0.2039770; M[8] = 1.0569715

var refU = 0.19783
var refV = 0.46832

// max in-gamut chroma for a given lightness and hue angle: intersect the
// hue ray with the six RGB gamut-boundary lines, keep the nearest
function maxChromaForLH(L, hrad) {
  var s1 = (L + 16) / 116
  var sub1 = s1 * s1 * s1
  var sub2 = sub1 > 0.0088564 ? sub1 : L / 903.2963
  var sh = sin(hrad)
  var chh = cos(hrad)
  var best = 1000
  var c = 0
  for (c = 0; c < 3; c++) {
    var m0 = M[c * 3]
    var m1 = M[c * 3 + 1]
    var m2 = M[c * 3 + 2]
    var top1 = (2.84517 * m0 - 0.94839 * m2) * sub2
    var t2base = (8.38422 * m2 + 7.69860 * m1 + 7.31718 * m0) * L * sub2
    var botBase = (6.32260 * m2 - 1.26452 * m1) * sub2
    var t = 0
    for (t = 0; t < 2; t++) {
      var bottom = botBase + 1.26452 * t
      if (bottom == 0) continue
      var slope = top1 / bottom
      var intercept = (t2base - 7.69860 * t * L) / bottom
      var denom = sh - slope * chh
      if (denom == 0) continue
      var len = intercept / denom
      if (len >= 0 && len < best) best = len
    }
  }
  return best
}

// h in turns 0..1, s in 0..1, L in 0..100; writes tr/tg/tb
var tr = 0
var tg = 0
var tb = 0
function hsluv2rgbT(h, s, L) {
  if (L < 0.1) { tr = 0; tg = 0; tb = 0; return 0 }
  if (L > 99.9) { tr = 1; tg = 1; tb = 1; return 0 }
  var hrad = h * PI2
  var C = maxChromaForLH(L, hrad) * s
  // LCH -> LUV
  var u = C * cos(hrad)
  var v = C * sin(hrad)
  // LUV -> XYZ
  var varU = u / (13 * L) + refU
  var varV = v / (13 * L) + refV
  var y = 0
  if (L > 8) {
    var yy = (L + 16) / 116
    y = yy * yy * yy
  } else {
    y = L / 903.2963
  }
  var x = 2.25 * y * varU / varV
  var z = y * (12 - 3 * varU - 20 * varV) / (4 * varV)
  // XYZ -> linear sRGB -> gamma
  tr = toSRGB(M[0] * x + M[1] * y + M[2] * z)
  tg = toSRGB(M[3] * x + M[4] * y + M[5] * z)
  tb = toSRGB(M[6] * x + M[7] * y + M[8] * z)
}

function toSRGB(c) {
  if (c <= 0.0031308) return clamp(12.92 * c, 0, 1)
  return clamp(1.055 * pow(c, 0.4166667) - 0.055, 0, 1)
}

// per-pixel RGB cache: one full hue revolution across the strip
var rT = array(pixelCount)
var gT = array(pixelCount)
var bT = array(pixelCount)

function buildTable() {
  var i = 0
  for (i = 0; i < pixelCount; i++) {
    hsluv2rgbT(i / pixelCount, luvSat, luvLight)
    rT[i] = tr
    gT[i] = tg
    bT[i] = tb
  }
}

buildTable()   // also computed once at startup
