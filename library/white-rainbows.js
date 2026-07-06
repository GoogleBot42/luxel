// name: White Rainbows
// Clean-room reimplementation from a prose functional description of the
// community pattern "White Rainbows"; original source never consulted.

// Counter-flowing comet heads stamp near-white dots into two per-pixel
// persistence layers (saturation and brightness). Each frame saturation
// compounds back up — white head resaturating into a vivid tail — while
// brightness decays away exponentially. Rainbow mode draws tails from a
// many-times-repeated rainbow along the strip; off, everything shares one
// slowly drifting hue.

var satBuf = array(pixelCount)
var briBuf = array(pixelCount)

var rainbow = 1
//# min=0 max=1 step=1 default=1
export function sliderRainbow(v) {
  rainbow = v > 0.5
}

var heads = 5
//# min=0 max=1 step=0.01 default=0.4
export function sliderNumber(v) {
  // floored at one head per direction so the bottom of travel isn't dark
  heads = max(1, floor(v * 12.99))
}

var hueT = 0

function stamp(p) {
  var idx = floor(p * pixelCount)
  briBuf[idx] = 1
  satBuf[idx] = 0.2   // nearly white head
}

export function beforeRender(delta) {
  var fwd = time(0.07)   // ~4.6 s to traverse the strip
  var rev = 1 - fwd      // the mirrored, opposite-direction set
  // independent hue drift, a few times faster in rainbow mode
  hueT = rainbow ? time(0.03) : time(0.1)

  for (var i = 0; i < heads; i++) {
    var off = i / heads
    stamp(frac(fwd + off))
    stamp(frac(rev + off))
  }
}

export function render(index) {
  var h = hueT + (rainbow ? index / pixelCount * 18 : 0)

  // frame-based decays, faithful to the original (tail length tracks the
  // frame rate); saturation is clamped at 1 so 16.16 math can't wrap
  var s = min(1, satBuf[index] * 1.3)
  satBuf[index] = s
  var b = briBuf[index] * 0.94
  briBuf[index] = b

  hsv(h, s, b)
}
