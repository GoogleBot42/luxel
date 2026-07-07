// name: Nyan Lights
// Clean-room reimplementation from a prose functional description of the
// community pattern "Nyan Lights"; original source never consulted.

// Nyan Cat pixel art: a pop-tart with crust and sprinkled pink frosting, a
// gray cat head with paws and tail, and a bobbing rainbow. Implemented as a
// true 2D renderer over a 16x16 logical canvas (the pixel mapper handles the
// physical wiring) instead of the original's hardcoded serpentine math. The
// original used "hue == 0" as a transparency sentinel; we use an explicit
// opacity flag instead.

var W = 16
var hueA = array(256)
var satA = array(256)
var valA = array(256)
var opq = array(256) // explicit transparency flag: 1 = sprite pixel

function px(c, r, h, s, v) {
  var i = r * W + c
  hueA[i] = h
  satA[i] = s
  valA[i] = v
  opq[i] = 1
}

// ---- sprite colors ----
var CRUST_H = 0.09, CRUST_S = 1, CRUST_V = 1        // golden orange-brown
var FROST_H = 0.95, FROST_S = 0.45, FROST_V = 1     // light pink
var BERRY_H = 0.93, BERRY_S = 0.85, BERRY_V = 0.9   // deeper pink sprinkles
var GRAY_H = 0.1, GRAY_S = 0.12, GRAY_V = 0.28      // warm dim gray
var EYE_V = 0.04                                    // near-black beads
var CHEEK_H = 0.95, CHEEK_S = 0.6, CHEEK_V = 0.8    // pink cheek dots

// ---- stamp the sprite (once, at startup) ----
var c, r

// Pop-tart: rectangle cols 1..7, rows 5..11 — crust border, frosting fill.
for (r = 5; r <= 11; r++) {
  for (c = 1; c <= 7; c++) {
    if (r == 5 || r == 11 || c == 1 || c == 7) {
      px(c, r, CRUST_H, CRUST_S, CRUST_V)
    } else if (c % 2 == 0 && r % 2 == 0) {
      px(c, r, BERRY_H, BERRY_S, BERRY_V) // sparse checker of berry sprinkles
    } else {
      px(c, r, FROST_H, FROST_S, FROST_V)
    }
  }
}

// Cat head to the right of the tart: fill rows 6..10, cols 8..12.
for (r = 6; r <= 10; r++) {
  for (c = 8; c <= 12; c++) {
    px(c, r, GRAY_H, GRAY_S, GRAY_V)
  }
}
px(8, 5, GRAY_H, GRAY_S, GRAY_V)   // left ear
px(12, 5, GRAY_H, GRAY_S, GRAY_V)  // right ear
px(9, 7, GRAY_H, GRAY_S, EYE_V)    // eyes: same gray family, very dim
px(11, 7, GRAY_H, GRAY_S, EYE_V)
px(8, 8, CHEEK_H, CHEEK_S, CHEEK_V)  // pink cheek dots
px(12, 8, CHEEK_H, CHEEK_S, CHEEK_V)
px(10, 9, GRAY_H, GRAY_S, 0.16)    // mouth line
px(0, 8, GRAY_H, GRAY_S, GRAY_V)   // short tail at the tart's left

// Paws below the body.
px(2, 12, GRAY_H, GRAY_S, GRAY_V)
px(4, 12, GRAY_H, GRAY_S, GRAY_V)
px(9, 12, GRAY_H, GRAY_S, GRAY_V)
px(11, 12, GRAY_H, GRAY_S, GRAY_V)

// Rainbow band hues, top to bottom.
var rain = array(6)
rain[0] = 0     // red
rain[1] = 0.075 // orange/amber
rain[2] = 0.18  // yellow-green
rain[3] = 0.33  // green
rain[4] = 0.55  // cyan-leaning blue
rain[5] = 0.78  // violet

// ---- two-frame GIF-style animation ----
var acc = 0
var shifted = 0

export function beforeRender(delta) {
  acc += delta
  if (acc > 200) { // flip ~5x per second
    acc -= 200
    shifted = !shifted
  }
}

export function render2D(index, x, y) {
  var col = floor(x * 15.99)
  var row = floor(y * 15.99)
  var i = row * W + col

  // Sprite pass: on alternate frames read the neighboring entry so the cat
  // jiggles one pixel sideways. Transparent-after-shift falls through to the
  // rainbow/black logic (cleaner than the original's stale-pixel quirk).
  var j = i
  if (shifted) j = i + 1
  if (j < 256 && opq[j]) {
    hsv(hueA[j], satA[j], valA[j])
    return
  }

  // Rainbow: columns past roughly the first third, grouped in runs of four;
  // alternate groups bob one row out of phase with the flip flag.
  if (col >= 5) {
    var f = shifted
    if (floor(col / 4) % 2 == 1) f = !f
    var band = row - 2 - f // six rows starting a couple rows from the top
    if (band >= 0 && band < 6) {
      hsv(rain[band], 1, 1)
      return
    }
  }

  rgb(0, 0, 0)
}
