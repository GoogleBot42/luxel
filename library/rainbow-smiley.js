// name: Rainbow Smiley
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rainbow Smiley"; original source never consulted.

// A static 16x16 pixel-art smiley: the face disc is solid white, and
// everything else (background plus the eye/mouth cutouts) is a stencil
// filled with an animated rainbow keyed to the linear pixel index.

// 16x16 bitmap, row-major: 1 = white face pixel, 0 = rainbow stencil.
// Built procedurally instead of hand-packed color data — for this image
// the mask is all that matters.
var face = array(256)
var r, c
for (r = 0; r < 16; r++) {
  for (c = 0; c < 16; c++) {
    var dy = r - 7.5
    var dx = c - 7.5
    var d = sqrt(dx * dx + dy * dy)
    var lit = d <= 7                                   // the head disc
    if (r >= 4 && r <= 5 && ((c >= 4 && c <= 5) || (c >= 10 && c <= 11))) {
      lit = 0                                          // two square eyes
    }
    if (d >= 4.4 && d <= 6.3 && dy >= 2.2) {
      lit = 0                                          // smiling mouth arc
    }
    face[r * 16 + c] = lit
  }
}

var hueBase = 0

export function beforeRender(delta) {
  hueBase = time(0.03)  // full rainbow rotation every ~2 s
}

export function render2D(index, x, y) {
  var cell = floor(y * 15.99) * 16 + floor(x * 15.99)
  if (face[cell]) {
    rgb(1, 1, 1)  // face: pure white
  } else {
    // stencil: full hue wheel spread across the panel in wiring order,
    // rotating with the phase clock
    hsv(hueBase + index / pixelCount, 1, 1)
  }
}

// 1D fallback: just the animated rainbow
export function render(index) {
  hsv(hueBase + index / pixelCount, 1, 1)
}
