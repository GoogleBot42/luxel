// name: Lotus Bloom
// tags: panel, nature
// Clean-room reimplementation from the prose specs docs/pattern-specs/infinity-flower-2d.md
// and docs/pattern-specs/patternflow/lotus-bloom.md (Patternflow preset, CC BY-SA 4.0 —
// prose-described idea only, no code) — reimagined, not a replica.
// Golden bloom: a flower of tapered drawLine petals set at golden-angle steps
// round a sunflower seed head; it opens with easeOutBack, holds, wilts while
// the next species opens. Rose mandala: a polar rose drawn as distance bands
// (pale outline, inner glow, crawling striped core, spoke aura) on a cached
// r/theta canvas (<= 40 wide), upscaled bilinearly with fillCanvas.

var mode = 0
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1) }

var petals = 8
//# min=3 max=12 step=1 default=8
export function sliderPetals(v) { petals = clamp(floor(v), 3, 12) }

// Seconds each flower stays fully open.
var holdS = 5
//# min=1 max=20 step=0.5 default=5
export function sliderHoldSeconds(v) { holdS = clamp(v, 1, 20) }

// Flower radius in percent of half the panel's short side.
var sizePct = 92
//# min=40 max=100 step=1 default=92
export function sliderSize(v) { sizePct = clamp(v, 40, 100) }

var GOLD = 2.39996, OPEN = 1.6, WILT = 1.4

// two species slots: the one opening and the one wilting behind it
var spHue = array(2), spSpan = array(2), spW = array(2), spRot = array(2), spAge = array(2), spDir = array(2)
var cur = 0
function newSpecies(s) {
  spHue[s] = random(1)
  spSpan[s] = 0.04 + random(0.16)
  spW[s] = 1 + random(0.45)
  spRot[s] = random(PI2)
  spDir[s] = random(1) < 0.5 ? -1 : 1
  spAge[s] = 0
}
newSpecies(0)
spAge[0] = OPEN
spAge[1] = 99

// rose-mandala canvas, with radius and angle cached per cell
var MAXC = 1600
var cR = array(MAXC), cT = array(MAXC), cX = array(MAXC)
var cH = array(MAXC), cS = array(MAXC), cV = array(MAXC)
var CW = 0, CH = 0, passes = 0, gwSeen = -1, ghSeen = -1, asp = 1

function sizeCanvas() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw < 1) { gw = 32; gh = 32 }
  if (gw == gwSeen && gh == ghSeen) return
  gwSeen = gw
  ghSeen = gh
  asp = gw / gh
  CW = gw
  CH = gh
  passes = 0
  while (CW * CH > MAXC || CW > 40) {
    CW = ceil(CW / 2)
    CH = ceil(CH / 2)
    passes++
  }
  var j = 0
  for (var row = 0; row < CH; row++) {
    for (var col = 0; col < CW; col++) {
      var dx = ((col + 0.5) / CW - 0.5) * asp, dy = (row + 0.5) / CH - 0.5
      cR[j] = hypot(dx, dy)
      cT[j] = atan2(dy, dx) / PI2
      cX[j] = dx + dy
      j++
    }
  }
}

var t = 0, spin = 0, cyc = 0, fld = 0

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  t = mod(t + dt, 3600)
  spin = mod(spin + dt * 0.12, PI2)
  cyc = mod(cyc + dt / (OPEN + holdS + WILT), 1)
  spAge[0] += dt
  spAge[1] += dt
  if (spAge[cur] >= OPEN + holdS) {
    cur = 1 - cur
    newSpecies(cur)
  }
}

function flower(s, R) {
  var age = spAge[s]
  if (age > OPEN + holdS + WILT) return
  var o = easeOutBack(saturate(age / OPEN))
  var w = easeInCubic(saturate((age - OPEN - holdS) / WILT))
  var lum = 1 - w
  var T = petals * 2, hue = spHue[s], span = spSpan[s]
  var rot = spRot[s] + spin * spDir[s] + w * 0.6 * spDir[s]
  var base = R * 0.13
  // outer (longest) petals first, so inner whorls sit on top
  for (var k = T - 1; k >= 0; k--) {
    var f = (k + 1) / T
    var L = R * (0.38 + 0.62 * f) * o * (1 - 0.3 * w)
    if (L <= base) continue
    var a = rot + k * GOLD + 0.04 * sin(t * 1.3 + k)
    var c = cos(a) * ax, sn = sin(a)
    var wd = L * spW[s] * min(o, 1), hk = hue + (1 - f) * 0.08
    // a fat, deep capsule REPLACES what is under it (its dark rim outlines
    // the petal against those behind); a slim pale one max-blends the tip
    hsv(hk, 0.95, 0.8 * lum)
    drawLine(0.5 + c * L * 0.3, 0.5 + sn * L * 0.3, 0.5 + c * L * 0.64, 0.5 + sn * L * 0.64, wd * 0.3, 0)
    hsv(hk + span, 0.5, lum)
    drawLine(0.5 + c * L * 0.55, 0.5 + sn * L * 0.55, 0.5 + c * L * 0.85, 0.5 + sn * L * 0.85, wd * 0.16, 2)
  }
  // sunflower seed head
  hsv(hue + 0.55, 0.9, 0.25 * lum)
  fillCircle(0.5, 0.5, base * 1.25 * min(o, 1))
  var sr = base * 0.2
  for (var q = 1; q <= 34; q++) {
    var rr = base * 1.05 * sqrt(q / 34) * min(o, 1)
    var aq = q * GOLD - spin * 2
    hsv(0.1 + q * 0.002, 0.8 - q * 0.012, lum)
    splat(0.5 + cos(aq) * rr * ax, 0.5 + sin(aq) * rr, sr, 2)
  }
}

var ax = 1
function bloom() {
  clear()
  ax = gridWidth() > 0 ? gridHeight() / gridWidth() : 1
  var R = sizePct / 200
  flower(1 - cur, R)
  flower(cur, R)
}

function mandala() {
  sizeCanvas()
  var R0 = sizePct / 200 * 0.7
  // petals open with easeOutBack and close again once per cycle
  var u = triangle(cyc)
  var amp = 0.12 + 0.3 * easeOutBack(saturate(u * 1.6))
  var half = petals / 2, rot = spin / PI2, crawl = t * 0.6
  var hue = mod(t / 50, 1)
  var aa = 1.6 / CH, wOut = 0.9 / CH, glowB = 0.12, auraB = 0.2
  fld = 1 - fld
  for (var row = fld; row < CH; row += 2) {
    var j = row * CW
    for (var col = 0; col < CW; col++) {
      var r = cR[j], th = cT[j]
      var tgt = R0 * (1 - amp + amp * abs(cos(PI * (th * petals + rot)))) + 0.02 * sin(PI2 * (r * 6 - t * 0.4))
      var d = r - tgt, ad = abs(d)
      var hh = hue, ss = 0.25, vv = 1 - smoothstep(wOut, wOut + aa, ad)
      if (d < 0) {
        var g = 0.7 * (1 - smoothstep(0, glowB, -d))
        if (g > vv) { hh = hue + 0.06; ss = 0.9; vv = g }
        if (r < tgt * 0.45) {
          var st = 0.42 * smoothstep(0.35, 0.65, triangle(cX[j] * 9 - crawl))
          if (st > vv) { hh = hue + 0.5; ss = 0.8; vv = st }
        }
      } else if (d < auraB) {
        var sp = 0.5 * (1 - d / auraB) * smoothstep(0.4, 0.75, triangle(th * petals * 3 - rot * 2))
        if (sp > vv) { hh = hue + 0.72; ss = 0.85; vv = sp }
      }
      cH[j] = hh
      cS[j] = ss
      cV[j] = vv
      j++
    }
  }
  fillCanvas(cH, cS, cV, CW, CH, passes > 0 ? 1 : 0)   // bilinear only when upscaling
}

export function renderBulk() {
  if (mode == 0) bloom()
  else mandala()
}
