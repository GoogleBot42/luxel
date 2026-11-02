// name: Fibonacci Clock
// tags: panel, clock
// Curated original (hand-written showcase of the Luxel language/builtins).
// A Mondrian-like golden rectangle of five squares sized 1, 1, 2, 3, 5 that
// secretly tells the time: add the hour squares (red) for the hour and the
// minute squares (green) for the minutes in fives; blue counts for both. The
// colouring reshuffles among the valid answers every few seconds, each square
// flipping over like a card; four dots under it count the spare minutes.

var palette = 0
var shuffleS = 15

// 0 classic red/green/blue on grey, 1 Mondrian red/yellow/blue on white, 2 neon
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 2) }

// Re-deal the colours among the other valid answers this often, seconds.
//# min=5 max=60 step=1 default=15
export function sliderReshuffleS(v) { shuffleS = clamp(v, 5, 60) }

// squares 1, 1, 2, 3, 5 in an 8x5 golden rectangle (unit cells)
var VAL = [1, 1, 2, 3, 5]
var SX = [2, 2, 0, 0, 3], SY = [0, 1, 0, 2, 0]
// state per square: 0 off, 1 hours, 2 minutes, 3 both; hue/sat/val per palette
var PH = [0, 0, 0.33, 0.62, 0, 0, 0.14, 0.64, 0.66, 0.9, 0.48, 0.76]
var PS = [0, 0.85, 0.8, 0.85, 0.06, 0.9, 0.95, 0.9, 0.6, 0.85, 0.9, 0.8]
var PV = [0.13, 0.9, 0.8, 0.95, 0.6, 0.9, 0.95, 0.8, 0.09, 1, 0.9, 1]
var cur = array(5), nxt = array(5), flipT = array(5)
var lastKey = -1, sinceShuffle = 0, spare = 0, t = 0, hMask = -1, mMask = -1

var SUM = array(32)                      // value of each 5-bit subset of the squares
for (var m0 = 0; m0 < 32; m0++) for (var b0 = 0; b0 < 5; b0++) if ((m0 >> b0) & 1) SUM[m0] += VAL[b0]

function pick(target, avoid) {           // a random subset summing to target, not the last one
  var n = 0, m, c = 0, k = avoid
  for (m = 0; m < 32; m++) if (SUM[m] == target) n++
  var skip = n > 1 ? avoid : -1
  if (skip >= 0) n--
  var choice = floor(random(n))
  for (m = 0; m < 32; m++) {
    if (SUM[m] != target || m == skip) continue
    if (c++ == choice) k = m
  }
  return k
}

function deal() {
  var h = clockHour() % 12, m = clockMinute()
  if (h == 0) h = 12
  hMask = pick(h, hMask)
  mMask = pick(floor(m / 5), mMask)
  spare = m % 5
  for (var i = 0; i < 5; i++) {
    var st = ((hMask >> i) & 1) + 2 * ((mMask >> i) & 1)
    if (st != nxt[i]) { nxt[i] = st; flipT[i] = -0.12 * i }
  }
  sinceShuffle = 0
}

export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  t = mod(t + dt, 3600)
  var key = clockHour() * 12 + floor(clockMinute() / 5)
  sinceShuffle += dt
  if (key != lastKey || sinceShuffle >= shuffleS) { lastKey = key; deal() }
  spare = clockMinute() % 5
  for (var i = 0; i < 5; i++) {
    if (flipT[i] < 1) flipT[i] += dt / 0.7
    if (flipT[i] >= 0.5) cur[i] = nxt[i]
  }
}

var gw = 32, gh = 32, kx = 1 / 31, ky = 1 / 31
function rect(x0, y0, x1, y1) { fillRect((x0 - 0.4) * kx, (y0 - 0.4) * ky, (x1 + 0.4) * kx, (y1 + 0.4) * ky) }
function colour(st, k) { var j = palette * 4 + st; hsv(PH[j], PS[j], PV[j] * k) }

export function renderBulk() {
  gw = gridWidth(); gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  kx = 1 / (gw - 1); ky = 1 / (gh - 1)
  var gap = palette == 1 && gw >= 48 ? 2 : 1
  var u = max(2, floor(min((gw - gap) / 8, (gh - gap - 4) / 5)))
  var ox = floor((gw - 8 * u - gap) / 2), oy = floor((gh - 5 * u - gap - 4) / 2)
  clear()
  for (var i = 0; i < 5; i++) {
    var x0 = ox + SX[i] * u + gap, y0 = oy + SY[i] * u + gap
    var x1 = ox + (SX[i] + VAL[i]) * u - 1, y1 = oy + (SY[i] + VAL[i]) * u - 1
    var p = clamp(flipT[i], 0, 1)
    var half = (x1 - x0) / 2 * abs(cos(p * PI)), mid = (x0 + x1) / 2
    var sheen = triangle(t / 11 - (SX[i] + SY[i] + VAL[i]) / 16)
    var k = 0.82 + 0.18 * pow(sheen, 6)
    colour(cur[i], k)
    rect(mid - half, y0, mid + half, y1)
    if (u >= 5 && half >= 1) {               // bevel: lit top-left, shaded bottom-right
      var j = palette * 4 + cur[i]
      hsv(PH[j], PS[j] * 0.8, min(1, PV[j] * k * 1.25 + 0.04))
      rect(mid - half, y0, mid + half, y0); rect(mid - half, y0, mid - half, y1)
      colour(cur[i], k * 0.62)
      rect(mid - half, y1, mid + half, y1); rect(mid + half, y0 + 1, mid + half, y1)
    }
  }
  var dy = oy + 5 * u + gap + 2, ds = max(1, floor(u / 3))   // spare-minute dots
  for (i = 0; i < 4; i++) {
    var dx = floor(gw / 2) + (i - 2) * (ds + 2) + 1
    if (i < spare) colour(3, 1)
    else colour(0, palette == 1 ? 0.3 : 1)
    rect(dx, dy, dx + ds - 1, dy + ds - 1)
  }
}
