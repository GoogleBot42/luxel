// name: Bulk Sprite Scroll 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Grid-space drawing: an 8x8 sprite held in three parallel canvas arrays
// (hue / saturation / value, row-major) is pasted onto the matrix with one
// `blit()` per frame, scrolling by whole cells. Blit mode 3 is KEYED — a
// source cell whose value is 0 is transparent — so the sprite is drawn once
// with a hole around it and the background shows through; the eyes and mouth
// are dim rather than black for exactly that reason (a v of 0 there would
// punch holes right through the face).
//
// `blit` is grid-space: it needs a real w x h matrix. On a fixture that is
// not a grid `gridWidth()` reports 0 and the blit is a no-op, so the pattern
// degrades to its background wash rather than erroring.

const SW = 8
const SH = 8

var speed = 1
var scroll = 0
var bgHue = 0

var sprH = array(SW * SH)
var sprS = array(SW * SH)
var sprV = array(SW * SH)

// Draw the sprite once, at init: a round face, cut out of the 8x8 square.
var r
var c
for (r = 0; r < SH; r++) {
  for (c = 0; c < SW; c++) {
    if (hypot(c - 3.5, r - 3.5) > 3.7) continue   // corners stay v = 0
    sprH[r * SW + c] = 0.13
    sprS[r * SW + c] = 1
    sprV[r * SW + c] = 1
  }
}
sprV[2 * SW + 2] = 0.08          // eyes
sprV[2 * SW + 5] = 0.08
sprV[5 * SW + 1] = 0.08          // mouth
sprV[6 * SW + 2] = 0.08
sprV[6 * SW + 3] = 0.08
sprV[6 * SW + 4] = 0.08
sprV[6 * SW + 5] = 0.08
sprV[5 * SW + 6] = 0.08

export function beforeRender(delta) {
  var dt = delta * 0.001
  scroll = mod(scroll + dt * 0.15 * speed, 1)
  bgHue = mod(bgHue + dt * 0.02, 1)
}

export function renderFrame() {
  // dim two-tone wash along mapped x (axis 1), so the sprite has something
  // to be transparent over
  fillGradient(bgHue, 0.9, 0.06, bgHue + 0.45, 0.9, 0.16, 1)

  var gw = gridWidth()
  if (gw == 0) return            // not a matrix — the wash is the pattern
  var gh = gridHeight()

  // travel from just off the right edge to just off the left one
  var col = gw - scroll * (gw + SW)
  var row = (gh - SH) / 2 + 1.2 * wave(scroll * 4) - 0.6
  blit(sprH, sprS, sprV, SW, SH, col, row, 3)     // 3 = keyed
}

//# min=0 max=4 step=0.1 default=1
export function sliderSpeed(v) { speed = v }
