// name: Frame Rate Scan (fast)
// Curated original for the Luxel library — an instrument, not a look.
//
// The sweep from `frame-rate-scan.js` and nothing else, rebuilt so the
// pattern itself is never the bottleneck: no `clear()`, no per-pixel loops,
// no clock bars, no ticks. Every frame is two `fillRange` memsets — erase
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
    fillRange(prevSweep * W, prevSweep * W + W)
  }
  brush()
  fillRange(sweepRow * W, sweepRow * W + W)
  prevSweep = sweepRow
  sweepRow = sweepRow + 1 >= H ? 0 : sweepRow + 1
  step()
}

function brush() {
  if (sweepParity) rgb(0, 1, 0)
  else rgb(1, 0, 0)
}

function step() {
  sweepParity = 1 - sweepParity
  frameIndex = frameIndex >= 30000 ? 0 : frameIndex + 1
}
