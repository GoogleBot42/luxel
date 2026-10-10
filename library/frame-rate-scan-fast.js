// name: Frame Rate Scan (fast)
// Curated original for the Luxel library — an instrument, not a look.
//
// The sweep from `frame-rate-scan.js` and nothing else, rebuilt so the
// pattern itself is never the bottleneck: no `clear()`, no per-pixel loops,
// no clock bars, no ticks. Every frame is two native row fills — erase
// the previous row, draw the new one — under 100 us of VM time on a 64x64
// panel. What you measure with it is the firmware's compose + hand-off (on
// the Seengreat S3 the bitplane packer is ~2.5 ms per 4096-px frame, so
// composed frames top out around 300-400/s; Gitea #831 is the lever).
//
// WHAT IT DRAWS
//   A full-width ROW, one row lower per composed frame, RED on even
//   composed frames and GREEN on odd. Wraps every H rows. A composed frame
//   the panel never showed is a row you never see; two adjacent visible rows
//   of the same colour mean at least one frame between them was dropped.
//   Time it against /api/status `fps` (composed) — displayed rows per second
//   on a video are the displayed rate.
//
// On a fixture with no grid (a bare strip) the same thing happens along the
// strip, one pixel per composed frame.

var sweepRow = 0
var sweepParity = 0
var prevSweep = -1      // row (or pixel) drawn last frame; -1 = nothing yet

export var frameIndex = 0

export function renderBulk() {
  var W = gridWidth()
  var H = gridHeight()
  if (W < 1 || H < 1) {
    var n = pixelCount
    if (prevSweep >= 0) { rgb(0, 0, 0); setPixel(prevSweep) }
    brush()
    setPixel(sweepRow)
    prevSweep = sweepRow
    sweepRow = sweepRow + 1 >= n ? 0 : sweepRow + 1
    step()
    return
  }
  if (prevSweep >= 0) {
    rgb(0, 0, 0)
    band(0, W, prevSweep, prevSweep + 1, W, H)
  }
  brush()
  band(0, W, sweepRow, sweepRow + 1, W, H)
  prevSweep = sweepRow
  sweepRow = sweepRow + 1 >= H ? 0 : sweepRow + 1
  step()
}

// Cells [c0, c1) x [r0, r1) of the W x H grid, in the brush, as ONE native
// `fillRect`. The frame is in wire order, so a grid row is not an index
// range on a chain of panels; a rectangle in mapped coordinates is, on any
// grid. An inner edge is the edge cell's own mapped coordinate (`pixelCoord`
// of its `gridIndex`), so the inclusive bound lands exactly on that cell's
// centre and takes in exactly the cells inside it. An edge at the grid's
// border is left open (-1 / 2, outside 0..1) instead: a one-row grid — what
// a bare strip gets — has no y axis of its own, and `pixelCoord` would read
// it as 0 where the shape test sees the 1D fallback's 0.5. The y edges are
// read from column 0 and the x edges from row 0: those cells always have a
// pixel, where the tail of an over-provisioned last row does not.
function band(c0, c1, r0, r1, W, H) {
  if (c1 <= c0 || r1 <= r0) return
  var x0 = c0 <= 0 ? -1 : pixelCoord(gridIndex(c0, 0), 0)
  var x1 = c1 >= W ? 2 : pixelCoord(gridIndex(c1 - 1, 0), 0)
  var y0 = r0 <= 0 ? -1 : pixelCoord(gridIndex(0, r0), 1)
  var y1 = r1 >= H ? 2 : pixelCoord(gridIndex(0, r1 - 1), 1)
  fillRect(x0, y0, x1, y1)
}

function brush() {
  if (sweepParity) rgb(0, 1, 0)
  else rgb(1, 0, 0)
}

function step() {
  sweepParity = 1 - sweepParity
  frameIndex = frameIndex >= 30000 ? 0 : frameIndex + 1
}
