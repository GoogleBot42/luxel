// name: Pulse Train
// Clean-room reimplementation from the prose specs docs/pattern-specs/color-pick-fade.md,
// color-fade-pulse.md — reimagined, not a replica.
// Evenly spaced soft pulses glide along the strip: one picked colour for calm
// mood light, a scrolling rainbow seen through the pulses, or that rainbow with
// its saturation washing to white and back. On a panel the train runs across,
// down, or outward in rings. Strip: each pulse is a few anti-aliased
// gradientRange spans. Panel: the profile along the axis is loaded into a live
// palette once per frame and a fixed axis canvas is painted with it.

var pickH = 0.05, pickS = 1
// Pulse colour for Color 0 (hue and saturation; the picker's brightness is ignored).
export function hsvPickerColor(h, s, v) { pickH = h; pickS = s }

var colorMode = 0
// 0 the picker's colour, 1 scrolling rainbow, 2 rainbow washing to white
//# min=0 max=2 step=1 default=0
export function sliderColor(v) { colorMode = clamp(floor(v), 0, 2) }

var pulses = 4
//# min=1 max=12 step=1 default=4
export function sliderPulses(v) { pulses = clamp(floor(v), 1, 12) }

var widthPct = 16
// Width of a pulse at half brightness, in percent of the spacing between pulses.
//# min=2 max=100 step=1 default=16
export function sliderPulseWidth(v) { widthPct = clamp(v, 2, 100) }

var lapS = 24
// Seconds for a pulse to travel the whole length.
//# min=2 max=120 step=1 default=24
export function sliderLapSeconds(v) { lapS = clamp(v, 2, 120) }

var axis = 2
// Panel only: 0 across, 1 down, 2 outward rings.
//# min=0 max=2 step=1 default=2
export function sliderAxis(v) { axis = clamp(floor(v), 0, 2) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var lap = 0, hueT = 0, satT = 0, wk = 1
export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  lap = mod(lap + dt / lapS, 1)
  hueT = mod(hueT + dt / 6, 1)
  satT = mod(satT + dt / 5, 1)
  wk = 16 / widthPct
}

// colour at axis position u (0..1) -> ch, cs
var ch = 0, cs = 0
function tint(u) {
  if (colorMode == 0) { ch = pickH; cs = pickS; return }
  ch = 2 * u - hueT
  cs = colorMode == 2 ? 0.55 + 0.45 * sin((satT + u) * PI2) : 1
}

// brightness at distance d from a crest, in spacings (0..0.5)
function bump(d) {
  var b = max(0, 1 - 2 * d * wk)
  b = b * b
  return b * b
}

// ---- strip: each pulse as four spans a side ----
function strip() {
  clear()
  var n = pixelCount, sp = n / pulses
  var dm = min(0.5, 0.5 / wk)
  for (var j = 0; j < pulses; j++) {
    var c = mod((j + 0.5) / pulses + lap, 1) * n
    for (var k = 0; k < 4; k++) {
      var d0 = dm * (k == 0 ? 0 : k == 1 ? 0.2 : k == 2 ? 0.4 : 0.6)
      var d1 = dm * (k == 0 ? 0.2 : k == 1 ? 0.4 : k == 2 ? 0.6 : 1)
      var v0 = bump(d0), v1 = bump(d1)
      for (var side = -1; side <= 1; side += 2) {
        var a = c + side * d0 * sp, b = c + side * d1 * sp
        tint(a / n)
        var h0 = ch, s0 = cs
        tint(b / n)
        gradientRange(a, b, h0, s0, v0, ch, cs, v1, 1)
        // a pulse running off one end comes back on at the other
        if (min(a, b) < 0) gradientRange(a + n, b + n, h0, s0, v0, ch, cs, v1, 1)
        if (max(a, b) > n) gradientRange(a - n, b - n, h0, s0, v0, ch, cs, v1, 1)
      }
    }
  }
}

// ---- panel: the profile along u as a live palette over an axis canvas ----
var NS = 128
var pal = array(NS * 4)
var rgb3 = array(3)
setPalette(pal)
var cvs = array(1024)
var cw = 0, ch2 = 0, builtAxis = -1

function buildCanvas() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw < 1) { gw = 32; gh = 32 }
  if (axis == 0) { cw = min(gw, 64); ch2 = 1 }
  else if (axis == 1) { cw = 1; ch2 = min(gh, 64) }
  else { cw = min(gw, 32); ch2 = min(gh, 32) }
  for (var r = 0; r < ch2; r++) {
    for (var c = 0; c < cw; c++) {
      var x = (c + 0.5) / cw, y = (r + 0.5) / ch2
      var u = axis == 0 ? x : axis == 1 ? y : sqrt((x - 0.5) * (x - 0.5) + (y - 0.5) * (y - 0.5)) * 1.414
      cvs[r * cw + c] = min(u, 0.999)
    }
  }
  builtAxis = axis
}

function panel() {
  if (builtAxis != axis) buildCanvas()
  for (var k = 0; k < NS; k++) {
    var u = k / (NS - 1)
    var q = mod(u * pulses - lap * pulses, 1)
    tint(u)
    hsv2rgb(ch, cs, bump(abs(q - 0.5)), rgb3)
    pal[k * 4] = u
    pal[k * 4 + 1] = rgb3[0]
    pal[k * 4 + 2] = rgb3[1]
    pal[k * 4 + 3] = rgb3[2]
  }
  paintCanvas(cvs, cw, ch2, 1, axis == 2 ? 1 : 0)   // the 1-D axes are already pixel-sized
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
