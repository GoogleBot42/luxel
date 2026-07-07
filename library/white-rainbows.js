// name: White Rainbows
// Clean-room reimplementation from a prose functional description of the
// community pattern "White Rainbows"; original source never consulted.

// Near-white comet heads sweep both directions along the strip, leaving
// trails that re-saturate from white into vivid color and fade out.
// Per-pixel saturation and brightness buffers act as decaying paint layers.

var satBuf = array(pixelCount)
var briBuf = array(pixelCount)

var RAINBOW_REPEATS = 18   // rainbow repetitions along the strip (stylistic)
var HEAD_SAT = 0.2         // freshly stamped head: nearly white
var SAT_REGROW = 1.32      // per-frame saturation compounding (white -> color)
var BRI_DECAY = 0.94       // per-frame exponential fade of the tail

// Rainbow mode: slider used as a toggle, on above midpoint. Default on.
var rainbow = 1
//# min=0 max=1 step=1 default=1
export function sliderRainbow(v) {
  rainbow = v > 0.5
}

// Comets per direction. Floored at one so the strip never goes dark.
var comets = 5
//# min=0 max=1 step=0.01 default=0.42
export function sliderNumber(v) {
  comets = max(1, floor(v * 12))
}

var hueDrift = 0

export function beforeRender(delta) {
  var fwd = time(0.08)          // ~5.2 s to traverse the strip
  var rev = 1 - fwd
  // hue drift: several seconds per cycle, a few times faster in rainbow mode
  hueDrift = rainbow ? time(0.04) : time(0.12)

  // Stamp each comet head (both directions) into the paint buffers.
  for (var i = 0; i < comets; i++) {
    var off = i / comets
    var idx = floor(((fwd + off) % 1) * pixelCount)
    briBuf[idx] = 1
    satBuf[idx] = HEAD_SAT
    idx = floor(((rev + off) % 1) * pixelCount)
    briBuf[idx] = 1
    satBuf[idx] = HEAD_SAT
  }
}

export function render(index) {
  var h = hueDrift
  if (rainbow) h += index / pixelCount * RAINBOW_REPEATS

  var s = satBuf[index]
  satBuf[index] = min(1, s * SAT_REGROW)   // compound back to full color
  var v = briBuf[index]
  briBuf[index] = v * BRI_DECAY            // exponential tail fade

  hsv(h, s, v)
}
