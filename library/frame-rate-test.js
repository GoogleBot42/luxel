// name: Frame Rate Test
// Curated original for the Luxel library — an instrument, not a look.
//
// WHAT IT ANSWERS
// "How many frames per second does this fixture actually DISPLAY?" That is
// not the same number as the render/compose rate `/api/status` reports. The
// engine composes a frame; the output stage shows whatever is in the buffer
// when the panel next scans it. Compose faster than the panel scans and the
// surplus frames are overwritten before anyone sees them.
//
// WHY YOU CANNOT SEE A DROPPED FRAME IN A SINGLE FRAME
// A dropped frame leaves no mark on the picture. Every composed frame is a
// complete image; the one that never reached the panel simply never existed
// as far as the eye is concerned, and the frames around it look perfect. So
// the signal has to be TEMPORAL: the pattern must make consecutive frames
// differ in a way whose *rhythm* breaks when one goes missing.
//
// THE STROBOSCOPE BEAT (the top band, rows 0..3/4 H)
// Every composed frame the whole field flips colour: RED on even frames,
// GREEN on odd ones. The flip is driven by a counter incremented once per
// `renderFrame` call — never by the clock — so it is exactly one flip per
// composed frame.
//   * If the panel displayed every composed frame exactly once, the eye
//     would fuse the alternation into steady YELLOW.
//   * Every frame the panel misses (or shows twice) puts two same-colour
//     frames next to each other on the retina — a single-refresh red or
//     green stumble in the yellow.
// Those stumbles happen |C - R| times a second, where C is the compose rate
// and R the displayed rate. So the field shimmers red/green at the BEAT
// FREQUENCY |C - R|, and
//
//     displayed fps  =  compose fps  -  stumbles per second
//
// READING THE BEAT BY EYE (the reference band, bottom 1/16)
// Counting ten shimmers a second is hard; matching two rates is easy. The
// bottom band blinks blue at |compose fps - "DisplayedFPS" slider| Hz. Turn
// the slider until the blue blink and the field's shimmer keep the same
// time. The slider then reads the displayed rate directly. Both moving the
// slider up and down from the match speeds the blink up, so the match is
// unambiguous even though the shimmer itself gives no direction.
//
// READING IT WITH A CAMERA (definitive)
// Film the panel at 240 fps. At ~115 displayed fps each displayed frame
// occupies about two camera frames, so the colour normally changes every
// two. Step through and count the runs that are twice as long — one colour
// held for ~4 camera frames. That count per second is exactly C - R, and
// the on-screen bar gives you C. This is the measurement the eye method
// only approximates.
//
// THE BANDS, top to bottom (fractions of the grid, so any matrix works)
//   0 .. 3/4 H     strobe field — red/green, one flip per composed frame
//   3/4 .. 7/8 H   compose-fps bar: the pattern's own EMA of 1000/delta,
//                  2 fps per column, with tick marks on its top row at
//                  60 / 77 / 115 (magenta) / 125 fps
//   7/8 .. 15/16 H frame-parity halves: left half lit on even frames, right
//                  on odd, dim — the same beat as a left/right stutter
//   15/16 .. H     beat reference: the blue blink described above
// A fixture with fewer than 4 rows (or no grid at all) gets the strobe
// across the whole strip and nothing else.
//
// THE COMPOSE CAP, AND WHY IT IS COARSE
// "ComposeCap" calls `setFrameRate(v)` (0 = uncapped). Be aware that the
// firmware's render loop is paced to one iteration per 8 ms, and the engine
// fires a capped frame on the first tick at or past 1000/v ms and then
// resets its accumulator — so the achievable compose rates are 125/n only:
// 125, 62.5, 41.7, 31.3, 25 ... Asking for 115 gets you 62.5, not 115.
// That is why the strobe above runs UNCAPPED: the beat against the panel's
// scan rate is the measurement, and only the uncapped 8 ms cadence is
// regular enough to give a clean one. The cap is still worth having — drop
// it below the display rate and every composed frame is shown at least
// once, so the beat disappears and is replaced by a gross, regular flicker
// at v/2 Hz. That transition is itself a bound on the displayed rate.
//
// On the Seengreat 64x64 HUB75 panel (30 MHz LCD_CAM) the compose rate is
// the 125 fps pacing cap and the panel rescans at ~115.3 Hz, so expect a
// ~10 Hz shimmer and a match near DisplayedFPS = 115.

var contrast = 1
var guessFps = 115
var composeCap = 0

var emaFps = 60          // EMA of 1000/delta — the measured compose rate
var refPhase = 0         // 0..1 phase of the beat-reference blink
var frameParity = 0      // flips once per renderFrame call
var tickFps = array(4)
tickFps[0] = 60
tickFps[1] = 77
tickFps[2] = 115
tickFps[3] = 125

// visible to the vars watcher
export var composeFPS = 0
export var beatHz = 0
export var frameIndex = 0

export function beforeRender(delta) {
  // delta is milliseconds since the last COMPOSED frame, so 1000/delta is
  // the compose rate. Clamped so a stall or a zero cannot poison the EMA.
  var dt = clamp(delta, 0.1, 100)
  emaFps = emaFps + (1000 / dt - emaFps) * 0.1
  composeFPS = emaFps

  // The reference blink: how far the slider's guess is from the compose
  // rate. When the guess is right this equals the stumble rate.
  beatHz = clamp(abs(emaFps - guessFps), 0.2, 25)
  refPhase = mod(refPhase + (dt * beatHz) / 1000, 1)
}

export function renderFrame() {
  var W = gridWidth()
  var H = gridHeight()

  // No grid (or a fixture too shallow to band): the strobe alone, which is
  // the whole measurement anyway. Everything else is a readout.
  if (W < 1 || H < 4) {
    strobeBrush()
    fill()
    frameStep()
    return
  }

  // Band heights. On a 64-row panel: 48 / 8 / 4 / 4.
  var refRows = max(1, floor(H / 16))
  var parRows = max(1, floor(H / 16))
  var barRows = max(1, floor(H / 8))
  var strobeRows = H - refRows - parRows - barRows
  if (strobeRows < 1) { strobeRows = 1; barRows = 1; parRows = 1; refRows = max(1, H - 3) }

  var barTop = strobeRows
  var parTop = barTop + barRows
  var refTop = parTop + parRows

  clear()

  // --- the strobe field -------------------------------------------------
  strobeBrush()
  fillRange(0, barTop * W)

  // --- compose-fps bar, 2 fps per column, ticks on its top row ----------
  var barCols = clamp(floor((emaFps * W) / 128 + 0.5), 0, W)
  var barFirst = barTop
  if (barRows > 1) {
    barFirst = barTop + 1
    for (var t = 0; t < 4; t++) {
      if (tickFps[t] == 115) hsv(0.85, 1, 1)      // the rate under test
      else hsv(0.5, 1, 0.8)
      setPixel(barTop * W + floor((tickFps[t] * W) / 128))
    }
  }
  hsv(0.08, 1, 1)
  for (var r = barFirst; r < parTop; r++) fillRange(r * W + 0, r * W + barCols)

  // --- frame-parity halves, dim ----------------------------------------
  var half = floor(W / 2)
  hsv(0, 0, 0.25)
  var x0 = frameParity ? half : 0
  var x1 = frameParity ? W : half
  for (var q = parTop; q < refTop; q++) fillRange(q * W + x0, q * W + x1)

  // --- beat reference ---------------------------------------------------
  if (refPhase < 0.5) {
    hsv(0.6, 1, 1)
    fillRange(refTop * W, H * W)
  }

  frameStep()
}

// Red on even composed frames, green on odd. `contrast` only scales the
// value — the alternation itself is what carries the measurement.
function strobeBrush() {
  if (frameParity) rgb(0, contrast, 0)
  else rgb(contrast, 0, 0)
}

// One step per COMPOSED frame. The parity is its own toggle rather than a
// bit of frameIndex so that the counter's roll cannot skip a flip.
function frameStep() {
  frameParity = 1 - frameParity
  frameIndex = frameIndex >= 30000 ? 0 : frameIndex + 1
}

// --- controls -------------------------------------------------------------

// Your guess at the displayed rate: the reference band blinks at the
// difference between it and the measured compose rate.
//# min=60 max=130 step=0.5 default=115
export function sliderDisplayedFPS(v) { guessFps = clamp(v, 1, 500) }

// Strobe brightness. Drop it if the base red/green alternation (compose/2,
// around 60 Hz) is distracting; the beat survives at any contrast.
//# min=0.2 max=1 step=0.05 default=1
export function sliderContrast(v) { contrast = clamp(v, 0.05, 1) }

// setFrameRate() cap, 0 = uncapped. Quantized to 125/n by the firmware's
// 8 ms render-loop pacing — see the header.
//# min=0 max=125 step=5 default=0
export function sliderComposeCap(v) {
  composeCap = clamp(floor(v), 0, 125)
  setFrameRate(composeCap)
}
