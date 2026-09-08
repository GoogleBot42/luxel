// name: Frame Rate Scan
// Curated original for the Luxel library — an instrument, not a look.
//
// WHAT IT IS
// A self-timing scan for measuring, from a phone video, how many frames a
// fixture actually DISPLAYS. It carries its own clock, so the camera's frame
// rate never enters the arithmetic — you do not need to know it, and it does
// not have to be exactly 240 Hz.
//
// WHY IT HAS TO BE A SCAN
// A dropped frame leaves no mark on any single composed frame: every frame is
// a complete picture, and the one that never reached the panel simply never
// existed. So the pattern spends one visible state per composed frame — a bar
// one column wide that steps exactly one column per `renderFrame` call. A
// frame the panel never showed is a column you never see.
//
// THE THREE BARS (all one column wide, so a position is countable)
//   rows 0..3/8 H     SWEEP    — column = frame counter mod W, RED on even
//                                composed frames, GREEN on odd. One step per
//                                composed frame, driven by a counter and
//                                never by the clock.
//   rows 7/16..5/8 H  FINE     — column = floor(elapsedMs / 10) mod W, BLUE.
//                                One column per 10 ms; wraps every 640 ms.
//   rows 11/16..7/8 H COARSE   — column = floor(elapsedMs / 100) mod W,
//                                WHITE. One column per 100 ms; wraps every
//                                6.4 s.
// Dim grey ticks every 8 columns sit under each bar's band and on the bottom
// row, with column 0 marked in cyan, so columns can be counted off a paused
// video frame without squinting. (On a 64-wide grid; every boundary above is
// a fraction of the grid, so other matrices scale. A fixture with no grid —
// a bare strip — gets the sweep alone, along the strip.)
//
// READING IT
// 1. Film the panel at any rate at least twice the display rate you expect
//    (a phone's 240 fps "slo-mo" is plenty for ~115). Get 2-3 seconds.
// 2. Scrub to a video frame near the start; call it A. Scrub to one about a
//    second later; call it B.
// 3. Read the clock on each. With `c` the coarse column and `f` the fine
//    column:
//        d = (f - 10*c) mod 64          -> 0..9, the tens-of-ms digit
//        t = 100*c + 10*d               -> milliseconds within the 6.4 s cycle
//    dt = tB - tA, adding 6400 ms if the coarse bar wrapped past column 0
//    between them. (Worked example: c = 7, f = 12 -> d = (12 - 70) mod 64 =
//    6, t = 700 + 60 = 760 ms.)
// 4. Step through every video frame from A to B and write down the SWEEP
//    column each time. Ignore repeats — the camera sees most displayed frames
//    two or three times. Count the DISTINCT positions: that is how many
//    frames the panel displayed.
//
//        displayed fps = distinct positions / dt
//
// 5. Two extras from the same list. Where the column jumps by more than one,
//    the panel skipped composed frames — `jump - 1` of them each time. And
//    the colour is the check: the sweep alternates red/green every composed
//    frame, so two adjacent DISTINCT positions sharing a colour means an even
//    number of frames went by, i.e. at least one was dropped.
//
//        composed fps = (distinct positions + dropped) / dt
//
//    That composed figure should match `/api/status` `fps`; the displayed one
//    is the number no API field reports.
//
// The camera's real frame rate cancels out completely: it appears nowhere in
// step 4 or 5, and dt comes off the panel, not off the video's timeline.
//
// CAMERA CAVEATS (rolling shutter; dark frames at the sweep's wrap; dimmed
// columns) — read "Camera caveats" under `library/frame-rate-scan.js` in
// docs/boards.md before calling anything on the video a display artefact.
//
// ON THE COMPOSE CAP
// The one slider caps `setFrameRate`. Leave it at 0 for a real measurement.
// Note the firmware's render loop is paced to one iteration per 8 ms and the
// engine resets its frame accumulator when a capped frame fires, so the only
// achievable compose rates are 125/n — 125, 62.5, 41.7, 31.3 ... Asking for
// 100 gets you 62.5 (Gitea #384). Dropping the cap below the display rate is
// still a useful control: every composed frame is then shown at least once,
// so the sweep never skips a column and the count must come out equal to the
// cap.

var composeCap = 0

var elapsedMs = 0        // pattern clock, wrapped to the coarse bar's cycle
var frameCol = 0         // sweep column: one step per composed frame
var frameParity = 0      // red/green, flipped once per composed frame

export var frameIndex = 0
export var clockMs = 0

export function beforeRender(delta) {
  // A stall must not fast-forward the clock past a whole coarse wrap. The
  // 30000 ms roll only ever fires on a fixture with no clock bars to draw —
  // renderFrame reduces the clock modulo the coarse cycle every frame — and
  // exists so a bare strip cannot walk the counter into 16.16 overflow.
  elapsedMs = mod(elapsedMs + clamp(delta, 0, 1000), 30000)
  clockMs = elapsedMs
}

export function renderFrame() {
  var W = gridWidth()
  var H = gridHeight()

  clear()

  // No grid, or too few rows to band: the sweep alone, along the strip.
  if (W < 1 || H < 4) {
    var n = pixelCount
    sweepBrush()
    setPixel(mod(frameCol, n))
    step(n)
    return
  }

  // The clock wraps with the coarse bar so neither can overflow 16.16.
  var cycle = W * 100
  if (elapsedMs >= cycle) elapsedMs = mod(elapsedMs, cycle)

  // Band edges as fractions of the grid — 0/24, 28/40, 44/56 on a 64-row
  // panel. `max` keeps every band at least one row tall on a small matrix.
  var sweepBot = max(1, floor((H * 3) / 8))
  var fineTop = max(sweepBot + 1, floor((H * 7) / 16))
  var fineBot = max(fineTop + 1, floor((H * 5) / 8))
  var coarseTop = max(fineBot + 1, floor((H * 11) / 16))
  var coarseBot = max(coarseTop + 1, floor((H * 7) / 8))

  // --- the three bars ---------------------------------------------------
  sweepBrush()
  column(0, sweepBot, W, mod(frameCol, W))

  rgb(0, 0.35, 1)
  column(fineTop, fineBot, W, mod(floor(elapsedMs / 10), W))

  rgb(1, 1, 1)
  column(coarseTop, coarseBot, W, mod(floor(elapsedMs / 100), W))

  // --- counting ticks ---------------------------------------------------
  // One row under each band plus the bottom row: every 8th column dim grey,
  // column 0 cyan so a paused frame has an origin to count from.
  ticks(sweepBot, W, H)
  ticks(fineBot, W, H)
  ticks(coarseBot, W, H)
  ticks(H - 1, W, H)

  step(W)
}

// The brush for the sweep: red on even composed frames, green on odd.
function sweepBrush() {
  if (frameParity) rgb(0, 1, 0)
  else rgb(1, 0, 0)
}

// One column of the brush, rows [r0, r1), on a W-wide row-major grid.
function column(r0, r1, W, col) {
  for (var r = r0; r < r1; r++) setPixel(r * W + col)
}

// Every 8th column on one row, dim; column 0 marked cyan.
function ticks(row, W, H) {
  if (row < 0 || row >= H) return
  for (var c = 0; c < W; c += 8) {
    if (c == 0) rgb(0, 0.22, 0.22)
    else rgb(0.16, 0.16, 0.16)
    setPixel(row * W + c)
  }
}

// One step per COMPOSED frame. The parity is its own toggle rather than a
// bit of the counter, so the counter's roll can never skip a flip.
function step(W) {
  frameCol = frameCol + 1 >= W ? 0 : frameCol + 1
  frameParity = 1 - frameParity
  frameIndex = frameIndex >= 30000 ? 0 : frameIndex + 1
}

// --- controls -------------------------------------------------------------

// setFrameRate() cap, 0 = uncapped. Quantized to 125/n by the firmware's 8 ms
// render-loop pacing — see the header. Leave at 0 to measure.
//# min=0 max=125 step=5 default=0
export function sliderComposeCap(v) {
  composeCap = clamp(floor(v), 0, 125)
  setFrameRate(composeCap)
}
