// name: Chasing Rainbows & HSLuv
// Clean-room reimplementation from a prose functional description of the
// community pattern "Chasing Rainbows & HSLuv"; original source never consulted.

// One slowly scrolling rainbow across the strip, with a mode selector
// comparing six position→color mappings:
//   1 plain HSV hue wheel          2 half-sinusoid warped hue wheel
//   3 exponential "gain" warp      4 classic 9-stop table, lerped in HSV
//   5 same 9-stop table, in RGB    6 HSLuv (perceptually uniform)
// Modes 1-5 are dimmed to ~2/3 so the comparison with HSLuv is fair.
// Mode 6 renders from a per-pixel RGB lookup table covering one hue
// revolution, refreshed only ~10x per second (live conversion is far too
// slow per pixel); scrolling stays smooth because the phase is applied
// when indexing the table, not baked into it.

var mode = 6
var speed = 0.3
var shift = 0
var gainAmt = 2
var stretchHue = 0
var hsluvS = 1
var hsluvL = 0.5

var phase = 0
var cacheMs = 0
var cacheInit = 0

//# min=1 max=6 step=1 default=6
export function inputNumberRainbowMode(v) { mode = clamp(floor(v + 0.5), 1, 6) }
//# min=0 max=1 step=0.01 default=0.3
export function sliderSpeed(v) { speed = v }
//# min=0 max=1 step=0.01 default=0
export function sliderColorShift(v) { shift = v }
//# min=1 max=3 step=0.05 default=2
export function sliderGainAmount(v) { gainAmt = max(v, 0.05) }
//# min=0 max=1 step=0.01 default=0
export function sliderHueToStretch(v) { stretchHue = v }
//# min=0 max=1 step=0.01 default=1
export function sliderHsluvSaturation(v) { hsluvS = v }
//# min=0 max=1 step=0.01 default=0.5
export function sliderHsluvLightness(v) { hsluvL = v }

// ---- classic 9-stop rainbow (red orange yellow green aqua blue purple
// pink red), converted once to HSV at startup ----
var stopsR = array(9), stopsG = array(9), stopsB = array(9)
var stopsH = array(9), stopsS = array(9), stopsV = array(9)
function setStop(i, r, g, b) { stopsR[i] = r; stopsG[i] = g; stopsB[i] = b }
setStop(0, 1, 0, 0);   setStop(1, 1, 0.5, 0); setStop(2, 1, 1, 0)
setStop(3, 0, 1, 0);   setStop(4, 0, 1, 1);   setStop(5, 0, 0, 1)
setStop(6, 0.5, 0, 1); setStop(7, 1, 0, 0.5); setStop(8, 1, 0, 0)
var c3 = array(3)
var i
for (i = 0; i < 9; i++) {
  rgb2hsv(stopsR[i], stopsG[i], stopsB[i], c3)
  stopsH[i] = c3[0]; stopsS[i] = c3[1]; stopsV[i] = c3[2]
}

// ---- HSLuv: fixed-point port of the reference algorithm ----
// XYZ -> linear sRGB matrix
var Mm = array(9)
Mm[0] = 3.2409699;  Mm[1] = -1.5373832; Mm[2] = -0.4986108
Mm[3] = -0.9692436; Mm[4] = 1.8759675;  Mm[5] = 0.0415551
Mm[6] = 0.0556301;  Mm[7] = -0.2039770; Mm[8] = 1.0569715
var REF_U = 0.19783, REF_V = 0.46832
var KAPPA = 903.2963, EPS = 0.0088565

// Gamut boundary: six lines in (u,v) chroma space for a given lightness.
// Coefficients are the reference implementation's integer constants
// normalized by 126452 so every intermediate fits in 16.16 range.
var bSlope = array(6), bInt = array(6)
function computeBounds(L) {
  var sub1 = pow((L + 16) / 116, 3)
  var sub2 = sub1 > EPS ? sub1 : L / KAPPA
  var k = 0, c, t
  for (c = 0; c < 3; c++) {
    var m1 = Mm[c * 3], m2 = Mm[c * 3 + 1], m3 = Mm[c * 3 + 2]
    for (t = 0; t < 2; t++) {
      var bottom = (5 * m3 - m2) * sub2 + t
      if (abs(bottom) < 0.02) {
        // degenerate (line at infinity) — mark unused
        bSlope[k] = 0; bInt[k] = 0
      } else {
        bSlope[k] = (2.25 * m1 - 0.75 * m3) * sub2 / bottom
        bInt[k] = ((6.63036 * m3 + 6.08816 * m2 + 5.78653 * m1) * L * sub2
                   - 6.08816 * t * L) / bottom
      }
      k++
    }
  }
}

// Max in-gamut chroma along hue direction (cosT, sinT): nearest positive
// ray-line intersection over the six boundary lines
function maxChroma(sinT, cosT) {
  var best = 200, k
  for (k = 0; k < 6; k++) {
    var denom = sinT - bSlope[k] * cosT
    if (denom == 0) continue
    var len = bInt[k] / denom
    if (len > 0.01 && len < best) best = len
  }
  return best
}

function toSrgb(c) {
  c = clamp(c, 0, 1)
  if (c <= 0.0031308) return 12.92 * c
  return 1.055 * pow(c, 0.4166667) - 0.055
}

// ---- the cache: three pixel-count arrays holding one full HSLuv hue
// revolution, rebuilt ~10x/s from beforeRender ----
var rCache = array(pixelCount)
var gCache = array(pixelCount)
var bCache = array(pixelCount)

function rebuildCache() {
  var L = clamp(hsluvL, 0, 1) * 100
  var S = clamp(hsluvS, 0, 1)
  if (L < 0.1 || S <= 0) L = max(L, 0)   // chroma collapses; handled below
  computeBounds(L)
  var Y = L > 8 ? pow((L + 16) / 116, 3) : L / KAPPA
  var p
  for (p = 0; p < pixelCount; p++) {
    var theta = p / pixelCount * PI2
    var sinT = sin(theta), cosT = cos(theta)
    var C = L > 0.1 ? maxChroma(sinT, cosT) * S : 0
    // LCh -> LUV -> XYZ
    var varU = C * cosT / (13 * max(L, 0.1)) + REF_U
    var varV = C * sinT / (13 * max(L, 0.1)) + REF_V
    var X = 9 * Y * varU / (4 * varV)
    var Z = (9 * Y - 15 * varV * Y - varV * X) / (3 * varV)
    // XYZ -> linear sRGB -> gamma
    rCache[p] = toSrgb(Mm[0] * X + Mm[1] * Y + Mm[2] * Z)
    gCache[p] = toSrgb(Mm[3] * X + Mm[4] * Y + Mm[5] * Z)
    bCache[p] = toSrgb(Mm[6] * X + Mm[7] * Y + Mm[8] * Z)
  }
}

// symmetric two-halved power-curve easing ("gain"): identity at k=1,
// stretches one side / compresses the complement as k rises
function gainWarp(x, k) {
  if (x < 0.5) return 0.5 * pow(2 * x, k)
  return 1 - 0.5 * pow(2 * (1 - x), k)
}

export function beforeRender(delta) {
  // scroll phase: rate proportional to the speed slider; 0 freezes
  phase = frac(phase + delta * speed * 0.0002)
  // refresh the HSLuv lookup table about ten times per second
  cacheMs += delta
  if (!cacheInit || cacheMs >= 100) {
    cacheMs = 0
    cacheInit = 1
    rebuildCache()
  }
}

export function render(index) {
  // base hue fraction: position minus animation phase minus user shift
  var f = frac(index / pixelCount - phase - shift + 2)

  if (mode == 1) {
    hsv(f, 1, 0.65)
  } else if (mode == 2) {
    // sine-based smoothstep: stretches red/pink ends, compresses cyan
    hsv(0.5 - cos(f * PI) / 2, 1, 0.65)
  } else if (mode == 3) {
    // gain warp around a slider-chosen hue
    var w = gainWarp(frac(f + stretchHue), gainAmt)
    hsv(frac(w - stretchHue + 1), 1, 0.65)
  } else if (mode == 4 || mode == 5) {
    var seg = f * 8
    var si = floor(seg)
    if (si > 7) si = 7
    var t = seg - si
    if (mode == 4) {
      var h2 = stopsH[si + 1]
      if (h2 < stopsH[si]) h2 += 1
      hsv(mix(stopsH[si], h2, t), mix(stopsS[si], stopsS[si + 1], t),
          mix(stopsV[si], stopsV[si + 1], t) * 0.65)
    } else {
      rgb(mix(stopsR[si], stopsR[si + 1], t) * 0.65,
          mix(stopsG[si], stopsG[si + 1], t) * 0.65,
          mix(stopsB[si], stopsB[si + 1], t) * 0.65)
    }
  } else {
    var ti = floor(f * pixelCount)
    if (ti >= pixelCount) ti = pixelCount - 1
    rgb(rCache[ti], gCache[ti], bCache[ti])
  }
}
