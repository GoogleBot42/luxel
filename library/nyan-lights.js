// name: Nyan Lights
// Clean-room reimplementation from a prose functional description of the
// community pattern "Nyan Lights"; original source never consulted.

// Nyan Cat pixel art: a pop-tart with golden crust and sprinkled pink
// frosting, a gray cat head with ears/eyes/cheeks plus paws and tail, and
// a six-band rainbow waving across the rest of the panel. A two-frame
// animation flips ~5x/second: rainbow column groups bob one row out of
// phase and the sprite jiggles one pixel sideways — the classic GIF wiggle.
//
// The original hardcoded a serpentine ~30x10 strip layout; this version is
// a true 2D renderer over a 16x16 virtual canvas, letting the pixel mapper
// own the wiring. Sprite transparency is an explicit flag (the original
// abused "hue == 0" as the sentinel, making red sprite pixels impossible).

var W = 16
var canvasH = array(256)
var canvasS = array(256)
var canvasV = array(256)
var opaque = array(256)

function stamp(c, r, h, s, v) {
  var i = r * W + c
  canvasH[i] = h
  canvasS[i] = s
  canvasV[i] = v
  opaque[i] = 1
}

// ---- static sprite setup (runs once) ----
var i
var j

// pop-tart crust: rectangle outline, c1..7 x r4..11, golden orange-brown
for (i = 1; i <= 7; i++) { stamp(i, 4, 0.09, 1, 1); stamp(i, 11, 0.09, 1, 1) }
for (j = 4; j <= 11; j++) { stamp(1, j, 0.09, 1, 1); stamp(7, j, 0.09, 1, 1) }

// frosting fill: light pink interior
for (j = 5; j <= 10; j++) {
  for (i = 2; i <= 6; i++) stamp(i, j, 0.97, 0.5, 1)
}
// berry sprinkles: deeper pink on a sparse checker
for (j = 5; j <= 9; j += 2) {
  for (i = 2; i <= 6; i += 2) stamp(i, j, 0.95, 0.95, 1)
}

// cat head to the right of the tart: dim warm gray outline
for (i = 8; i <= 12; i++) { stamp(i, 5, 0.08, 0.15, 0.3); stamp(i, 9, 0.08, 0.15, 0.3) }
for (j = 5; j <= 9; j++) { stamp(8, j, 0.08, 0.15, 0.3); stamp(12, j, 0.08, 0.15, 0.3) }
stamp(8, 4, 0.08, 0.15, 0.3)    // ears
stamp(12, 4, 0.08, 0.15, 0.3)
stamp(9, 6, 0.08, 0.15, 0.04)   // bead eyes: near-black
stamp(11, 6, 0.08, 0.15, 0.04)
stamp(9, 8, 0.97, 0.7, 0.6)     // pink cheek dots
stamp(11, 8, 0.97, 0.7, 0.6)
stamp(10, 8, 0.08, 0.15, 0.3)   // mouth

// paws below, tail at the left
stamp(2, 12, 0.08, 0.15, 0.3)
stamp(4, 12, 0.08, 0.15, 0.3)
stamp(6, 12, 0.08, 0.15, 0.3)
stamp(9, 12, 0.08, 0.15, 0.3)
stamp(11, 12, 0.08, 0.15, 0.3)
stamp(0, 7, 0.08, 0.15, 0.3)
stamp(0, 8, 0.08, 0.15, 0.3)

// rainbow band hues, top to bottom: red, amber, yellow-green, green,
// cyan-leaning blue, violet
var rainbow = array(6)
rainbow[0] = 0.0
rainbow[1] = 0.08
rainbow[2] = 0.2
rainbow[3] = 0.33
rainbow[4] = 0.58
rainbow[5] = 0.78

// ---- two-frame animation clock ----
var FLIP_MS = 200
var accum = 0
var shifted = 0

export function beforeRender(delta) {
  accum += delta
  if (accum >= FLIP_MS) {
    accum -= FLIP_MS
    shifted = 1 - shifted
  }
}

export function render2D(index, x, y) {
  var c = floor(x * 15.99)
  var r = floor(y * 15.99)
  var idx = r * W + c

  // sprite jiggles one pixel sideways on alternate frames; transparent
  // after the shift falls through to the rainbow/black logic
  var si = idx
  if (shifted && c < 15) si = idx + 1
  if (opaque[si]) {
    hsv(canvasH[si], canvasS[si], canvasV[si])
    return
  }

  // rainbow trail past roughly the first third of the panel: column
  // groups of four bob one row up/down in alternation
  if (c >= 6) {
    var f = shifted
    if (floor(c / 4) % 2) f = 1 - f
    var band = r - 4 - f
    if (band >= 0 && band < 6) {
      hsv(rainbow[band], 1, 1)
      return
    }
  }

  rgb(0, 0, 0)
}
