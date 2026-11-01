// name: Shadebobs
// Curated original (hand-written showcase of the Luxel language/builtins).
// The demoscene classic: invisible brushes trace Lissajous paths and ADD heat where
// they pass; heat that overflows wraps back to the bottom of the palette, so the
// panel fills with crisp psychedelic contour bands that build up and slowly dissolve.
// One accumulation canvas (panel-sized up to 64x64): soft disc stamps, one
// arrayScale for the decay, one paintCanvas through the palette.

var CAP = 64, MAXB = 6

var bobs = 4
//# min=1 max=6 step=1 default=4
export function sliderBobs(v) { bobs = clamp(floor(v), 1, MAXB) }

// seconds for untouched heat to fall to about a third
var decaySec = 12
//# min=2 max=60 step=1 default=12
export function sliderDecaySeconds(v) { decaySec = clamp(v, 2, 60) }

// brush radius, percent of the panel width
var sizePct = 13
//# min=5 max=30 step=1 default=13
export function sliderSizePercent(v) { sizePct = clamp(v, 5, 30) }

var speed = 1
//# min=0.2 max=3 step=0.1 default=1
export function sliderSpeed(v) { speed = clamp(v, 0.2, 3) }

// 0 neon, 1 fire, 2 ocean
var palMode = 0
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palMode = clamp(floor(v), 0, 2); setPal() }

var acc = array(CAP * CAP)
var pal = array(24)
setPalette(pal)
var fx = [1.0, 1.31, 0.73, 1.57, 0.89, 1.13], fy = [1.42, 0.67, 1.19, 0.81, 1.53, 0.97]
var px = [0, 1.1, 2.3, 3.7, 4.4, 5.6], py = [0.7, 2.9, 1.3, 4.9, 0.2, 3.3]
var CW = 0, CH = 0, T = 0

function st(j, pos, r, g, b) { pal[j] = pos; pal[j + 1] = r; pal[j + 2] = g; pal[j + 3] = b }
function setPal() {
  if (palMode == 0) {
    st(0, 0, 0, 0, 0.05); st(4, 0.25, 0.1, 0.05, 0.7); st(8, 0.5, 0.8, 0.05, 0.75)
    st(12, 0.72, 1, 0.45, 0.1); st(16, 0.9, 1, 0.95, 0.4); st(20, 1, 1, 1, 0.9)
  } else if (palMode == 1) {
    st(0, 0, 0.02, 0, 0); st(4, 0.3, 0.6, 0.02, 0); st(8, 0.55, 1, 0.3, 0)
    st(12, 0.75, 1, 0.7, 0.05); st(16, 0.9, 1, 1, 0.5); st(20, 1, 1, 1, 1)
  } else {
    st(0, 0, 0, 0.02, 0.06); st(4, 0.3, 0, 0.2, 0.5); st(8, 0.55, 0, 0.6, 0.7)
    st(12, 0.75, 0.3, 0.9, 0.7); st(16, 0.9, 0.85, 1, 0.9); st(20, 1, 1, 1, 1)
  }
}
setPal()

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  var gw = gridWidth(), gh = gridHeight()
  var w = gw > 0 ? min(gw, CAP) : 32, h = gh > 0 ? min(gh, CAP) : 32
  if (w != CW || h != CH) { CW = w; CH = h; feedback(acc, 0) }
  setBlur(gw > CW ? 0.35 : 0)          // soften the 2x upscale on big panels
  T += dt * speed * 0.55
  if (T > 6000) T -= 6000

  arrayScale(acc, exp(-dt / decaySec))
  var R = sizePct * 0.01 * CW, R2 = R * R, heat = dt * speed * 0.9
  for (var b = 0; b < bobs; b++) {
    var cx = (0.5 + 0.42 * sin(T * fx[b] + px[b])) * CW
    var cy = (0.5 + 0.42 * sin(T * fy[b] + py[b])) * CH
    var r0 = max(0, ceil(cy - R)), r1 = min(CH - 1, floor(cy + R))
    var c0 = max(0, ceil(cx - R)), c1 = min(CW - 1, floor(cx + R))
    for (var r = r0; r <= r1; r++) {
      var dy = r + 0.5 - cy, row = r * CW
      for (var c = c0; c <= c1; c++) {
        var dx = c + 0.5 - cx, q = 1 - (dx * dx + dy * dy) / R2
        if (q <= 0) continue
        var v = acc[row + c] + heat * q
        acc[row + c] = v >= 1 ? v - 1 : v      // the wrap IS the effect
      }
    }
  }
}

export function renderBulk() {
  paintCanvas(acc, CW, CH)
}
