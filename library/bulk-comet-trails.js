// name: Bulk Comet Trails
// Curated example (hand-written showcase of the Luxel language/builtins).
// Decay trails with NO buffers at all. The frame the whole-frame entry draws
// into PERSISTS between frames, so `fade(k)` dims last frame's picture and
// three `setPixel()` calls put this frame's comet heads on top — the classic
// KITT/comet loop without the `array(pixelCount)` every per-pixel version of
// it needs (32 KB of it on a 4096-pixel panel). State here is six numbers.
//
// The bounce is a `triangle()` of a steadily advancing phase, which is the
// same thing as a point moving at constant speed and reflecting off both
// ends — so the comets need no velocity variables either.

var speed = 1
var decay = 0.88
var ph1 = 0
var ph2 = 0.33
var ph3 = 0.66
var age = 0

export function beforeRender(delta) {
  var dt = delta * 0.001
  age = mod(age + dt * 0.03, 1)
  ph1 = mod(ph1 + dt * 0.50 * speed, 1)
  ph2 = mod(ph2 + dt * 0.37 * speed, 1)
  ph3 = mod(ph3 + dt * 0.61 * speed, 1)
}

export function renderFrame() {
  fade(decay)                     // last frame, dimmer — the whole trail
  var last = pixelCount - 1
  hsv(age, 1, 1)                  // hsv() sets the "brush" the bulk ops use
  setPixel(triangle(ph1) * last)
  hsv(age + 0.33, 1, 1)
  setPixel(triangle(ph2) * last)
  hsv(age + 0.66, 1, 1)
  setPixel(triangle(ph3) * last)
}

//# min=0 max=3 step=0.05 default=1
export function sliderSpeed(v) { speed = v }

//# min=0.5 max=0.99 step=0.01 default=0.88
export function sliderTrail(v) { decay = v }
