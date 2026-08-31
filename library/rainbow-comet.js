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
  headInterval = 0.13 - v * 0.11   // ~8.5 s .. ~1.3 s per full bounce
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderFade(v) {
  fade = v
  decay = 0.97 - v * 0.15          // higher slider => lower decay => faster fade
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

export function render(index) {
  var b = bri[index]
  hsv(hue[index], sat[index], b * b)   // squared for a snappier tail

  // decay / evolve this pixel's state for next frame
  hue[index] -= 0.004                  // smear the tail through the rainbow
  sat[index] = min(sat[index] * 1.06, 1)   // head "cures" up to full sat
  bri[index] = b * decay               // exponential brightness fade
}
