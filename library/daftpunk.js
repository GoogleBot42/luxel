// name: DAFTPUNK
// Clean-room reimplementation from a prose functional description of the
// community pattern "DAFTPUNK"; original source never consulted.

// A marquee text scroller: bold red pixel-font letters scroll smoothly
// right-to-left over black, looping forever. This clean version carries
// just the eight chunky demo glyphs (D A F T P U N K); the message is an
// exported array of glyph indices so it can be rewritten live.

// --- Font: 8x8 glyphs, one byte per row, MSB = leftmost column ---
var font = array(64)
var fontFill = 0
function glyph(r0, r1, r2, r3, r4, r5, r6, r7) {
  font[fontFill] = r0
  font[fontFill + 1] = r1
  font[fontFill + 2] = r2
  font[fontFill + 3] = r3
  font[fontFill + 4] = r4
  font[fontFill + 5] = r5
  font[fontFill + 6] = r6
  font[fontFill + 7] = r7
  fontFill += 8
}
glyph(248, 252, 206, 198, 198, 206, 252, 248)  // 0: D
glyph(56, 124, 238, 198, 254, 254, 198, 198)   // 1: A
glyph(254, 254, 192, 252, 252, 192, 192, 192)  // 2: F
glyph(254, 254, 56, 56, 56, 56, 56, 56)        // 3: T
glyph(252, 254, 198, 254, 252, 192, 192, 192)  // 4: P
glyph(198, 198, 198, 198, 198, 198, 254, 124)  // 5: U
glyph(198, 230, 246, 254, 222, 206, 198, 198)  // 6: N
glyph(198, 204, 216, 240, 240, 216, 204, 198)  // 7: K

// Message = glyph indices; exported so it can be changed over the network.
export var message = array(8)
var msgLen = 8
message[0] = 0
message[1] = 1
message[2] = 2
message[3] = 3
message[4] = 4
message[5] = 5
message[6] = 6
message[7] = 7

// --- Scroll state: circular column buffer, no content ever shifts ---
var dispW = 16                       // displayed columns (16x16 canvas)
var bufW = 24                        // buffer columns (display + headroom)
var buf = array(bufW * 8)            // 8 rows per column, 0/1 cells
var canvas = array(256)              // 16x16 virtual canvas, row-major
var writePtr = 0                     // buffer column shown at the left edge
var msgCol = 0                       // next column of the message to decode
var msAccum = 0

var colsPerChar = 9                  // 8 glyph columns + 1 blank separator
var charsPerSec = 2.5                // edit to taste
var stepMs = 1000 / (charsPerSec * colsPerChar)

function stepColumn() {
  var ch = message[floor(msgCol / colsPerChar)]
  var col = msgCol % colsPerChar
  var dst = (writePtr + dispW) % bufW  // enters just past the right edge
  for (var r = 0; r < 8; r++) {
    var bit = 0
    // Column 8 reads past the glyph data -> blank separator column.
    if (col < 8) bit = (font[ch * 8 + r] >> (7 - col)) & 1
    buf[dst * 8 + r] = bit
  }
  writePtr = (writePtr + 1) % bufW
  msgCol = (msgCol + 1) % (msgLen * colsPerChar)
}

function blit() {
  // Repaint the canvas window starting at writePtr; text band is
  // vertically centered (rows 4..11 of the 16-row canvas).
  for (var x = 0; x < dispW; x++) {
    var src = (writePtr + x) % bufW
    for (var y = 0; y < 16; y++) {
      var v = 0
      if (y >= 4 && y < 12) v = buf[src * 8 + y - 4]
      canvas[y * 16 + x] = v
    }
  }
}

export function beforeRender(delta) {
  msAccum += delta
  if (msAccum > 500) msAccum = 500  // cap catch-up after a stall
  var stepped = 0
  while (msAccum > stepMs) {
    msAccum -= stepMs
    stepColumn()
    stepped = 1
  }
  if (stepped) blit()
}

export function render2D(index, x, y) {
  var v = canvas[floor(y * 15.99) * 16 + floor(x * 15.99)]
  hsv(0, 1, v)  // pure red on, black off
}
