// name: KITT (w/ color picker)
// Clean-room reimplementation from a prose functional description of the
// community pattern "KITT (w/ color picker)"; original source never consulted.

// A single dot of a user-chosen hue sweeps back and forth, dragging a short
// decaying comet tail. Sweep speed scales with pixelCount so a full pass takes
// the same wall-clock time (~0.4 s) on any strip length.

var trail = array(pixelCount)
var head = 0
var dir = 1

// Only hue is used: saturation is forced to 1 and value is the trail.
var hue = 0
export function hsvPickerColor(h, s, v) {
  hue = h
}

var SWEEP_MS = 400 // end-to-end pass time
var FADE_MS = 80   // full brightness -> dark

export function beforeRender(delta) {
  var last = floor(head)

  // Advance the head; speed in pixels/ms is proportional to strip length.
  head += dir * delta * pixelCount / SWEEP_MS
  if (head > pixelCount - 1) {
    head = pixelCount - 1
    dir = -1
  }
  if (head < 0) {
    head = 0
    dir = 1
  }

  // Gap-fill: at this speed the head can jump several pixels per frame, so
  // stamp full brightness on every integer pixel between last frame's head
  // and this frame's.
  var cur = floor(head)
  var step = cur >= last ? 1 : -1
  var i = last
  while (true) {
    trail[i] = 1
    if (i == cur) break
    i += step
  }

  // Linear decay, clamped at zero.
  var fade = delta / FADE_MS
  for (i = 0; i < pixelCount; i++) {
    trail[i] -= fade
    if (trail[i] < 0) trail[i] = 0
  }
}

export function render(index) {
  var v = trail[index]
  // Cubing sharpens the falloff: hot core, fast perceptual fade.
  hsv(hue, 1, v * v * v)
}
