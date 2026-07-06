// name: index walk
// Clean-room reimplementation from a prose functional description of the
// community pattern "index walk"; original source never consulted.

// A single lit pixel marches from the start of the strip to the end,
// then snaps back and repeats. Its hue cycles through the full rainbow
// in well under a second and also shifts with position. Everything else
// is written explicitly black (unwritten pixels would keep their old
// color). Unlike the original, the walk is time-based rather than
// per-frame, so speed is frame-rate independent.

var SPEED = 5           // pixels per second
var cursor = 0          // fractional pixel position
var huePhase = 0

export function beforeRender(delta) {
  huePhase = time(0.012)              // full rainbow in ~0.8 s
  cursor += SPEED * delta / 1000
  if (cursor >= pixelCount) cursor = 0
}

export function render(index) {
  if (index == floor(cursor)) {
    hsv(huePhase + index / pixelCount, 1, 1)
  } else {
    rgb(0, 0, 0)
  }
}
