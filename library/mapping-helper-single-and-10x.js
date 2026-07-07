// name: Mapping Helper Single and 10x
// Clean-room reimplementation from a prose functional description of the
// community pattern "Mapping Helper Single and 10x"; original source never
// consulted.

// Pixel-identification ruler: a full-white cursor steps ~3 px/s along the
// strip, filling in blocks of ten behind it, each block dimly lit from a
// five-color cycle (red, green, blue, amber, violet). Read tens off the
// block color, the exact pixel off the white dot. On wrap the whole strip
// flashes dim purple for one step, then the fill restarts.
//
// Instead of relying on persistent-framebuffer semantics, every completed
// block is explicitly repainted each frame from the deterministic mapping
// (block number modulo five selects the color) — identical visuals. The
// color cycle is well-defined from the very first block (startup quirk in
// the description fixed by construction).

var BLOCK = 10          // humans count in tens
var STEP_MS = 333       // ~3 cursor steps per second
var DIM = 0.2           // block fill brightness

var cursor = 0
var accum = 0

// Five-color cycle, dim; blue boosted a bit so it reads comparably.
var cr = array(5)
var cg = array(5)
var cb = array(5)
cr[0] = DIM;        cg[0] = 0;          cb[0] = 0            // red
cr[1] = 0;          cg[1] = DIM;        cb[1] = 0            // green
cr[2] = 0;          cg[2] = 0;          cb[2] = DIM * 1.5    // blue (boosted)
cr[3] = DIM;        cg[3] = DIM * 0.75; cb[3] = 0            // warm amber
cr[4] = DIM * 0.75; cg[4] = 0;          cb[4] = DIM          // violet

export function beforeRender(delta) {
  accum += delta
  if (accum > STEP_MS) {
    accum = 0
    cursor += 1
    if (cursor >= pixelCount) cursor = 0   // wrap -> wipe frame
  }
}

export function render(index) {
  if (cursor == 0) {
    rgb(0.1, 0, 0.1)                       // full-strip dim purple wipe
    return
  }
  if (index == cursor) {
    rgb(1, 1, 1)                           // the cursor itself
    return
  }
  var blockStart = floor(cursor / BLOCK) * BLOCK
  if (index < blockStart + BLOCK) {
    // completed blocks + the block in progress: block number picks the color
    var c = floor(index / BLOCK) % 5
    rgb(cr[c], cg[c], cb[c])
  } else {
    rgb(0, 0, 0)                           // not reached yet this pass
  }
}
