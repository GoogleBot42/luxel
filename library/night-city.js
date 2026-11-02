// name: Night City
// Curated original (hand-written showcase of the Luxel language/builtins).
// A skyline after dark: two rows of towers whose windows flick on and off
// (tungsten, flickering TV blue, the odd party colour), a red beacon on the
// tallest roof, a plane crossing now and then and cars on the street. The sky
// runs from dusk to midnight by the real clock, and so does how many windows
// are lit. Stateless: buildings and windows are hashes, drawn with fillRect.

var MAXB = 48
var bL = array(MAXB), bR = array(MAXB), bT = array(MAXB), bS = array(MAXB)
var nb = 0, nfar = 0, tallest = 0, lastW = -1, lastH = -1
var GW = 32, GH = 32, ix = 1 / 31, iy = 1 / 31, u = 1, ws = 1, street = 30

// Hour shown: 0 follows the device clock; 1..24 pins one (24 = midnight).
var hourSet = 0
//# min=0 max=24 step=0.5 default=0
export function sliderHour(v) { hourSet = clamp(v, 0, 24) }

// Mean seconds a window keeps its state before it may flip.
var flipS = 20
//# min=3 max=120 step=1 default=20
export function sliderWindowChangeSeconds(v) { flipS = clamp(v, 3, 120) }

// Share of lit windows throwing a party (colour-cycling).
var partyPct = 3
//# min=0 max=20 step=1 default=3
export function sliderPartyPct(v) { partyPct = clamp(v, 0, 20) }

// Cars on the street.
var cars = 4
//# min=0 max=10 step=1 default=4
export function sliderCars(v) { cars = clamp(floor(v), 0, 10) }

setFrameRate(20)                  // a city moves slowly; halve the work

function cellRect(c0, r0, c1, r1) {
  fillRect((c0 - 0.4) * ix, (r0 - 0.4) * iy, (c1 + 0.4) * ix, (r1 + 0.4) * iy)
}

function layout() {
  u = max(1, floor(GW / 40))
  ws = max(1, floor(GW / 50))                  // window size, cells
  street = GH - 1 - u
  nb = 0
  for (var layer = 0; layer < 2; layer++) {
    var c = -floor(hash(layer + 0.3) * 3 * u), k = layer * 50
    while (c < GW && nb < MAXB) {
      var w = (layer ? 4 : 3) * u + floor(hash(k * 1.31 + 0.2) * 5 * u)
      var hf = layer ? 0.18 + 0.47 * hash(k * 2.93 + 0.7) : 0.38 + 0.3 * hash(k * 2.93 + 0.7)
      bL[nb] = c
      bR[nb] = c + w - 1
      bT[nb] = floor(street - hf * street)
      bS[nb] = hash(k * 4.17 + 0.1) < 0.35 ? max(1, floor(w / 4)) : 0   // setback roof
      c += w + (hash(k * 3.7) < 0.3 ? u : 0)
      k++
      nb++
    }
    if (layer == 0) nfar = nb
  }
  tallest = nfar
  for (var b = nfar; b < nb; b++) if (bT[b] < bT[tallest] && bR[b] - bL[b] > 3 * u) tallest = b
}

var T = 0, night = 0, occ = 0.5

// Hours to "how deep into the night": 0 at dusk (and all day), 1 from 1 am to 4 am.
function nightOf(h) {
  if (h >= 17) return (h - 17) / 8
  if (h < 1) return (h + 7) / 8
  if (h < 4) return 1
  if (h < 7) return 1 - (h - 4) / 6
  return 0
}

export function beforeRender(delta) {
  T = mod(T + delta / 1000, 3600)
  var gw = gridWidth(), gh = gridHeight()
  GW = gw > 0 ? gw : 32
  GH = gh > 0 ? gh : 32
  if (GW != lastW || GH != lastH) { lastW = GW; lastH = GH; ix = 1 / max(GW - 1, 1); iy = 1 / max(GH - 1, 1); layout() }
  var h = hourSet > 0 ? hourSet : clockHour() + clockMinute() / 60
  night = nightOf(mod(h, 24))
  occ = 0.62 - 0.34 * night * night            // lights go out as the night deepens
}

function windows(b, far) {
  var pitch = 2 * ws, top = bT[b] + (bS[b] ? 2 * u + ws : ws)
  for (var r = top; r < street - ws; r += pitch) {
    for (var c = bL[b] + ws; c + ws <= bR[b]; c += pitch) {
      var id = b * 5.3 + c * 0.71, row = r * 0.37
      var e = floor((T + hash2(id, row) * flipS) / flipS)
      var on = hash2(id + e * 0.13, row + e * 1.7)
      if (on > occ) continue
      var kind = hash2(id, row + 9.1), v = far ? 0.22 : 0.6
      if (kind < partyPct / 100) hsv(T * 0.2 + id, 1, v)
      else if (kind < 0.22) hsv(0.6, 0.7, v * (0.5 + 0.5 * hash2(id, floor(T * 7) + row)))
      else hsv(0.08 + kind * 0.04, 0.55 + kind * 0.2, v * (0.75 + 0.25 * on / occ))
      cellRect(c, r, c + ws - 1, r + ws - 1)
    }
  }
}

export function renderBulk() {
  // sky: dusk blue over a rose horizon, sinking to navy over a sodium glow
  var n = night, v0 = 0.22 - 0.18 * n, v1 = 0.55 - 0.3 * n
  fillGradient(0.64, 0.8, v0, 1.0 + 0.07 * n, 0.65, v1, 2)
  var k
  for (k = 0; k < 4 + GW / 3; k++) {          // stars come out as it darkens
    var sc = floor(hash(k * 1.9) * GW), sr = floor(hash(k * 3.3 + 0.5) * GH * 0.45)
    hsv(0.62, 0.3, mix(v0, v1, sr * iy) + n * (0.2 + 0.15 * sin(T * (1 + hash(k)) * 2 + k)))
    cellRect(sc, sr, sc, sr)
  }
  // plane: one crossing every 40 s, strobe and red nav light
  var pp = mod(T, 40) / 18, ep = floor(T / 40)
  if (pp < 1) {
    var dir = hash(ep) < 0.5, pc = (dir ? pp : 1 - pp) * (GW + 6) - 3, pr = floor((0.08 + 0.2 * hash(ep + 0.5)) * GH)
    hsv(0, 1, 0.6)
    cellRect(pc, pr, pc, pr)
    if (frac(T) < 0.12) { hsv(0, 0, 1); cellRect(pc + (dir ? -u : u), pr, pc + (dir ? -u : u), pr) }
  }
  var b
  for (b = 0; b < nb; b++) {                  // towers: far row hazy, near row black
    var far = b < nfar
    hsv(0.66, 0.45, far ? 0.07 + 0.04 * (1 - n) : 0.015)
    cellRect(bL[b] + bS[b], bT[b], bR[b] - bS[b], street)
    if (bS[b]) cellRect(bL[b], bT[b] + 2 * u, bR[b], street)
    windows(b, far)
  }
  // beacon and its mast on the tallest near tower
  var mc = floor((bL[tallest] + bR[tallest]) / 2), mt = bT[tallest] - 2 * u
  hsv(0.66, 0.3, 0.06)
  cellRect(mc, mt, mc, bT[tallest])
  var bl = wave(T * 0.6)
  hsv(0, 1, 0.15 + 0.85 * bl * bl * bl)
  cellRect(mc, mt, mc, mt)
  // street: a dark kerb with cars passing both ways
  hsv(0.08, 0.5, 0.04)
  cellRect(0, street + 1, GW - 1, GH - 1)
  for (k = 0; k < cars; k++) {
    var right = k % 2 == 0, sp = 3 + hash(k * 5.5) * 5
    var cc = mod(hash(k * 2.2) * GW * 2 + T * sp * u, GW * 2) - GW / 2
    if (!right) cc = GW - 1 - cc
    var cr = right ? GH - 1 : street + 1
    hsv(right ? 0 : 0.12, right ? 1 : 0.25, right ? 0.6 : 0.9)
    cellRect(cc, cr, cc + u, cr)
  }
}
