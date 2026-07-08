// name: KITT
// Clean-room reimplementation from a prose functional description of the
// community pattern "KITT"; original source never consulted.
// A single bright red eye sweeps end-to-end (fixed ~0.9 s wall-clock time
// regardless of strip length), bounces, and leaves a red comet tail that
// fades to black over ~1.5 s. Value is cubed for a perceptually sharp comet.

var trail = array(1)   // resized to pixelCount on first frame
var head = 0           // fractional head position (pixels)
var dir = 1            // +1 / -1 sweep direction
var sized = 0

export function beforeRender(delta) {
  if (!sized) {
    trail = array(pixelCount)
    sized = 1
  }

  // Speed proportional to strip length -> constant wall-clock sweep time.
  // pixelCount pixels per (SWEEP_MS) milliseconds, one way.
  var speed = pixelCount / 900   // pixels per ms (~0.9 s end-to-end)
  head += dir * speed * delta

  if (head >= pixelCount - 1) {
    head = pixelCount - 1
    dir = -1
  } else if (head <= 0) {
    head = 0
    dir = 1
  }

  // Stamp the pixel under the head to full brightness.
  var hi = floor(head)
  if (hi >= 0 && hi < pixelCount) trail[hi] = 1

  // Linear decay: full -> black in ~1.5 s.
  var decay = delta / 1500
  for (var i = 0; i < pixelCount; i++) {
    var b = trail[i] - decay
    trail[i] = b < 0 ? 0 : b
  }
}

export function render(index) {
  var b = trail[index]
  hsv(0, 1, b * b * b)
}
