// name: DNA Helix
// Curated original (hand-written showcase of the Luxel language/builtins); the
// single-ribbon look is a clean-room reimagining of docs/pattern-specs/sinus.md.
// Sine ribbons wound round a gently snaking axis: one is a lone ribbon, two a
// double helix with base-pair rungs, more a braid. Each ribbon is a polyline of
// drawLine capsules (max blend) whose width and brightness follow its depth, so
// the back strands sink; colours run in OKLab from Strand A to Strand B. On a
// strip the helix is seen end-on: a twisted rope showing whichever strand is in front.

var ribbons = 2
var turns = 2
var spinS = 4
var vertical = 0
var hA = 0.5, sA = 0.85, hB = 0.88, sB = 0.85

//# min=1 max=6 step=1 default=2
export function sliderRibbons(v) { ribbons = clamp(floor(v), 1, 6) }
// Helix turns across the panel.
//# min=0.5 max=6 step=0.5 default=2
export function sliderTurns(v) { turns = clamp(v, 0.5, 6) }
// Seconds per full rotation of the helix.
//# min=1 max=60 step=0.5 default=4
export function sliderRotationS(v) { spinS = clamp(v, 1, 60) }
export function toggleVertical(v) { vertical = v }
// Ribbon colours (hue and saturation; the pickers' brightness is ignored).
export function hsvPickerStrandA(h, s, v) { hA = h; sA = s }
export function hsvPickerStrandB(h, s, v) { hB = h; sB = s }

var cR = array(6), cG = array(6), cB = array(6)
var ca = array(3), cb = array(3), cm = array(3)
var ph = 0, tw = 0

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  ph = mod(ph + dt / spinS, 1)
  tw = mod(tw + dt / 37, 1)
  hsv2rgb(hA, sA, 1, ca)
  hsv2rgb(hB, sB, 1, cb)
  for (var k = 0; k < ribbons; k++) {
    mixColors(ca[0], ca[1], ca[2], cb[0], cb[1], cb[2], ribbons > 1 ? k / (ribbons - 1) : 0, cm)
    cR[k] = cm[0]; cG[k] = cm[1]; cB[k] = cm[2]
  }
}

function seg(a0, b0, a1, b1, w) {
  if (vertical) drawLine(b0, a0, b1, a1, w, 2)
  else drawLine(a0, b0, a1, b1, w, 2)
}

export function renderBulk() {
  clear()
  if (!has2DMap() && !has3DMap()) { rope(); return }
  var n = vertical ? gridHeight() : gridWidth()
  if (n == 0) n = 32
  var steps = clamp(ceil(turns * 14), 16, max(n, 16))
  var amp = 0.3 + 0.05 * sin(tw * PI2 * 3)
  var w0 = 1.6 / max(n, 16)
  for (var k = 0; k < ribbons; k++) {
    var off = k / ribbons, pa = 0, pb = 0, pp = 0
    for (var j = 0; j <= steps; j++) {
      var a = j / steps
      var p = a * turns + ph + off + 0.12 * sin((a * 0.8 + tw) * PI2)
      var d = cos(p * PI2)                       // depth: 1 = front
      var mid = 0.5 + 0.06 * sin((a * 0.7 + tw * 2) * PI2)
      var b = mid + amp * sin(p * PI2)
      var lv = 0.3 + 0.7 * (0.5 + 0.5 * d)
      rgb(cR[k] * lv, cG[k] * lv, cB[k] * lv)
      if (j > 0) seg(pa, pb, a, b, w0 * (1.6 + 0.9 * d))
      // a double helix gets base pairs: a two-tone rung every sixth of a turn
      if (ribbons == 2 && k == 0 && j > 0 && floor(p * 6) != floor(pp * 6)) {
        rgb(cR[0] * 0.75 * lv, cG[0] * 0.75 * lv, cB[0] * 0.75 * lv)
        seg(a, b, a, mid, w0 * 1.2)
        rgb(cR[1] * 0.75 * (1.3 - lv), cG[1] * 0.75 * (1.3 - lv), cB[1] * 0.75 * (1.3 - lv))
        seg(a, mid, a, 2 * mid - b, w0 * 1.2)
      }
      pa = a; pb = b; pp = p
    }
  }
}

// end-on view for strips: the front-most ribbon's colour, darkening at each crossing
function rope() {
  for (var i = 0; i < pixelCount; i++) {
    var a = i / pixelCount, best = -2, kb = 0
    for (var k = 0; k < ribbons; k++) {
      var d = cos((a * turns * 4 + ph + k / ribbons) * PI2)
      if (d > best) { best = d; kb = k }
    }
    var lv = ribbons > 1 ? best * best : 0.25 + 0.75 * (0.5 + 0.5 * best)
    rgb(cR[kb] * lv, cG[kb] * lv, cB[kb] * lv)
    setPixel(i)
  }
}

// Mapless strips stay strips: this export suppresses the default grid.
export function render(index) { hsv(0, 0, 0) }
