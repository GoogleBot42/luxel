// name: Rainbow Smiley
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rainbow Smiley"; original source never consulted.

// A static 16x16 pixel-art smiley: the face disc is solid white, while the
// background plus the eye/mouth cutouts are a stencil filled with an
// animated rainbow keyed to linear pixel index. Only the rainbow animates.

var SIZE = 16
// Stencil bitmap: 1 = white face pixel, 0 = masked (rainbow shows through).
var face = array(SIZE * SIZE)

// Build the smiley procedurally instead of hand-typing 256 cells:
// a filled disc, minus two square eyes and a lower arc for the mouth.
var r, c
for (r = 0; r < SIZE; r++) {
  for (c = 0; c < SIZE; c++) {
    var dr = r - 7.5
    var dc = c - 7.5
    var d = sqrt(dr * dr + dc * dc)
    var lit = d <= 7                                  // head disc
    if (r >= 4 && r <= 5 && c >= 4 && c <= 5) lit = 0 // left eye
    if (r >= 4 && r <= 5 && c >= 10 && c <= 11) lit = 0 // right eye
    if (r >= 9 && d >= 4 && d <= 5.6) lit = 0         // smiling mouth arc
    face[r * SIZE + c] = lit
  }
}

var t1

export function beforeRender(delta) {
  t1 = time(0.03) // ~2 s full hue rotation
}

export function render2D(index, x, y) {
  // First coordinate selects the row, second the column (per the original;
  // orientation depends on the mapper).
  var cell = floor(y * 15.99) * SIZE + floor(x * 15.99)
  if (face[cell]) {
    rgb(1, 1, 1) // the face itself stays plain white
  } else {
    // Background, eyes and mouth: rainbow spread by wiring-order index.
    hsv(t1 + index / pixelCount, 1, 1)
  }
}

// 1D fallback: whole strip becomes the rainbow fill.
export function render(index) {
  hsv(t1 + index / pixelCount, 1, 1)
}
