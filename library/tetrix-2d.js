// name: Tetrix
// Curated original (hand-written showcase of the Luxel language/builtins).
// A well of columns, each dropping coloured blocks onto its own pile at its own pace.
// Whenever every pile is past the dim clear line, the complete rows flash, vanish and
// everything drops; a pile that hits the top first blinks and burns back to the rest.
// Columns and rows come from the grid; renderBulk draws one fillRect per block.

var colPx = 4           // column width, px (narrowed to keep 8+ columns on small grids)
var dropRate = 8        // falling speed, rows/s
var palette = 0         // 0 classic, 1 neon, 2 ember, 3 ice

//# min=1 max=8 step=1 default=4
export function sliderColumnWidth(v) { colPx = clamp(floor(v), 1, 8) }

//# min=2 max=30 step=1 default=8
export function sliderDropSpeed(v) { dropRate = clamp(v, 0.5, 60) }

// 0 classic, 1 neon, 2 ember, 3 ice
//# min=0 max=3 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 3) }

// hue, saturation pairs — seven per palette
var PAL = [0.5, 0.9, 0.15, 1, 0.8, 0.85, 0.33, 0.95, 0.0, 0.95, 0.64, 0.95, 0.07, 1,
           0.9, 1, 0.5, 1, 0.28, 1, 0.95, 0.75, 0.75, 1, 0.45, 1, 0.16, 1,
           0.0, 1, 0.04, 1, 0.08, 1, 0.11, 0.95, 0.02, 0.8, 0.13, 0.7, 0.06, 0.9,
           0.55, 0.35, 0.6, 0.8, 0.52, 0.6, 0.66, 0.7, 0.5, 0.15, 0.62, 0.5, 0.57, 1]

var MAXC = 64, MAXB = 32
var bS = array(MAXC * MAXB)   // settled blocks per column, bottom first: colour serial
var bH = array(MAXC * MAXB)   //   …and height in cells
var cnt = array(MAXC), hgt = array(MAXC)
var fy = array(MAXC), fh = array(MAXC), fs = array(MAXC), mul = array(MAXC)
var landT = array(MAXC), blinkT = array(MAXC), burnT = array(MAXC), keepH = array(MAXC)
var dropOff = array(MAXC), dropV = array(MAXC)

var gw = 0, gh = 0, cw = 2, cols = 1, rows = 1, ox = 0, oy = 0, clearLine = 2
var clearT = -1, clearK = 0, lowest = 0, seenW = -1, seenH = -1, seenCol = -1

function newBlock(c) {
  fh[c] = 1 + floor(random(3))
  fy[c] = 0
  fs[c] = floor(random(7))
}

function layout() {
  gw = gridWidth()
  gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  cw = max(min(colPx, max(1, floor(gw / 8))), max(ceil(gw / MAXC), ceil(gh / (MAXB * 2))))
  cols = max(1, floor(gw / cw))
  rows = max(2, floor(gh / cw))
  ox = floor((gw - cols * cw) / 2)
  oy = gh - rows * cw
  clearLine = max(2, floor(rows * 0.3))
  clearT = -1
  for (var c = 0; c < cols; c++) {
    cnt[c] = 0; hgt[c] = 0; blinkT[c] = 0; burnT[c] = 0; dropOff[c] = 0; landT[c] = 9
    mul[c] = 0.55 + random(0.9)
    var target = floor(random(rows * 0.45))   // start part-filled, never empty
    while (hgt[c] < target) push(c, min(1 + floor(random(3)), rows - hgt[c]), floor(random(7)))
    newBlock(c)
  }
  seenW = gridWidth(); seenH = gridHeight(); seenCol = colPx
}

function push(c, h, s) {
  var k = c * MAXB + cnt[c]
  bH[k] = h; bS[k] = s
  cnt[c]++
  hgt[c] += h
}

// Remove the bottom `k` rows from column c's pile.
function trim(c, k) {
  var b = c * MAXB, n = cnt[c], drop = 0
  while (drop < n && k >= bH[b + drop]) { k -= bH[b + drop]; drop++ }
  if (drop < n) bH[b + drop] -= k
  for (var i = drop; i < n; i++) { bH[b + i - drop] = bH[b + i]; bS[b + i - drop] = bS[b + i] }
  cnt[c] = n - drop
  hgt[c] = 0
  for (i = 0; i < cnt[c]; i++) hgt[c] += bH[b + i]
}

// Cut column c's pile down to `k` rows from the top.
function trimTop(c, k) {
  var b = c * MAXB
  while (cnt[c] > 0 && hgt[c] > k) {
    var top = b + cnt[c] - 1
    var cut = min(bH[top], hgt[c] - k)
    bH[top] -= cut
    hgt[c] -= cut
    if (bH[top] <= 0) cnt[c]--
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  if (gridWidth() != seenW || gridHeight() != seenH || colPx != seenCol) layout()

  if (clearT >= 0) {                       // line clear: the well pauses
    clearT += dt
    if (clearT >= 0.6) {
      for (var c = 0; c < cols; c++) { trim(c, clearK); dropOff[c] = clearK; dropV[c] = 0 }
      clearT = -1
    }
    return
  }
  var minH = 999
  for (var c = 0; c < cols; c++) {
    landT[c] += dt
    if (dropOff[c] > 0) {                  // settling after a clear
      dropV[c] += 60 * dt
      dropOff[c] = max(0, dropOff[c] - dropV[c] * dt)
    }
    if (blinkT[c] > 0) {                   // overflowed: blink, then burn back
      blinkT[c] -= dt
      if (blinkT[c] <= 0) { blinkT[c] = 0; burnT[c] = 0.5; keepH[c] = min(lowest, rows - 3) }
      continue
    }
    if (burnT[c] > 0) {
      burnT[c] -= dt
      if (burnT[c] <= 0) { burnT[c] = 0; trimTop(c, keepH[c]); mul[c] = 0.55 + random(0.9); newBlock(c) }
      continue
    }
    // a pile far above the lowest one slows down, so the rows do get completed
    fy[c] += dropRate * mul[c] * clamp(1.3 - (hgt[c] - lowest) / (rows * 0.4), 0.3, 1.3) * dt
    if (fy[c] >= rows - hgt[c]) {
      push(c, min(fh[c], rows - hgt[c]), fs[c])
      landT[c] = 0
      if (hgt[c] >= rows || cnt[c] >= MAXB) blinkT[c] = 0.6
      else newBlock(c)
    }
    minH = min(minH, hgt[c])
  }
  lowest = minH
  if (minH >= clearLine && minH < 999) { clearT = 0; clearK = minH }
}

// A block: on 3+ px columns a shaded rim, then the face inset by one pixel.
function drawBlock(c, top, bot, s, v, white) {
  var k = palette * 14 + s * 2
  var x0 = ox + c * cw, y0 = oy + top * cw, y1 = oy + bot * cw
  if (cw >= 3) {
    hsv(PAL[k], PAL[k + 1], v * 0.4)
    fillRect((x0 - 0.3) / (gw - 1), (y0 - 0.3) / (gh - 1), (x0 + cw - 0.7) / (gw - 1), (y1 - 0.7) / (gh - 1))
    hsv(PAL[k], PAL[k + 1] * (1 - white), v)
    fillRect((x0 - 0.3) / (gw - 1), (y0 - 0.3) / (gh - 1), (x0 + cw - 1.7) / (gw - 1), (y1 - 1.7) / (gh - 1))
  } else {
    hsv(PAL[k], PAL[k + 1] * (1 - white), v)
    fillRect((x0 - 0.3) / (gw - 1), (y0 - 0.3) / (gh - 1), (x0 + cw - 0.7) / (gw - 1), (y1 - 0.7) / (gh - 1))
  }
}

export function renderBulk() {
  hsv(0.62, 0.6, 0.03)
  fill()
  hsv(0.62, 0.3, 0.09)                     // the clear line
  fillRect(0, (oy + (rows - clearLine) * cw - 0.3) / (gh - 1), 1, (oy + (rows - clearLine) * cw + 0.3) / (gh - 1))
  for (var c = 0; c < cols; c++) {
    var b = c * MAXB
    var base = rows - dropOff[c]           // row just below the pile's bottom block
    var blink = blinkT[c] > 0 ? square(blinkT[c] * 5, 0.5) : 0
    var burn = burnT[c] * 2, under = 0
    for (var i = 0; i < cnt[c]; i++) {
      var top = base - bH[b + i]
      var wh = blink, v = mod(i, 2) ? 0.7 : 0.85
      if (i == cnt[c] - 1 && landT[c] < 0.2) wh = max(wh, 1 - landT[c] * 5)
      if (burnT[c] > 0 && under + bH[b + i] > keepH[c]) { wh = 0.6; v *= burn * burn }
      drawBlock(c, top, base, bS[b + i], v, wh)
      under += bH[b + i]
      base = top
    }
    if (blinkT[c] == 0 && burnT[c] == 0) {
      drawBlock(c, fy[c] - fh[c], fy[c], fs[c], 1, 0.1)
    }
  }
  if (clearT >= 0) {
    rgb(1, 1, 1)
    if (square(clearT * 5, 0.5)) fillRect(0, (oy + (rows - clearK) * cw - 0.3) / (gh - 1), 1, 1)
  }
}
