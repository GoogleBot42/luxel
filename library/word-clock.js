// name: Word Clock
// tags: panel, clock
// Curated original (hand-written showcase of the Luxel language/builtins).
// The time in words: an 11x10 letter grid in dim grey where "IT IS TWENTY FIVE
// PAST SEVEN" lights up, rounded down to five minutes; four corner dots count
// the minutes in between. Words cross-fade and sparkle as they change. One
// drawText per row for the grid, one per lit word; below 48x62 it falls back to
// the lit words alone, one per line, centred.

var fadeS = 1.2
var litH = 0.09, litS = 0.35
var rainbow = 0

// How long a word takes to fade in or out, seconds.
//# min=0.2 max=4 step=0.1 default=1.2
export function sliderFadeTimeS(v) { fadeS = clamp(v, 0.2, 4) }

// Colour of the lit words (V is ignored).
export function hsvPickerWordColour(h, s, v) { litH = h; litS = s }

// Each lit word in its own slowly drifting hue instead of the picked colour.
//# default=0
export function toggleRainbow(v) { rainbow = v }

var NW = 22
var ROW = [0, 0, 1, 1, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 5, 6, 6, 7, 7, 8, 8, 9]
var COL = [0, 3, 0, 8, 0, 7, 0, 5, 9, 0, 3, 6, 0, 4, 8, 0, 6, 0, 4, 0, 4, 5]
var LEN = [2, 2, 7, 3, 6, 4, 4, 4, 2, 3, 3, 5, 4, 4, 3, 5, 5, 4, 6, 3, 6, 6]
var HOURW = [18, 9, 10, 11, 12, 13, 14, 15, 16, 17, 19, 20, 18]   // hour 0..12 -> word
var MINW = [-1, 5, 3, 2, 4, 4, 6, 4, 4, 2, 3, 5]               // first minute word per 5-min step
var lvl = array(22), want = array(22), spark = array(22)
var order = array(6), nOrder = 0
var lastKey = -1, mDots = 0, dotLvl = array(4), t = 0

function setWords() {
  var h = clockHour(), m = clockMinute(), m5 = floor(m / 5), i
  for (i = 0; i < NW; i++) want[i] = 0
  nOrder = 0
  want[0] = 1; want[1] = 1
  if (m5 > 0) {
    want[MINW[m5]] = 1; order[nOrder++] = MINW[m5]
    if (m5 == 5 || m5 == 7) { want[5] = 1; order[nOrder++] = 5 }
    var rel = m5 <= 6 ? 7 : 8
    want[rel] = 1; order[nOrder++] = rel
  }
  if (m5 > 6) h = h + 1
  var hw = HOURW[h % 12 == 0 ? 12 : h % 12]
  want[hw] = 1; order[nOrder++] = hw
  if (m5 == 0) { want[21] = 1; order[nOrder++] = 21 }
  mDots = m % 5
}

export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000, i
  t = mod(t + dt, 3600)
  var key = clockHour() * 100 + clockMinute()
  if (key != lastKey) {
    lastKey = key
    setWords()
    for (i = 0; i < NW; i++) if (want[i] && lvl[i] < 0.5) spark[i] = 1
  }
  for (i = 0; i < NW; i++) {
    lvl[i] = want[i] ? min(1, lvl[i] + dt / fadeS) : max(0, lvl[i] - dt / fadeS)
    spark[i] = max(0, spark[i] - dt / 0.9)
  }
  for (i = 0; i < 4; i++) dotLvl[i] = i < mDots ? min(1, dotLvl[i] + dt / fadeS) : max(0, dotLvl[i] - dt / fadeS)
}

function drawRow(r, x, y) {
  if (r == 0) drawText("ITNISXLUXEL", x, y)
  else if (r == 1) drawText("QUARTERZTEN", x, y)
  else if (r == 2) drawText("TWENTYXFIVE", x, y)
  else if (r == 3) drawText("HALFXPASTTO", x, y)
  else if (r == 4) drawText("ONETWOTHREE", x, y)
  else if (r == 5) drawText("FOURFIVESIX", x, y)
  else if (r == 6) drawText("SEVENJEIGHT", x, y)
  else if (r == 7) drawText("NINETWELVEZ", x, y)
  else if (r == 8) drawText("TENKELEVENQ", x, y)
  else drawText("ZBRYXOCLOCK", x, y)
}

function drawWord(w, x, y, al) {
  if (w == 0) drawText("IT", x, y, al)
  else if (w == 1) drawText("IS", x, y, al)
  else if (w == 2) drawText("QUARTER", x, y, al)
  else if (w == 3 || w == 19) drawText("TEN", x, y, al)
  else if (w == 4) drawText("TWENTY", x, y, al)
  else if (w == 5 || w == 13) drawText("FIVE", x, y, al)
  else if (w == 6) drawText("HALF", x, y, al)
  else if (w == 7) drawText("PAST", x, y, al)
  else if (w == 8) drawText("TO", x, y, al)
  else if (w == 9) drawText("ONE", x, y, al)
  else if (w == 10) drawText("TWO", x, y, al)
  else if (w == 11) drawText("THREE", x, y, al)
  else if (w == 12) drawText("FOUR", x, y, al)
  else if (w == 14) drawText("SIX", x, y, al)
  else if (w == 15) drawText("SEVEN", x, y, al)
  else if (w == 16) drawText("EIGHT", x, y, al)
  else if (w == 17) drawText("NINE", x, y, al)
  else if (w == 18) drawText("TWELVE", x, y, al)
  else if (w == 20) drawText("ELEVEN", x, y, al)
  else drawText("OCLOCK", x, y, al)
}

function brush(w, k) {
  var hh = rainbow ? litH + w * 0.13 + t / 40 : litH
  var ss = rainbow ? 0.75 : litS
  hsv(hh, ss * k, 0.1 + 0.9 * k)
}

export function renderBulk() {
  var gw = gridWidth(), gh = gridHeight(), i, w
  if (gw == 0) return
  clear()
  if (gw >= 72 && gh >= 92) font("regular")
  else font("tiny")
  var cw = textWidth("A"), rh = cw > 4 ? 9 : 6
  var gridOk = gw >= 11 * cw + 4 && gh >= 10 * rh + 2
  if (gridOk) {
    var ox = floor((gw - 11 * cw + 1) / 2), oy = floor((gh - 10 * rh + 1) / 2)
    for (i = 0; i < 10; i++) {               // dim grid, a slow glint rolling down it
      var g = triangle(t / 9 - i / 24)
      hsv(0.6, 0.1, 0.1 + 0.08 * g * g * g * g)
      drawRow(i, ox, oy + i * rh)
    }
    for (w = 0; w < NW; w++) {
      if (lvl[w] <= 0) continue
      var x = ox + COL[w] * cw, y = oy + ROW[w] * rh
      brush(w, easeInOutSine(lvl[w]))
      drawWord(w, x, y, 0)
      if (spark[w] > 0) {                    // a few glints run over a word as it arrives
        hsv(0.12, 0.2, spark[w])
        for (i = 0; i < 3; i++) {
          var hk = hash2(w * 7 + i, floor(t * 14))
          setPixel(gridIndex(x + floor(hk * LEN[w] * cw), y + floor(hash(hk * 99) * (rh - 1))))
        }
      }
    }
  } else {                                    // small panel: just the lit words, centred
    var lh = rh + 1, top = floor((gh - nOrder * lh) / 2) + 1
    for (i = 0; i < nOrder; i++) {
      w = order[i]
      brush(w, max(0.25, easeInOutSine(lvl[w])))
      drawWord(w, floor(gw / 2), top + i * lh, 1)
    }
  }
  var d = gridOk ? 1 : 0                      // minute dots in the four corners
  for (i = 0; i < 4; i++) {
    var cx = i == 0 || i == 3 ? d : gw - 1 - d, cy = i < 2 ? d : gh - 1 - d
    brush(21, dotLvl[i] * 0.9)
    setPixel(gridIndex(cx, cy))
  }
}
