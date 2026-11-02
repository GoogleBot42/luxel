// name: Block Clock
// tags: panel, clock, retro
// Curated original (hand-written showcase of the Luxel language/builtins).
// HH:MM built live from falling blocks: each digit is a fixed set of 1-4 cell pieces
// that drop in, turning and sliding into place, and lock with a flash. When a digit
// changes it crumbles away and the new one is rebuilt. Square panels stack HH over MM
// with a seconds bar; wide ones run HH:MM in a row with a blinking colon. renderBulk,
// one or two fillRects per cell; the clock sizes itself from the grid.

var speed = 12          // fall speed, cells/s
var colours = 0         // 0 blocks, 1 per digit, 2 one hue
var monoHue = 190 / 360
var h24 = 0

//# min=4 max=40 step=1 default=12
export function sliderFallSpeed(v) { speed = clamp(v, 1, 80) }

// 0 every piece its own colour, 1 a hue per digit, 2 one hue (the dial below)
//# min=0 max=2 step=1 default=0
export function sliderColours(v) { colours = clamp(floor(v), 0, 2) }

//# min=0 max=360 step=1 default=190
export function sliderHue(v) { monoHue = mod(v, 360) / 360 }

//# default=0
export function toggle24Hour(v) { h24 = v }

// Digits 0-9 on a 3x5 cell grid, as pieces in drop order (bottom first): each piece is
// a cell count then its cells (col + 3*row); 0 ends a digit.
var PCS = [4, 5, 8, 11, 14, 4, 6, 9, 12, 13, 4, 0, 1, 2, 3, 0, 4, 10, 12, 13, 14, 4, 1, 3, 4, 7, 0,
  3, 12, 13, 14, 4, 6, 7, 8, 9, 4, 0, 1, 2, 5, 0, 3, 12, 13, 14, 4, 6, 7, 8, 11, 4, 0, 1, 2, 5, 0,
  1, 14, 4, 2, 5, 8, 11, 4, 0, 3, 6, 7, 0, 3, 12, 13, 14, 4, 6, 7, 8, 11, 4, 0, 1, 2, 3, 0,
  4, 11, 12, 13, 14, 4, 6, 7, 8, 9, 4, 0, 1, 2, 3, 0, 3, 8, 11, 14, 4, 0, 1, 2, 5, 0,
  4, 9, 12, 13, 14, 1, 11, 4, 5, 6, 7, 8, 4, 0, 1, 2, 3, 0, 4, 11, 12, 13, 14, 4, 5, 6, 7, 8, 4, 0, 1, 2, 3, 0]
var START = [0, 16, 27, 42, 57, 70, 85, 101, 111, 129]
var PAL = [0.5, 0.15, 0.8, 0.33, 0.0, 0.62, 0.07]   // seven block hues

var val = array(4), want = array(4), built = array(4), nPc = array(4)
var fOff = array(4), fD = array(4), rot0 = array(4), slide0 = array(4)
var dissT = array(4), lockT = array(4), seed = array(4), wait = array(4)
var DX = array(4), DY = array(4)                     // digit origins, cells
var gw = 0, gh = 0, cpx = 2, ox = 0, oy = 0, stacked = 1, seenW = -1, seenH = -1
var sec = 0, secFrac = 0, glint = 0
var BGH = array(8), BGV = array(8)
for (var b0 = 0; b0 < 8; b0++) { BGH[b0] = 0.64 + b0 * 0.012; BGV[b0] = 0.045 - b0 * 0.0047 }

function pieceAt(d, k) {                             // PCS offset of piece k of digit d
  var p = START[val[d]]
  for (var i = 0; i < k; i++) p += PCS[p] + 1
  return p
}

function countPieces(v) {
  var p = START[v], n = 0
  while (PCS[p] > 0) { p += PCS[p] + 1; n++ }
  return n
}

function spawn(d) {
  if (val[d] < 0 || built[d] >= nPc[d]) return
  var p = pieceAt(d, built[d])
  var top = floor(PCS[p + 1] / 3)                    // its highest cell's row
  fD[d] = (oy / cpx + DY[d] + top) + 4
  fOff[d] = -fD[d]
  rot0[d] = floor(random(4))
  slide0[d] = floor(random(5)) - 2
}

function layout() {
  seenW = gridWidth(); seenH = gridHeight()
  gw = seenW; gh = seenH
  if (gw == 0) { gw = 32; gh = 32 }
  var row = min(floor(gw / 17), floor(gh / 8))
  var stack = min(floor(gw / 8), floor(gh / 14))
  stacked = stack > row
  cpx = max(1, stacked ? stack : row)
  var bw = stacked ? 7 : 17, bh = stacked ? 12 : 5
  ox = floor((gw - bw * cpx) / 2)
  oy = floor((gh - bh * cpx) / 2 + (stacked ? cpx * 0.5 : cpx))
  for (var d = 0; d < 4; d++) {
    DX[d] = stacked ? (d % 2) * 4 : (d < 2 ? d * 4 : 10 + (d - 2) * 4)
    DY[d] = stacked ? floor(d / 2) * 7 : 0
    val[d] = -1; built[d] = 0; dissT[d] = -1; wait[d] = d * 0.5
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  if (gridWidth() != seenW || gridHeight() != seenH) layout()
  var h = clockHour(), m = clockMinute(), s = clockSecond()
  if (!h24) { h = h % 12; if (h == 0) h = 12 }
  want[0] = h >= 10 || h24 ? floor(h / 10) : -1
  want[1] = h % 10
  want[2] = floor(m / 10)
  want[3] = m % 10
  if (s != sec) { sec = s; secFrac = 0 } else secFrac = min(secFrac + dt, 0.999)
  glint = mod(glint + dt / 7, 1)
  for (var d = 0; d < 4; d++) {
    lockT[d] += dt
    if (wait[d] > 0) { wait[d] -= dt; continue }
    if (dissT[d] >= 0) {                             // crumbling
      dissT[d] += dt
      if (dissT[d] > 1.1) { dissT[d] = -1; val[d] = -1 }
      continue
    }
    if (want[d] != val[d]) {
      if (val[d] >= 0 && built[d] > 0) { dissT[d] = 0; continue }
      val[d] = want[d]; built[d] = 0; seed[d] = random(7)
      nPc[d] = val[d] < 0 ? 0 : countPieces(val[d])
      spawn(d)
    }
    if (val[d] >= 0 && built[d] < nPc[d]) {
      fOff[d] += speed * dt * (0.6 + 0.4 * min(1, (fD[d] + fOff[d]) / 3))   // a little ease-in
      if (fOff[d] >= 0) { built[d]++; lockT[d] = 0; spawn(d) }
    }
  }
}

// One cell at pixel (x, y): a shaded rim and an inset face once cells are 3+ px.
function cell(x, y, hh, ss, vv) {
  var x0 = (x - 0.3) / (gw - 1), y0 = (y - 0.3) / (gh - 1)
  if (cpx >= 3) {
    hsv(hh, ss, vv * 0.45)
    fillRect(x0, y0, (x + cpx - 0.7) / (gw - 1), (y + cpx - 0.7) / (gh - 1))
    hsv(hh, ss * 0.85, vv)
    fillRect(x0, y0, (x + cpx - 1.7) / (gw - 1), (y + cpx - 1.7) / (gh - 1))
  } else {
    hsv(hh, ss, vv)
    fillRect(x0, y0, (x + cpx - 0.7) / (gw - 1), (y + cpx - 0.7) / (gh - 1))
  }
}

function pieceHue(d, k) {
  if (colours == 1) return time(0.15) + d * 0.13 + 0.02 * k
  if (colours == 2) return monoHue
  return PAL[floor(seed[d] + k * 3) % 7]
}

export function renderBulk() {
  fillCanvas(BGH, 0.85, BGV, 1, 8)                   // night-blue backdrop, fading down
  var d, k, i, p, n, c, cx, cy
  for (d = 0; d < 4; d++) {
    if (val[d] < 0) continue
    p = START[val[d]]
    for (k = 0; k < nPc[d]; k++) {
      n = PCS[p]
      var hh = pieceHue(d, k), ss = colours == 1 ? 0.75 : 0.9
      for (i = 1; i <= n; i++) {
        c = PCS[p + i]
        cx = DX[d] + c % 3
        cy = DY[d] + floor(c / 3)
        if (k >= built[d]) {                         // not yet built: a faint ghost
          if (dissT[d] < 0) cell(ox + cx * cpx, oy + cy * cpx, hh, 0.5, 0.07)
          continue
        }
        var vv = 0.85, y = oy + cy * cpx
        var g = abs(mod(cx + cy * 0.5, 22) / 22 - glint)
        if (g < 0.06) vv = 1                         // a glint sweeps the face now and then
        var wh = k == built[d] - 1 && lockT[d] < 0.3 ? 1 - lockT[d] / 0.3 : 0
        if (dissT[d] >= 0) {
          var t = dissT[d] - hash(d * 16 + c) * 0.4
          if (t > 0) { y += 20 * t * t * cpx; vv *= max(0, 1 - t * 1.6) }
          if (vv <= 0) continue
        }
        cell(ox + cx * cpx, y, hh, ss * (1 - wh), vv)
      }
      p += n + 1
    }
    if (dissT[d] < 0 && built[d] < nPc[d]) {         // the falling piece
      p = pieceAt(d, built[d])
      n = PCS[p]
      var q = clamp((1 + fOff[d] / fD[d]) / 0.55, 0, 1)
      var r = ceil(rot0[d] * (1 - q)), sl = round(slide0[d] * (1 - q))
      var mnx = 9, mxx = -9, mny = 9, mxy = -9
      for (i = 1; i <= n; i++) {
        c = PCS[p + i]
        mnx = min(mnx, c % 3); mxx = max(mxx, c % 3); mny = min(mny, floor(c / 3)); mxy = max(mxy, floor(c / 3))
      }
      var pcx = (mnx + mxx) / 2, pcy = (mny + mxy) / 2
      for (i = 1; i <= n; i++) {
        c = PCS[p + i]
        cx = c % 3; cy = floor(c / 3)
        for (k = 0; k < r; k++) { var t2 = cx; cx = pcx - (cy - pcy); cy = pcy + (t2 - pcx) }
        cell(ox + (DX[d] + cx + sl) * cpx, oy + (DY[d] + cy + fOff[d]) * cpx, pieceHue(d, built[d]), 0.8, 1)
      }
    }
  }
  if (stacked) {                                     // seconds bar between the rows
    var th = max(1, floor(cpx / 2))
    var by0 = (oy + 6 * cpx - floor(th / 2) - 0.3) / (gh - 1), by1 = (oy + 6 * cpx - floor(th / 2) + th - 0.7) / (gh - 1)
    hsv(0.6, 0.5, 0.08)
    fillRect((ox - 0.3) / (gw - 1), by0, (ox + 7 * cpx - 0.7) / (gw - 1), by1)
    hsv(colours == 2 ? monoHue : 0.55, 0.5, 0.75)
    fillRect((ox - 0.3) / (gw - 1), by0, (ox + (sec + secFrac) / 60 * 7 * cpx - 0.7) / (gw - 1), by1)
  } else {                                           // the colon blinks the seconds
    var cv = mod(sec, 2) ? 0.2 : 0.9
    cell(ox + 8 * cpx, oy + cpx, 0.55, 0.3, cv)
    cell(ox + 8 * cpx, oy + 3 * cpx, 0.55, 0.3, cv)
  }
}
