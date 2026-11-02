// name: Copper Bars
// Curated original (hand-written showcase of the Luxel language/builtins).
// The demoscene raster classic: glossy chrome tubes swinging on sines, drawn
// back to front so they pass over and under each other, each with a dark rim,
// a lit body and a specular streak (OKLCH lightness ramps, one fillRect per
// bar row). Kefrens style stamps a vertical bar per row into a line buffer
// that is never cleared (one blit a row), so the bars pile into a writhing
// staircase. On a strip the tubes become metallic droplets sliding past
// each other (gradientRange spans).

var style = 0
// 0 copper bars, 1 Kefrens staircase
//# min=0 max=1 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 1) }

var nBars = 7
//# min=2 max=12 step=1 default=7
export function sliderBars(v) { nBars = clamp(floor(v), 2, 12) }

var swingS = 5
// Seconds for one full swing.
//# min=1 max=20 step=0.5 default=5
export function sliderSwingSeconds(v) { swingS = clamp(v, 1, 20) }

var thick = 0.14
// Bar thickness as a share of the panel height (or strip length).
//# min=4 max=30 step=1 default=14
export function sliderThicknessPercent(v) { thick = clamp(v, 4, 30) / 100 }

var spread = 22
// Phase gap between neighbouring bars (Kefrens: 22 = one wave per panel).
//# min=5 max=90 step=1 default=22
export function sliderSpreadDegrees(v) { spread = clamp(v, 5, 90) }

var pal = 0
// 0 rainbow, 1 copper, 2 ice
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { pal = clamp(floor(v), 0, 2) }

var ph = 0, drift = 0
export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  ph = mod(ph + dt / swingS, 1)
  drift = mod(drift + dt / 40, 1)
}

function hueOf(f) {
  if (pal == 1) return 0.03 + 0.09 * triangle(f)
  if (pal == 2) return 0.5 + 0.14 * triangle(f)
  return f + drift
}

// chrome brush at offset d (-1 top/left edge .. 1 bottom/right), depth k
function chrome(d, h, k) {
  var body = 1 - d * d
  var spec = max(0, 1 - abs(d + 0.35) * 4)
  spec *= spec
  oklch((0.18 + 0.55 * body + 0.3 * spec) * k, (0.17 - 0.15 * spec) * k, h)
}

// ---- grid slices: exact row/column coordinates, or uniform bands ----
var rowY = array(128), colX = array(128), RN = 0, CN = 0, hw = 0, lastW = -1, lastH = -1
function slices() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw == lastW && gh == lastH) return
  lastW = gw; lastH = gh
  if (gw > 0 && gh > 0) {
    RN = min(gh, 128); CN = min(gw, 128); hw = 0
    for (var r = 0; r < RN; r++) rowY[r] = pixelCoord(gridIndex(0, r), 1)
    for (var c = 0; c < CN; c++) colX[c] = pixelCoord(gridIndex(c, 0), 0)
  } else {
    RN = 48; CN = 48; hw = 0.5 / 48
    for (var i = 0; i < 48; i++) { rowY[i] = (i + 0.5) / 48; colX[i] = rowY[i] }
  }
}

// ---- copper: horizontal tubes, z-sorted ----
var ord = array(12), zz = array(12), yy = array(12)
function copper() {
  fillGradient(0.72, 0.7, 0.12, 0.66, 0.9, 0.03, 2)     // deep indigo dusk
  var amp = 0.5 - thick * 0.55
  for (var k = 0; k < nBars; k++) {
    var a = PI2 * (ph + k * spread / 360)
    yy[k] = 0.5 + amp * sin(a); zz[k] = cos(a)
    var j = k
    while (j > 0 && zz[ord[j - 1]] > zz[k]) { ord[j] = ord[j - 1]; j-- }
    ord[j] = k
  }
  for (var i = 0; i < nBars; i++) {
    var k = ord[i], z = (zz[k] + 1) * 0.5
    var h = thick * (0.5 + 0.2 * z), dk = 0.6 + 0.4 * z, hu = hueOf(k / nBars)
    var r0 = max(0, floor((yy[k] - h) * RN - 1)), r1 = min(RN - 1, ceil((yy[k] + h) * RN + 1))
    for (var r = r0; r <= r1; r++) {
      var d = (rowY[r] - yy[k]) / h
      if (abs(d) > 1) continue
      chrome(d, hu, dk)
      fillRect(-1, rowY[r] - hw, 2, rowY[r] + hw)
    }
  }
}

// ---- Kefrens: each row stamps a vertical tube into a line buffer that is
// never cleared, then the buffer is blitted as that row (keyed, mode 3) ----
var lh = array(128), ls = array(128), lv = array(128)
function kefrens() {
  fillGradient(0.72, 0.7, 0.12, 0.66, 0.9, 0.03, 2)
  var bw = max(3, round(CN * thick * 0.7)), amp = 0.5 - bw / CN * 0.6
  feedback(lv, 0)
  for (var r = 0; r < RN; r++) {
    var f = r / RN
    var x = 0.5 + amp * (0.65 * sin(PI2 * (ph + f * spread / 22)) + 0.35 * sin(PI2 * (2 * drift + f * 1.7)))
    var c0 = round(x * CN - bw / 2), hu = hueOf(f * nBars / 6)
    for (var c = max(c0, 0); c < min(c0 + bw, CN); c++) {
      var d = ((c - c0) + 0.5) / bw * 2 - 1
      if (hw > 0) {   // no grid (sparse map): paint the column down instead
        chrome(d, hu, 1)
        fillRect(colX[c] - hw, rowY[r] - hw, colX[c] + hw, 2)
      } else {
        var spec = max(0, 1 - abs(d + 0.35) * 4)
        lh[c] = hu; ls[c] = 1 - 0.75 * spec * spec
        lv[c] = 0.18 + 0.6 * (1 - d * d) + 0.3 * spec
      }
    }
    if (hw == 0) blit(lh, ls, lv, CN, 1, 0, r, 3)
  }
}

// ---- strip: chrome droplets as four gradientRange spans each ----
function strip() {
  var n = pixelCount
  hsv(0.68, 0.8, 0.06)
  fill()
  var w = max(2, n * thick * 0.5), amp = n * 0.5 - w
  for (var k = 0; k < nBars; k++) {
    var a = PI2 * (ph + k * spread / 360)
    zz[k] = cos(a); yy[k] = n * 0.5 + amp * sin(a)
    var j = k
    while (j > 0 && zz[ord[j - 1]] > zz[k]) { ord[j] = ord[j - 1]; j-- }
    ord[j] = k
  }
  for (var i = 0; i < nBars; i++) {
    var k = ord[i], dk = 0.55 + 0.45 * (zz[k] + 1) * 0.5, p = yy[k], hu = hueOf(k / nBars)
    var ww = w * (0.8 + 0.2 * dk)
    // knock out what is behind, then add the spans so shared ends sum whole
    hsv(0, 0, 0)
    fillRangeAA(p - ww, p + ww, 0)
    gradientRange(p - ww, p - ww * 0.35, hu, 1, 0.12 * dk, hu, 0.9, 0.7 * dk, 1)
    gradientRange(p - ww * 0.35, p, hu, 0.9, 0.7 * dk, hu, 0.2, dk, 1)
    gradientRange(p, p + ww * 0.5, hu, 0.2, dk, hu, 1, 0.5 * dk, 1)
    gradientRange(p + ww * 0.5, p + ww, hu, 1, 0.5 * dk, hu, 1, 0.1 * dk, 1)
  }
}

export function renderBulk() {
  if (!has2DMap()) { strip(); return }
  slices()
  if (style == 1) kefrens()
  else copper()
}

// Keeps a mapless strip mapless (renderBulk always wins on Luxel).
export function render(index) { hsv(0, 0, 0) }
