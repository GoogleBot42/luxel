// name: Marquee
// Curated original (hand-written showcase of the Luxel language/builtins); folds in the
// clean-room idea of docs/pattern-specs/three-red-pixels-mathy.md — reimagined, not a replica.
// A theatre marquee: discrete incandescent bulbs every few pixels, wired on three
// circuits that a chaser switches in turn. Each circuit's filament warms up and cools
// down on its own time constant and reddens as it dims, so the chase flows instead of
// blinking. Strip: one bulb row. Panel: nested bulb borders, alternate rings chasing
// opposite ways (bulbs scale up on big panels). renderBulk: a halo and a core per bulb.

var spacing = 3         // px from one bulb to the next
//# min=2 max=8 step=1 default=3
export function sliderSpacingPixels(v) { spacing = clamp(floor(v), 2, 8) }

var bulbPx = 1          // lit pixels per bulb (kept below the spacing)
//# min=1 max=4 step=1 default=1
export function sliderBulbPixels(v) { bulbPx = clamp(floor(v), 1, 4) }

var stepS = 0.15        // seconds between chaser steps
//# min=0.05 max=1 step=0.01 default=0.15
export function sliderStepSeconds(v) { stepS = clamp(v, 0.05, 1) }

var filamentMs = 90     // filament warm-up time constant (cool-down is 1.6x)
//# min=20 max=300 step=5 default=90
export function sliderFilamentMs(v) { filamentMs = clamp(v, 20, 300) }

var style = 0           // 0: one circuit lit (running light) | 1: one circuit dark
//# min=0 max=1 step=1 default=0
export function sliderChaseStyle(v) { style = clamp(floor(v), 0, 1) }

var tint = 0            // 0: incandescent at Kelvin | 1: the bulb picker (coloured glass)
//# min=0 max=1 step=1 default=0
export function sliderTint(v) { tint = clamp(floor(v), 0, 1) }

var kelvin = 2400
//# min=2000 max=3500 step=50 default=2400
export function sliderKelvin(v) { kelvin = clamp(v, 2000, 3500) }

var dir = 1
export function toggleReverse(on) { dir = on ? -1 : 1 } //# default=0

// Coloured-glass bulbs (Tint 1). The picker's brightness is ignored.
var glR = 1, glG = 0.08, glB = 0.02, tmp = array(3)
export function hsvPickerBulbColor(h, s, v) { hsv2rgb(h, s, 1, tmp); glR = tmp[0]; glG = tmp[1]; glB = tmp[2] }

// Background underglow on the sign: here the picker's brightness IS the glow level.
var bgR = 0, bgG = 0, bgB = 0
export function hsvPickerBackground(h, s, v) { hsv2rgb(h, s, v * 0.25, tmp); bgR = tmp[0]; bgG = tmp[1]; bgB = tmp[2] }

// Incandescent colour at temperature k (K), into cR/cG/cB (a log fit of blackbody light).
var cR = 1, cG = 0.6, cB = 0.2
function blackbody(k) {
  var t = k / 100
  cR = 1
  cG = clamp((99.47 * log(t) - 161.12) / 255, 0, 1)
  cB = t <= 19.5 ? 0 : clamp((138.52 * log(t - 10) - 305.04) / 255, 0, 1)
}

var lvl = array(3)      // filament level per circuit
var cirR = array(3), cirG = array(3), cirB = array(3)
var head = 0, acc = 0

export function beforeRender(delta) {
  acc += delta
  if (acc > stepS * 10000) acc = 0
  while (acc >= stepS * 1000) { acc -= stepS * 1000; head = mod(head + dir, 3) }
  var kUp = 1 - exp(-delta / filamentMs), kDn = 1 - exp(-delta / (filamentMs * 1.6))
  for (var c = 0; c < 3; c++) {
    var on = (c == head) != (style == 1)
    lvl[c] += ((on ? 1 : 0) - lvl[c]) * (on ? kUp : kDn)
    var L = lvl[c]
    if (tint == 0) blackbody(1200 + (kelvin - 1200) * sqrt(L))   // a dim filament glows red
    else { cR = glR; cG = glG; cB = glB }
    cirR[c] = cR * L; cirG[c] = cG * L; cirB[c] = cB * L
  }
}

// A mapless strip stays a strip; Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

function strip() {
  var bp = min(bulbPx, spacing - 1), n = ceil(pixelCount / spacing)
  for (var k = 0; k < n; k++) {
    var c = k % 3, a = k * spacing
    rgb(cirR[c] * 0.16, cirG[c] * 0.16, cirB[c] * 0.16)  // light on the sign around the bulb
    fillRangeAA(a - 1, a + bp + 1, 2)
    rgb(cirR[c], cirG[c], cirB[c])
    fillRange(a, a + bp)
  }
}

// Panel: a sign of nested bulb borders, each ring chasing the opposite way to the
// one outside it. Every ring holds a multiple of three bulbs so the circuits close.
var bx = 0, by = 0
function perim(s, x0, y0, w, h) {
  if (s < w) { bx = x0 + s; by = y0 }
  else if (s < w + h) { bx = x0 + w; by = y0 + s - w }
  else if (s < 2 * w + h) { bx = x0 + w - (s - w - h); by = y0 + h }
  else { bx = x0; by = y0 + h - (s - 2 * w - h) }
  bx = floor(bx); by = floor(by)
}
function panel() {
  var gw = gridWidth(), gh = gridHeight(), u = max(1, floor(min(gw, gh) / 32))  // big panels: bigger bulbs
  var sp = spacing * u, bp = min(bulbPx, spacing - 1) * u
  var pitch = sp * 2, hr = (bp * 0.5 + 1.4 * u) / max(gw - 1, 1), ring = 0
  for (var d = 0; d * 2 + 2 < min(gw, gh); d += pitch) {
    var w = gw - bp - 2 * d, h = gh - bp - 2 * d, per = 2 * (w + h)
    var nb = max(3, round(per / sp / 3) * 3), stp = per / nb
    for (var k = 0; k < nb; k++) {
      var c = ring % 2 ? (3 - k % 3) % 3 : k % 3
      perim(k * stp, d, d, w, h)
      var i = gridIndex(bx, by)
      rgb(cirR[c] * 0.4, cirG[c] * 0.4, cirB[c] * 0.4)
      splat(pixelCoord(i, 0) + (bp - 1) * 0.5 / max(gw - 1, 1), pixelCoord(i, 1) + (bp - 1) * 0.5 / max(gh - 1, 1), hr, 2)
      rgb(cirR[c], cirG[c], cirB[c])
      for (var yy = 0; yy < bp; yy++) for (var xx = 0; xx < bp; xx++) setPixel(gridIndex(bx + xx, by + yy))
    }
    ring++
  }
}

export function renderBulk() {
  rgb(bgR, bgG, bgB)
  fill()
  if (has2DMap() && gridWidth() > 0) panel()
  else strip()
}
