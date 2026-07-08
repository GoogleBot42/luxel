// name: scrolling text marquee 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "scrolling text marquee 2D"; original source never
// consulted.
//
// A marquee scroller: blocky glyphs stream right-to-left across the matrix in
// a single warm-white color on black, looping forever. Luxel has no string
// type, so "text" is a fixed list of glyph codes into a small bitmap font of
// blocky segments (real glyph rendering is out of scope). The scroll runs on a
// circular column buffer + head pointer, so each shift decodes just one glyph
// column and per-pixel work is a single array lookup. On a bare strip the same
// state drives a POV/light-painting column with a slow rainbow drift.

var GLYPH_W = 8      // font columns per glyph (inherent to the 8-bit row format)
var GLYPH_H = 8      // font rows per glyph
var W = 16           // display columns (virtual 16x16 canvas)
var H = 16           // display rows
var TOP = 4          // vertical margin: 8-row text centered on a 16-row display
var NGLYPH = 7       // glyph slots (0 = space, 6 = runtime-built variant)
var MSGLEN = 12

// --- storage ----------------------------------------------------------
var font = array(NGLYPH * GLYPH_H * GLYPH_W)  // on/off cells, one per bit
var canvas = array(W * H)                     // 16x16 virtual display, row-major
var tmpRows = array(GLYPH_H)                   // reusable 8-row-byte buffer
var scratch = array(GLYPH_H)                   // fetch/mutate scratch buffer

// Remote-settable state (rewrite over the device API without editing code)
export var message = array(MSGLEN)
export var textHue = 0.08     // warm incandescent white: slightly orange...
export var textSat = 0.35     // ...mostly toward white, lightly saturated

var speed = 3        // characters per second
var head = 0         // buffer column mapped to the leftmost physical column
var msgCol = 0       // pointer into the message's total column stream
var accum = 0        // millisecond accumulator
var totalCols = MSGLEN * GLYPH_W

// --- the three glyph operations ---------------------------------------
// store a glyph from eight row-bytes (MSB = leftmost column)
function glyphStoreRows(g, rows) {
  var r = 0
  while (r < GLYPH_H) {
    var byte = rows[r]
    var p = 128
    var c = 0
    while (c < GLYPH_W) {
      font[(g * GLYPH_H + r) * GLYPH_W + c] = floor(byte / p) % 2
      p = p / 2
      c = c + 1
    }
    r = r + 1
  }
}

// fetch a glyph into an 8-row-byte scratch buffer
function glyphFetch(g, out) {
  var r = 0
  while (r < GLYPH_H) {
    var acc = 0
    var p = 128
    var c = 0
    while (c < GLYPH_W) {
      acc = acc + font[(g * GLYPH_H + r) * GLYPH_W + c] * p
      p = p / 2
      c = c + 1
    }
    out[r] = acc
    r = r + 1
  }
}

// store back a (possibly modified) scratch glyph
function glyphStoreScratch(g, rows) {
  glyphStoreRows(g, rows)
}

// helper: load one built-in glyph from eight literal row-bytes
function loadGlyph(g, b0, b1, b2, b3, b4, b5, b6, b7) {
  tmpRows[0] = b0
  tmpRows[1] = b1
  tmpRows[2] = b2
  tmpRows[3] = b3
  tmpRows[4] = b4
  tmpRows[5] = b5
  tmpRows[6] = b6
  tmpRows[7] = b7
  glyphStoreRows(g, tmpRows)
}

// --- built-in blocky font (glyph 0 stays blank) -----------------------
loadGlyph(1, 66, 66, 66, 126, 66, 66, 66, 0)   // H
loadGlyph(2, 64, 64, 64, 64, 64, 64, 126, 0)   // L
loadGlyph(3, 60, 66, 66, 66, 66, 66, 60, 0)    // O
loadGlyph(4, 24, 24, 24, 24, 24, 0, 24, 0)     // !
loadGlyph(5, 60, 66, 2, 12, 16, 0, 16, 0)      // ? (a retro-styled slot)

// build glyph 6 at runtime as a tweaked copy of glyph 5: fetch -> mutate
// (shift every row right one column) -> store back into the adjacent slot
glyphFetch(5, scratch)
var mr = 0
while (mr < GLYPH_H) {
  scratch[mr] = floor(scratch[mr] / 2)
  mr = mr + 1
}
glyphStoreScratch(6, scratch)

// --- the message ("  HLLO?! " plus the runtime variant), glyph codes ---
message[0] = 0
message[1] = 0
message[2] = 1
message[3] = 2
message[4] = 2
message[5] = 3
message[6] = 5
message[7] = 4
message[8] = 6
message[9] = 0
message[10] = 0
message[11] = 0

// --- controls ---------------------------------------------------------
//# min=0 max=1 step=0.02 default=0.1
export function sliderSpeed(v) {
  // characters per second: ~0.5 (readable matrix) up to ~20 (POV strip)
  speed = 0.5 + v * 20
}

export function hsvPickerTextColor(h, s, v) {
  // recolor the marquee; value is ignored (glyph bit drives brightness)
  textHue = h
  textSat = s
}

// --- scroll -----------------------------------------------------------
// advance one column: decode a single glyph column into the head slot
function scrollStep() {
  var ch = floor(msgCol / GLYPH_W)
  var colIn = msgCol % GLYPH_W
  var g = message[ch]
  var r = 0
  while (r < GLYPH_H) {
    canvas[(TOP + r) * W + head] = font[(g * GLYPH_H + r) * GLYPH_W + colIn]
    r = r + 1
  }
  head = (head + 1) % W
  msgCol = (msgCol + 1) % totalCols
}

export function beforeRender(delta) {
  // one column-step every (1s / speed / glyph-width); subtract (don't reset)
  // so cadence stays accurate even at high frame rates
  var period = 1000 / (speed * GLYPH_W)
  accum = accum + delta
  var guard = 0
  while (accum >= period && guard < 64) {
    accum = accum - period
    scrollStep()
    guard = guard + 1
  }
}

// --- 2D render: readable marquee --------------------------------------
export function render2D(index, x, y) {
  var row = floor(y * 15.99)
  var physCol = floor(x * 15.99)
  var bufCol = (physCol + head) % W   // physical column offset by the head
  var cell = canvas[row * W + bufCol]
  hsv(textHue, textSat, cell)         // warm white where set, black otherwise
}

// --- 1D render: POV / light-painting column with rainbow drift --------
export function render(index) {
  var p = pixelCount - 1 - index      // flip so pixel 0 is the bottom of text
  var span = floor(GLYPH_H * 1.5)     // glyphs repeat with half-glyph spacing
  var row = p % span
  if (row >= GLYPH_H) {               // the blank line-spacing gap
    hsv(0, 0, 0)
    return
  }
  var cell = canvas[(TOP + row) * W + head]  // always the marquee's left column
  // slow triangle hue drift (~90 s cycle) minus a per-pixel ramp -> multicolor
  var hue = triangle(time(1.4)) - index / pixelCount
  hsv(hue, 1, cell)
}
