// name: Stacker
// tags: strip, ambient
// Clean-room reimplementation from the prose spec docs/pattern-specs/stacker.md — reimagined, not a replica.
// Strip filler: in every segment a block glides in from both ends and locks onto a
// pile growing out of the centre. Each lock flashes white, a full segment blinks and
// collapses back into its core, and stacking starts over in new colours. One
// half-segment is simulated and mirrored into every segment; renderBulk draws it with
// a few fillRange calls per block (anti-aliased block edges, no pixel buffers).
// On a panel the piles are columns: blocks drop from the top and stack up from the
// floor, two columns per segment, each column on its own round and pace.

var speed = 30          // block travel, px/s
var blockPx = 6         // block length, px
var segments = 3
var mode = 0            // 0 bands, 1 rainbow, 2 solid (the pickers)
var h1 = 0.0, s1 = 0.9  // solid pile colour
var h2 = 0.55, s2 = 1   // solid block colour

//# min=5 max=120 step=1 default=30
export function sliderBlockSpeed(v) { speed = clamp(v, 1, 500) }

//# min=1 max=20 step=1 default=6
export function sliderBlockSize(v) { blockPx = clamp(floor(v), 1, 64) }

//# min=1 max=6 step=1 default=3
export function sliderSegments(v) {
  var n = clamp(floor(v), 1, 6)
  if (n != segments) { segments = n; restart() }
}

// 0 bands (every block its own hue), 1 rainbow, 2 solid — the two pickers below
//# min=0 max=2 step=1 default=0
export function sliderColour(v) { mode = clamp(floor(v), 0, 2) }

// Solid mode only. The picker's V is ignored — brightness is the device's.
export function hsvPickerPileColour(h, s, v) { h1 = h; s1 = s }
export function hsvPickerBlockColour(h, s, v) { h2 = h; s2 = s }

var halfLen = 1, w = 1  // simulated half-segment and effective block length
var pos = 0             // block's trailing edge, px from the segment end
var stackH = 0          // pile length, px from the centre
var nBlocks = 0
var phase = 0           // 0 stacking, 1 full: blink, 2 collapse
var phaseT = 0
var lockAge = 9
var serial = 0          // colour seed, advances every round
var rainbow = 0

function restart() {
  halfLen = max(1, floor(floor(pixelCount / segments) / 2))
  w = halfLen / max(3, round(halfLen / blockPx))   // whole blocks fill the half exactly
  pos = 0
  stackH = 0
  nBlocks = 0
  phase = 0
  phaseT = 0
}
restart()

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  pdt = dt
  if (stackH == 0 && phase == 0) w = halfLen / max(3, round(halfLen / blockPx))
  rainbow = time(0.06)
  lockAge += dt
  if (phase == 0) {
    pos += speed * dt
    if (pos + w >= halfLen - stackH - 0.001) {
      stackH += w
      nBlocks++
      lockAge = 0
      pos = 0
      if (stackH >= halfLen - 0.01) { stackH = halfLen; phase = 1; phaseT = 0 }
    }
  } else {
    phaseT += dt
    if (phase == 1 && phaseT >= 0.7) { phase = 2; phaseT = 0 }
    if (phase == 2 && phaseT >= 0.5) { restart(); serial = mod(serial + 7, 997) }
  }
}

function blockHue(j) {
  if (mode == 0) return (serial + j) * 0.618
  if (mode == 1) return rainbow + j * w / halfLen * 0.75
  return h1
}

// Brush for pile block j (0 = innermost); `flash` 0..1 bleaches toward white.
function blockBrush(j, flash) {
  var s = mode == 2 ? s1 : (mode == 0 ? 0.85 : 1)
  hsv(blockHue(j), s * (1 - flash), mod(j, 2) ? 0.82 : 1)
}

var bc = array(3)       // the travelling block's colour, for its partial pixels

// One side's travelling block on [x0, x0 + len), fractional ends lit by coverage.
function drawBlock(x0, len) {
  var x1 = x0 + len
  var a = ceil(x0), b = floor(x1)
  rgb(bc[0], bc[1], bc[2])
  fillRange(a, b)
  var ca = a - x0, cb = x1 - b
  if (ca > 0) { rgb(bc[0] * ca, bc[1] * ca, bc[2] * ca); setPixel(a - 1) }
  if (cb > 0) { rgb(bc[0] * cb, bc[1] * cb, bc[2] * cb); setPixel(b) }
}

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) {}

// ---- panel: one round per column, simulated in rows (block height `wb`)
var NC = 12
var cPos = array(NC), cStack = array(NC), cN = array(NC), cPh = array(NC)
var cT = array(NC), cAge = array(NC), cSer = array(NC), cSp = array(NC)
var pinit = 0, pdt = 0, wm = 31, hm = 31

function pround(k) {
  cPos[k] = 0; cStack[k] = 0; cN[k] = 0; cPh[k] = 0; cT[k] = 0
  cSp[k] = 0.7 + random(0.6)
}

function pHue(k, j, nc, wb, H) {
  if (mode == 0) return (cSer[k] + j) * 0.618
  if (mode == 1) return rainbow + j * wb / H * 0.75 + k / nc * 0.25
  return h1
}

// grid rows [top, bot) (fractional) of columns x0..x1 in rgb bc × k; the partial
// end rows lit by coverage, like the strip's block edges
function vband(x0, x1, top, bot, k) {
  var a = ceil(top), e = floor(bot)
  if (e < a) {
    k *= bot - top
    rgb(bc[0] * k, bc[1] * k, bc[2] * k)
    fillRect(x0, (e - 0.5) / hm, x1, (e + 0.5) / hm)
    return
  }
  rgb(bc[0] * k, bc[1] * k, bc[2] * k)
  if (e > a) fillRect(x0, (a - 0.5) / hm, x1, (e - 0.5) / hm)
  var ca = (a - top) * k, ce = (bot - e) * k
  if (ca > 0) { rgb(bc[0] * ca, bc[1] * ca, bc[2] * ca); fillRect(x0, (a - 1.5) / hm, x1, (a - 0.5) / hm) }
  if (ce > 0) { rgb(bc[0] * ce, bc[1] * ce, bc[2] * ce); fillRect(x0, (e - 0.5) / hm, x1, (e + 0.5) / hm) }
}

function renderPanel() {
  var W = gridWidth(), H = gridHeight()
  if (W < 2 || H < 2) { W = 32; H = 32 }
  wm = W - 1; hm = H - 1
  var nc = clamp(segments * 2, 2, min(NC, floor(W / 2)))
  var nb = max(6, round(H / blockPx)), wb = H / nb
  var dt = pdt, k, j
  if (!pinit) {
    // open mid-game: every column part-built, so the first frame already reads
    pinit = 1
    for (k = 0; k < NC; k++) {
      pround(k)
      cSer[k] = k * 3
      cN[k] = floor(random(nb - 1))
      cStack[k] = cN[k] * wb
      cAge[k] = 9
    }
  }
  hsv(mode == 2 ? h2 : rainbow, 0.7, 0.035)
  fill()
  for (k = 0; k < nc; k++) {
    // simulate
    cAge[k] += dt
    if (cPh[k] == 0) {
      cPos[k] += speed * cSp[k] * dt
      if (cPos[k] + wb >= H - cStack[k] - 0.001) {
        cStack[k] += wb; cN[k]++; cAge[k] = 0; cPos[k] = 0
        if (cN[k] >= nb) { cStack[k] = H; cPh[k] = 1; cT[k] = 0 }
      }
    } else {
      cT[k] += dt
      if (cPh[k] == 1 && cT[k] >= 0.7) { cPh[k] = 2; cT[k] = 0 }
      if (cPh[k] == 2 && cT[k] >= 0.5) { pround(k); cSer[k] = mod(cSer[k] + 7, 997) }
    }
    // draw: column cells [c0, c1), a one-cell gutter on the right
    var c0 = floor(k * W / nc), c1 = floor((k + 1) * W / nc)
    if (c1 - c0 >= 3) c1--
    var x0 = (c0 - 0.5) / wm, x1 = (c1 - 0.5) / wm
    var shown = cStack[k]
    if (cPh[k] == 2) shown = cStack[k] * (1 - easeInQuad(cT[k] / 0.5))
    var blink = cPh[k] == 1 ? square(cT[k] * 3.5, 0.5) * 0.8 : 0
    var full = ceil(shown / wb - 0.001)
    for (j = 0; j < full; j++) {
      var fl = blink
      if (j == cN[k] - 1 && cAge[k] < 0.25) fl = max(fl, 1 - cAge[k] * 4)
      var s = mode == 2 ? s1 : (mode == 0 ? 0.85 : 1)
      hsv2rgb(pHue(k, j, nc, wb, H), s * (1 - fl), 1, bc)
      vband(x0, x1, H - min(shown, (j + 1) * wb), H - j * wb, mod(j, 2) ? 0.82 : 1)
    }
    if (cPh[k] == 0) {
      if (mode == 2) hsv2rgb(h2, s2, 1, bc)
      else hsv2rgb(pHue(k, cN[k], nc, wb, H), 0.75, 1, bc)
      vband(x0, x1, cPos[k], cPos[k] + wb, 1)
    }
  }
}

export function renderBulk() {
  if (has2DMap()) { renderPanel(); return }
  hsv(mode == 2 ? h2 : blockHue(nBlocks), 0.7, 0.035)
  fill()
  var shown = stackH
  if (phase == 2) shown = stackH * (1 - easeInQuad(phaseT / 0.5))
  var blink = phase == 1 ? square(phaseT * 3.5, 0.5) : 0
  var full = ceil(shown / w)
  if (mode == 2) hsv2rgb(h2, s2, 1, bc)
  else hsv2rgb(blockHue(nBlocks), 0.75, 1, bc)
  var len = w
  for (var k = 0; k < segments; k++) {
    var a = floor(k * pixelCount / segments)
    var b = floor((k + 1) * pixelCount / segments)
    var cl = a + halfLen, cr = b - halfLen   // the centre core is [cl, cr)
    for (var j = 0; j < full; j++) {
      var lo = j * w, hi = min(shown, (j + 1) * w)
      var fl = blink
      if (j == nBlocks - 1 && lockAge < 0.25) fl = max(fl, 1 - lockAge * 4)
      blockBrush(j, fl)
      fillRange(cl - hi, cl - lo)
      fillRange(cr + lo, cr + hi)
      if (j == 0) fillRange(cl, cr)
    }
    if (phase == 0) {
      drawBlock(a + pos, len)
      drawBlock(b - pos - len, len)
    }
  }
}
