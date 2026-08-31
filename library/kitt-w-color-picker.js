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
var FADE_MS = 160  // full brightness -> dark; ~40% of a pass, a comet tail

export function beforeRender(delta) {
  // Decay FIRST, then stamp the head, so the head pixel always renders at a
  // full value of 1. (Stamping before the decay pass docked the head by a
  // whole frame's worth of fade — at a 50 ms frame that left the "hot" pixel
  // at 0.375, and cubed, a barely-visible 5% brightness.)
  var fade = delta / FADE_MS
  var i
  for (i = 0; i < pixelCount; i++) {
    trail[i] -= fade
    if (trail[i] < 0) trail[i] = 0
  }

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
  // stamp every integer pixel between last frame's head and this frame's --
  // but ramp the stamped value across the span (oldest = one frame's worth of
  // fade already applied) instead of flat-topping them all at 1. Without the
  // ramp the tail is a stack of hard-edged blocks, one per frame.
  var cur = floor(head)
  var step = cur >= last ? 1 : -1
  var n = abs(cur - last)
  var j = 0
  i = last
  while (true) {
    var age = n > 0 ? 1 - j / n : 0     // 1 at `last` (oldest), 0 at `cur`
    var val = 1 - fade * age
    if (val > trail[i]) trail[i] = val
    if (i == cur) break
    i += step
    j++
  }
}

export function render(index) {
  var v = trail[index]
  // Squared falloff: hot white-bright core with a visible comet tail behind
  // it. (Cubed was too aggressive — the scanner read as almost black.)
  hsv(hue, 1, v * v)
}
