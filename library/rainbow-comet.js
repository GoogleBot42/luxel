// name: Rainbow Comet
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rainbow Comet"; original source never consulted.

// A near-white comet head bounces back and forth along the strip (triangle
// wave, no wrap). Behind it a trail fades in brightness while its hue walks
// through neighbouring colours, so it blooms from whitish head into the
// head's colour and drifts through the rainbow as it dims. The stamped base
// hue also drifts slowly (~10 s per wheel), so each pass lays down a
// different part of the spectrum. Per-pixel state is mutated inside render.

var bri = array(pixelCount)   // stored (linear) brightness per pixel
var hue = array(pixelCount)   // hue per pixel
var sat = array(pixelCount)   // saturation per pixel
var lastHead = 0

var speed = 0.5   // 0..1, faster = quicker bounce
var fade = 0.5    // 0..1, higher = faster fade / shorter tail

var headInterval = 0.09
var decay = 0.9

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  speed = v
  // centered on the shipped 0.09 (~5.9 s per full bounce); ~9.8 s .. ~2.0 s
  headInterval = 0.09 - (v - 0.5) * 0.12
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderFade(v) {
  fade = v
  // centered on the shipped 0.9; higher slider => lower decay => faster fade
  decay = 0.9 - (v - 0.5) * 0.14
}

export function beforeRender(delta) {
  var baseHue = time(0.15)                              // ~10 s wheel drift
  var head = floor(triangle(time(headInterval)) * (pixelCount - 1))

  var lo = min(head, lastHead)
  var hi = max(head, lastHead)
  // guard against a bogus huge jump covering essentially the whole strip
  if (hi - lo < pixelCount * 0.75) {
    for (var i = lo; i <= hi; i++) {
      bri[i] = 1
      hue[i] = baseHue
      sat[i] = 0.35   // noticeably below full -> whitish head
    }
  }
  lastHead = head
}

// One `renderFrame` instead of one `render` call per LED (docs/bulk-render.md).
//
// Not a `fillHSV`, for two reasons. The value channel is `bri[i] * bri[i]`,
// not `bri` — a fourth `array(pixelCount)` for it would drop the pattern's
// pixel ceiling from 3,408 to about 2,556 against the array budget (#420), and
// squaring `bri` in place instead is not bit-exact, because the stored value is
// re-multiplied by `decay` every frame and `(b * decay)^2` drifts from
// `b^2 * decay^2` over a tail's ~37 frames of 16.16 rounding. And the per-pixel
// body is not a read-out at all: it EVOLVES this pixel's state for the next
// frame, which no single bulk op expresses (#373 proposes the two arms that
// would — a scalar array-add for the hue smear and an array clamp for the
// saturation cure).
//
// What makes the loop worth it anyway is the two things it CAN hand to the
// engine. The brightness fade is one `feedback(bri, decay)` after the pass —
// exact, since each element is read before the whole array is scaled once — and
// dead pixels are skipped entirely, which on a strip is most of them between
// passes of the head. A naive port with neither measured 0.81x against the
// per-pixel `render` it replaces; this one wins. Index space: no map is used
// or needed.
export function renderFrame() {
  clear()                              // renderFrame does NOT clear between
                                       // frames, and the loop below skips dead
                                       // pixels — without this they would keep
                                       // last frame's colour forever
  var i, b
  for (i = 0; i < pixelCount; i++) {
    b = bri[i]
    if (b == 0) continue               // already black from clear(), and its
                                       // hue/sat are overwritten wholesale when
                                       // the head next stamps it
    hsv(hue[i], sat[i], b * b)         // squared for a snappier tail
    setPixel(i)
    hue[i] -= 0.004                    // smear the tail through the rainbow
    sat[i] = min(sat[i] * 1.06, 1)     // head "cures" up to full sat
  }
  feedback(bri, decay)                 // exponential brightness fade, natively
}
