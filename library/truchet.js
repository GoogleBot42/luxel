// name: Truchet Weave
// Curated original (hand-written showcase of the Luxel language/builtins).
// Quarter-circle tiles join into endless ribbons and loops over a two-tone ground;
// every so often a tile spins 90 degrees with a springy overshoot and re-routes
// them, and colour pulses travel ALONG every ribbon (flow direction alternates with
// the checkerboard, which keeps it continuous across tiles). 10 PRINT swaps the
// arcs for the Commodore maze's diagonal slashes. Only four tile images (two
// orientations x two parities) are drawn per frame and stamped with blit.

var MAXT = 16, MAXG = 32, MAXF = 16

// 0 arcs, 1 10 PRINT slashes
var mode = 0
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1) }

var tilePx = 10
//# min=4 max=16 step=1 default=10
export function sliderTilePixels(v) { tilePx = clamp(floor(v), 4, MAXT) }

// tile spins started per second, across the whole panel
var flipRate = 2
//# min=0 max=10 step=0.1 default=2
export function sliderFlipsPerSecond(v) { flipRate = clamp(v, 0, 10) }

// ribbon width as a percentage of the tile
var widthPct = 30
//# min=10 max=50 step=1 default=30
export function sliderWidthPercent(v) { widthPct = clamp(v, 10, 50) }

// seconds for a colour pulse to cross one tile
var flowSec = 1.2
//# min=0.3 max=5 step=0.1 default=1.2
export function sliderFlowSeconds(v) { flowSec = clamp(v, 0.3, 5) }

var TT = MAXT * MAXT
var tH = [array(TT), array(TT), array(TT), array(TT), array(TT)]
var tS = [array(TT), array(TT), array(TT), array(TT), array(TT)]
var tV = [array(TT), array(TT), array(TT), array(TT), array(TT)]
var orient = array(MAXG * MAXG)
var fCell = array(MAXF), fProg = array(MAXF)
var T = 0, cols = 0, rows = 0, flow = 0, hue0 = 0, spawn = 0

function reset() {
  for (var r = 0; r < rows; r++) for (var c = 0; c < cols; c++)
    orient[r * cols + c] = hash2(c * 7 + 3, r * 13 + 5) < 0.5 ? 0 : 1
  for (var i = 0; i < MAXF; i++) fProg[i] = -1
}

// one tile image into slot k: orientation o, checker parity q, turned by ang
function drawTile(k, o, q, ang) {
  var H = tH[k], S = tS[k], V = tV[k], w = widthPct * 0.005
  var ca = cos(ang), sa = sin(ang)
  for (var py = 0; py < T; py++) for (var px = 0; px < T; px++) {
    var a = (px + 0.5) / T - 0.5, b = (py + 0.5) / T - 0.5
    var u = a * ca + b * sa + 0.5, v = b * ca - a * sa + 0.5
    var e, s, region
    if (mode == 0) {
      var kx = o ? (u > v ? 1 : 0) : (u + v < 1 ? 0 : 1)
      var ky = o ? 1 - kx : kx
      var dx = abs(u - kx), dy = abs(v - ky), d = hypot(dx, dy)
      e = abs(d - 0.5) / w
      s = dx / (dx + dy + 0.0001)                  // 0 on a side edge, 1 on top/bottom
      region = (q + o + (d < 0.5 ? 1 : 0)) & 1
    } else {
      e = abs(o ? u + v - 1 : u - v) * 0.7071 / w
      s = o ? (u - v + 1) * 0.5 : (u + v) * 0.5
      region = 0
    }
    if (q) s = 1 - s                               // odd cells flow the other way
    var i = py * T + px
    if (e < 1) {
      var ph = s - flow, tube = 1 - e * e
      H[i] = mode ? 0.58 : hue0 + 0.22 * triangle(ph)
      S[i] = mode ? 0.45 - 0.3 * wave(ph) : 1 - 0.55 * pow(wave(ph), 6)
      V[i] = tube * (0.45 + 0.55 * wave(ph))
    } else {
      H[i] = mode ? 0.66 : hue0 + 0.5 + region * 0.12
      S[i] = mode ? 0.85 : 0.8
      V[i] = (mode ? 0.28 : 0.1 + region * 0.2) * smoothstep(1, 1.8, e)
    }
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  var gw = gridWidth(), gh = gridHeight()
  var c = gw > 0 ? min(ceil(gw / tilePx), MAXG) : 0, r = gh > 0 ? min(ceil(gh / tilePx), MAXG) : 0
  if (tilePx != T || c != cols || r != rows) { T = tilePx; cols = c; rows = r; reset() }
  flow = mod(flow + dt / flowSec, 1)
  hue0 = mod(hue0 + dt / 40, 1)

  spawn += dt * flipRate
  while (spawn >= 1) {
    spawn -= 1
    for (var i = 0; i < MAXF; i++) if (fProg[i] < 0) {
      fCell[i] = floor(random(cols * rows)); fProg[i] = 0
      break
    }
  }
  for (i = 0; i < MAXF; i++) if (fProg[i] >= 0) {
    fProg[i] += dt / 0.7
    if (fProg[i] >= 1) { orient[fCell[i]] = 1 - orient[fCell[i]]; fProg[i] = -1 }
  }
}

export function renderBulk() {
  if (cols == 0) return
  drawTile(0, 0, 0, 0); drawTile(1, 0, 1, 0); drawTile(2, 1, 0, 0); drawTile(3, 1, 1, 0)
  for (var r = 0; r < rows; r++) for (var c = 0; c < cols; c++) {
    var k = orient[r * cols + c] * 2 + ((c + r) & 1)
    blit(tH[k], tS[k], tV[k], T, T, c * T, r * T, 0)
  }
  // spinning tiles on top, each its own turned image
  for (var i = 0; i < MAXF; i++) if (fProg[i] >= 0) {
    var cell = fCell[i]
    c = mod(cell, cols); r = floor(cell / cols)
    drawTile(4, orient[cell], (c + r) & 1, easeOutBack(fProg[i]) * PI * 0.5)
    blit(tH[4], tS[4], tV[4], T, T, c * T, r * T, 0)
  }
}
