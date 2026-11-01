// name: Color Sequence
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/custom-sequences.md,
// christmas-lights.md, christmaslights.md, christmasstretch.md, 2-colors.md,
// 3-color-rotation.md — reimagined, not a replica.
// Eave and holiday colour runs: a repeating sequence of soft-edged bulbs (Run px lit,
// Gap px dark) crawling along the strip, and every Hold seconds each bulb steps,
// crossfades (in OKLab) or inchworm-slides on to the next colour. Presets cover the
// classic strings; Custom takes the four pickers. renderBulk, no per-pixel arrays.

var MAXC = 8
var SR = array(MAXC), SG = array(MAXC), SB = array(MAXC)   // the sequence
var CR = array(MAXC), CG = array(MAXC), CB = array(MAXC)   // this frame's colours
var pk = array(12), tmp = array(3)
pk[0] = 1; pk[1] = 0; pk[2] = 0             // custom defaults: red, gold, green, blue
pk[3] = 1; pk[4] = 0.55; pk[5] = 0
pk[6] = 0; pk[7] = 0.9; pk[8] = 0.08
pk[9] = 0.02; pk[10] = 0.2; pk[11] = 1
var seqN = 4

function sc(i, r, g, b) { SR[i] = r; SG[i] = g; SB[i] = b }
var preset = 0, nColors = 4
function build() {
  if (preset == 0) {             // custom: pickers 1-4, then orange, violet, teal, warm white
    for (var i = 0; i < 4; i++) sc(i, pk[i * 3], pk[i * 3 + 1], pk[i * 3 + 2])
    sc(4, 1, 0.3, 0); sc(5, 0.55, 0, 1); sc(6, 0, 0.75, 0.6); sc(7, 1, 0.72, 0.42)
    seqN = nColors
  } else if (preset == 1) { sc(0, 1, 0, 0); sc(1, 0, 0.9, 0.05); seqN = 2 }
  else if (preset == 2) { sc(0, 1, 0, 0); sc(1, 1, 0.72, 0.42); sc(2, 0, 0.9, 0.05); seqN = 3 }
  else if (preset == 3) { sc(0, 1, 0, 0); sc(1, 0, 0.9, 0.05); sc(2, 0.02, 0.2, 1); sc(3, 1, 0.7, 0); seqN = 4 }
  else if (preset == 4) { sc(0, 1, 0.28, 0); sc(1, 0.5, 0, 0.9); sc(2, 0.45, 1, 0); seqN = 3 }
  else { sc(0, 1, 0, 0.02); sc(1, 1, 0.85, 0.75); sc(2, 0, 0.12, 1); seqN = 3 }
}
build()

// Preset: 0 Custom (pickers) | 1 red/green | 2 red/white/green | 3 RGBY classic |
// 4 Halloween | 5 patriotic
//# min=0 max=5 step=1 default=0
export function sliderPreset(v) { preset = clamp(floor(v), 0, 5); build() }

// How many colours the Custom preset runs (5-8 are orange, violet, teal, warm white).
//# min=1 max=8 step=1 default=4
export function sliderColors(v) { nColors = clamp(floor(v), 1, 8); build() }

function pick(i, h, s) { hsv2rgb(h, s, 1, tmp); pk[i * 3] = tmp[0]; pk[i * 3 + 1] = tmp[1]; pk[i * 3 + 2] = tmp[2]; build() }
// Custom colours. The pickers' brightness is ignored.
export function hsvPickerColor1(h, s, v) { pick(0, h, s) }
export function hsvPickerColor2(h, s, v) { pick(1, h, s) }
export function hsvPickerColor3(h, s, v) { pick(2, h, s) }
export function hsvPickerColor4(h, s, v) { pick(3, h, s) }

var runPx = 5
//# min=1 max=60 step=1 default=5
export function sliderRunPixels(v) { runPx = clamp(floor(v), 1, 60) }

var gapPx = 1
//# min=0 max=10 step=1 default=1
export function sliderGapPixels(v) { gapPx = clamp(floor(v), 0, 10) }

var trans = 2            // 0 none (chase only) | 1 step | 2 crossfade | 3 stretch-slide
//# min=0 max=3 step=1 default=2
export function sliderTransition(v) { trans = clamp(floor(v), 0, 3) }

var holdS = 2
//# min=0.2 max=30 step=0.1 default=2
export function sliderHoldSeconds(v) { holdS = clamp(v, 0.2, 30) }

var chase = 3            // px/s, signed; also sets the direction colours step in
//# min=-60 max=60 step=0.5 default=3
export function sliderChasePxPerSecond(v) { chase = clamp(v, -60, 60) }

var soft = 0.4
//# min=0 max=100 step=1 default=40
export function sliderBulbSoftnessPercent(v) { soft = clamp(v, 0, 100) / 100 }

var blinkS = 0           // alternate bulbs trade places every half period; 0 = off
//# min=0 max=10 step=0.1 default=0
export function sliderBlinkSeconds(v) { blinkS = clamp(v, 0, 10) }

var offset = 0, holdT = 0, rot = 0, u = 0, dirS = 1, blinkT = 0, evenK = 1, oddK = 1
export function beforeRender(delta) {
  var P = runPx + gapPx
  offset = mod(offset + chase * delta / 1000, P * seqN * 2)   // x2 keeps the blink parity whole
  dirS = chase < 0 ? -1 : 1
  if (trans > 0) {
    holdT += delta / 1000
    if (holdT >= holdS) { holdT = holdT >= holdS * 2 ? 0 : holdT - holdS; rot = mod(rot + dirS, seqN) }
  }
  u = holdT / holdS
  var f = trans == 2 ? smoothstep(0.4, 1, u) : 0
  for (var j = 0; j < seqN; j++) {
    var a = mod(j - rot, seqN), b = mod(j - rot - dirS, seqN)
    if (f > 0) {
      mixColors(SR[a], SG[a], SB[a], SR[b], SG[b], SB[b], f, tmp)
      CR[j] = tmp[0]; CG[j] = tmp[1]; CB[j] = tmp[2]
    } else { CR[j] = SR[a]; CG[j] = SG[a]; CB[j] = SB[a] }
  }
  evenK = 1; oddK = 1
  if (blinkS > 0) {
    blinkT = mod(blinkT + delta / 1000, blinkS)
    var ph = blinkT / blinkS, e = min(0.12 / blinkS, 0.2)      // ~120 ms filament fade
    evenK = smoothstep(0, e, ph) * (1 - smoothstep(0.5, 0.5 + e, ph))
    oddK = 1 - smoothstep(0, e, ph) * (1 - smoothstep(0.5, 0.5 + e, ph))
  }
}

// A soft bulb on [a, b): a full core, then a linear falloff over soft/2 of its length.
function bulb(a, b) {
  var r = (b - a) * soft * 0.5
  if (r < 0.5) { fillRangeAA(a, b, 2); return }
  fillRangeAA(a + r, b - r, 2)
  splatIndex(a + r - 0.5, r, 2)
  splatIndex(b - r - 0.5, r, 2)
}

// A mapless strip stays a strip; Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

export function renderBulk() {
  clear()
  var P = runPx + gapPx, ks = floor(offset / P), o = offset - ks * P
  var lead = 0, trail = 0
  if (trans == 3) {               // glide one bulb on: the head leaves first, stretching over the gap
    trail = smoothstep(0.55, 1, u) * P
    lead = min(smoothstep(0.4, 0.85, u) * P, trail + gapPx)
  }
  var n = ceil(pixelCount / P) + 2
  for (var j = -2; j < n; j++) {
    var c = mod(j - ks, seqN), m = mod(j - ks, 2) ? oddK : evenK
    if (m <= 0) continue
    rgb(CR[c] * m, CG[c] * m, CB[c] * m)
    var a = j * P + o
    if (dirS > 0) bulb(a + trail, a + runPx + lead)
    else bulb(a - lead, a + runPx - trail)
  }
}
