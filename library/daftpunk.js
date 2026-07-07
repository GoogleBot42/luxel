// name: DAFTPUNK
// Clean-room reimplementation from a prose functional description of the
// community pattern "DAFTPUNK"; original source never consulted.

// Marquee text scroller: bold red 8x8 pixel-font letters scroll smoothly
// right-to-left on black, looping forever. Scrolling uses a circular
// column buffer — only the column entering at the right edge is decoded
// each step; nothing is ever shifted.

var ROWS = 8            // glyph height
var CHAR_COLS = 9       // 8 glyph columns + 1 blank separator column
var VIEW_W = 16         // displayed columns (virtual canvas width)
var BUF_W = 24          // circular buffer width (view + headroom)
var COL_MS = 40         // ms per one-column step (~2.8 chars/sec)
var TOP = 4             // text band sits on canvas rows 4..11

// Extra-bold custom glyphs, one 8-bit mask per row (bit 7 = leftmost).
var font = array(8 * ROWS)
function glyph(slot, r0, r1, r2, r3, r4, r5, r6, r7) {
  var b = slot * ROWS
  font[b] = r0;     font[b + 1] = r1
  font[b + 2] = r2; font[b + 3] = r3
  font[b + 4] = r4; font[b + 5] = r5
  font[b + 6] = r6; font[b + 7] = r7
}
glyph(0, 252, 254, 198, 198, 198, 198, 254, 252)  // D
glyph(1, 124, 254, 198, 198, 254, 254, 198, 198)  // A
glyph(2, 254, 254, 192, 252, 252, 192, 192, 192)  // F
glyph(3, 254, 254, 56, 56, 56, 56, 56, 56)        // T
glyph(4, 252, 254, 198, 254, 252, 192, 192, 192)  // P
glyph(5, 198, 198, 198, 198, 198, 198, 254, 124)  // U
glyph(6, 198, 230, 246, 254, 222, 206, 198, 198)  // N
glyph(7, 198, 204, 216, 240, 240, 216, 204, 198)  // K

// The message: glyph slot indices, exported so it can be rewritten live.
export var message = array(8)
message[0] = 0; message[1] = 1; message[2] = 2; message[3] = 3
message[4] = 4; message[5] = 5; message[6] = 6; message[7] = 7

var buf = array(BUF_W)     // circular buffer: one 8-bit row-mask per column
var head = 0               // buffer column currently shown at the left edge
var msgCol = 0             // next column of the message to decode
var accum = 0              // millisecond accumulator
var canvas = array(256)    // 16x16 virtual canvas, row-major

// Decode one column of the message into a row-bit mask (bit r = row r on).
// Column 8 of each character reads past the glyph and stays blank.
function decodeColumn(mc) {
  var ch = message[floor(mc / CHAR_COLS)]
  var cc = mc % CHAR_COLS
  if (cc >= 8) return 0
  var mask = 0
  var r
  for (r = 0; r < ROWS; r++) {
    if ((font[ch * ROWS + r] >> (7 - cc)) & 1) mask += 1 << r
  }
  return mask
}

export function beforeRender(delta) {
  accum += delta
  while (accum > COL_MS) {
    accum -= COL_MS
    // Advance the window one column and decode the message column that
    // just entered at the right edge — pointer motion, no buffer shift.
    head = (head + 1) % BUF_W
    buf[(head + VIEW_W - 1) % BUF_W] = decodeColumn(msgCol)
    msgCol = (msgCol + 1) % (arrayLength(message) * CHAR_COLS)
  }

  // Repaint the virtual canvas from the circular buffer.
  var cx, r
  for (cx = 0; cx < VIEW_W; cx++) {
    var mask = buf[(head + cx) % BUF_W]
    for (r = 0; r < ROWS; r++) {
      canvas[(TOP + r) * 16 + cx] = (mask >> r) & 1
    }
  }
}

export function render2D(index, x, y) {
  if (canvas[floor(y * 15.99) * 16 + floor(x * 15.99)]) {
    rgb(1, 0, 0)   // pure vivid red text
  } else {
    rgb(0, 0, 0)   // on black
  }
}
