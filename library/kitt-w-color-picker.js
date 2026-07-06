// name: KITT (w/ color picker)
// Clean-room reimplementation from a prose functional description of the
// community pattern "KITT (w/ color picker)"; original source never consulted.

// Knight Rider scanner: one bright dot of a user-picked hue sweeps back and
// forth, bouncing off the strip ends, dragging a short decaying comet tail.
// Sweep speed scales with pixelCount so a full pass is the same wall-clock
// time on any strip (~0.4 s); the tail dies in about a tenth of that.

var SWEEP_MS = 400          // full end-to-end pass duration
var FADE_PER_MS = 0.01      // full brightness -> dark in ~100 ms

var trail = array(pixelCount)
var head = 0
var dir = 1

// Only the hue of the picked color is used (faithful to the original quirk:
// saturation is forced full and brightness comes from the trail).
var hue = 0
var pickedS = 1
var pickedV = 1
export function hsvPickerColor(h, s, v) {
  hue = h
  pickedS = s
  pickedV = v
}

export function beforeRender(delta) {
  var last = floor(head)

  // advance; speed proportional to pixelCount for length-independent timing
  head += dir * delta * pixelCount / SWEEP_MS
  if (head >= pixelCount - 1) { head = pixelCount - 1; dir = -1 }
  if (head <= 0) { head = 0; dir = 1 }

  // decay everything first, then stamp the freshly-visited pixels at full
  var fade = delta * FADE_PER_MS
  for (var i = 0; i < pixelCount; i++) {
    trail[i] = max(0, trail[i] - fade)
  }

  // gap-fill: light every integer pixel between last frame's head and this
  // frame's, so fast sweeps leave no holes in the tail
  var cur = floor(head)
  var step = cur >= last ? 1 : -1
  var p = last
  while (true) {
    trail[p] = 1
    if (p == cur) break
    p += step
  }
}

export function render(index) {
  var v = trail[index]
  hsv(hue, 1, v * v * v)   // cubed for a hot core with a snappy falloff
}
